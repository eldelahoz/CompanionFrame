//! The one budget for every WFM request (port of WFHelper's `wfmScheduler`): a token bucket under
//! the documented 3 req/s, a concurrency cap served by priority, a second small bucket for contract
//! search, and a global gate after a 429.
//!
//! Nothing here retries: `wfm/client.rs` decided against it, and a gate is what stops a 429 loop.
//! Callers get the error and fall back to their cache.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::time::{Instant, sleep};

use super::errors::WfmError;

/// `wfm-api.md` §3 documents 3/s; stay under it.
const RATE_PER_SECOND: f64 = 2.5;
const BURST: f64 = 5.0;
/// Contract search is documented at ~10-20 a minute; stay at 12.
const CONTRACT_RATE: f64 = 12.0 / 60.0;
const CONTRACT_BURST: f64 = 3.0;
const MAX_CONCURRENT: usize = 3;
/// Background never takes the last slot, so a user's request waits for the budget, not a sweep.
const MAX_BACKGROUND: usize = 2;
const MAX_QUEUE: usize = 64;
const DEFAULT_GATE: Duration = Duration::from_secs(30);
const MIN_GATE: Duration = Duration::from_secs(1);
const MAX_GATE: Duration = Duration::from_secs(300);
/// Holding a user's call longer than this is worse than the rate-limit error it would get.
const MAX_INTERACTIVE_WAIT: Duration = Duration::from_secs(10);

/// Order of service is the declaration order: a click, then a bulk run the user started, then sweeps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Priority {
    Interactive,
    /// A bulk run the user started (hide all, workbench): may wait, never fast-fails.
    Job,
    Background,
}

const ORDER: [Priority; 3] = [Priority::Interactive, Priority::Job, Priority::Background];

/// Contract search has its own, much lower budget.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Normal,
    Contract,
}

tokio::task_local! {
    static CURRENT: Priority;
}

/// Priority for requests made in this task; a plain user call when nobody set one.
pub fn current() -> Priority {
    CURRENT.try_with(|p| *p).unwrap_or(Priority::Interactive)
}

/// Runs `f` so every `WfmClient::request` inside it is scheduled at `priority`.
pub async fn scoped<F: Future>(priority: Priority, f: F) -> F::Output {
    CURRENT.scope(priority, f).await
}

struct Bucket {
    tokens: f64,
    refilled: Instant,
    gate_until: Option<Instant>,
}

impl Bucket {
    fn new(burst: f64) -> Mutex<Self> {
        Mutex::new(Self {
            tokens: burst,
            refilled: Instant::now(),
            gate_until: None,
        })
    }

    /// How long until a token is free; takes it when it is now.
    fn take(&mut self, rate: f64, burst: f64) -> Option<Duration> {
        let now = Instant::now();
        if let Some(left) = self
            .gate_until
            .and_then(|g| g.checked_duration_since(now))
            .filter(|d| !d.is_zero())
        {
            return Some(left);
        }
        let gained = now.saturating_duration_since(self.refilled).as_secs_f64() * rate;
        self.tokens = (self.tokens + gained).min(burst);
        self.refilled = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return None;
        }
        Some(Duration::from_secs_f64((1.0 - self.tokens) / rate))
    }
}

#[derive(Default)]
struct Slots {
    in_use: usize,
    background: usize,
    /// One FIFO per priority, indexed like `ORDER`.
    waiters: [VecDeque<oneshot::Sender<Permit>>; 3],
}

fn can_grant(s: &Slots, p: Priority) -> bool {
    s.in_use < MAX_CONCURRENT && (p != Priority::Background || s.background < MAX_BACKGROUND)
}

fn grant(slots: &Arc<Mutex<Slots>>, s: &mut Slots, priority: Priority) -> Permit {
    s.in_use += 1;
    if priority == Priority::Background {
        s.background += 1;
    }
    Permit {
        slots: slots.clone(),
        priority,
    }
}

/// A concurrency slot; giving it back hands it to the most urgent waiter.
struct Permit {
    slots: Arc<Mutex<Slots>>,
    priority: Priority,
}

impl Drop for Permit {
    fn drop(&mut self) {
        {
            let mut s = self.slots.lock().unwrap();
            s.in_use -= 1;
            if self.priority == Priority::Background {
                s.background -= 1;
            }
        }
        dispatch(&self.slots);
    }
}

/// Hands free slots to waiters, most urgent first. Sends happen outside the lock: a waiter that gave
/// up returns its permit, whose drop dispatches again.
fn dispatch(slots: &Arc<Mutex<Slots>>) {
    let mut ready = Vec::new();
    {
        let mut s = slots.lock().unwrap();
        'again: loop {
            for (idx, p) in ORDER.into_iter().enumerate() {
                while s.waiters[idx].front().is_some_and(|w| w.is_closed()) {
                    s.waiters[idx].pop_front();
                }
                if s.waiters[idx].front().is_some() && can_grant(&s, p) {
                    let tx = s.waiters[idx].pop_front().unwrap();
                    ready.push((tx, grant(slots, &mut s, p)));
                    continue 'again;
                }
            }
            break;
        }
    }
    for (tx, permit) in ready {
        let _ = tx.send(permit);
    }
}

pub struct Scheduler {
    bucket: Mutex<Bucket>,
    contracts: Mutex<Bucket>,
    slots: Arc<Mutex<Slots>>,
    queued: AtomicUsize,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            bucket: Bucket::new(BURST),
            contracts: Bucket::new(CONTRACT_BURST),
            slots: Arc::default(),
            queued: AtomicUsize::new(0),
        }
    }
}

impl Scheduler {
    async fn acquire(&self, priority: Priority) -> Result<Permit, WfmError> {
        let idx = priority as usize;
        let rx = {
            let mut s = self.slots.lock().unwrap();
            if can_grant(&s, priority) && s.waiters[..=idx].iter().all(VecDeque::is_empty) {
                return Ok(grant(&self.slots, &mut s, priority));
            }
            let (tx, rx) = oneshot::channel();
            s.waiters[idx].push_back(tx);
            rx
        };
        rx.await.map_err(|_| WfmError::invalid("scheduler closed"))
    }

    /// Tests that script a 429 and then expect the next call to go out.
    #[cfg(test)]
    pub(super) fn lift_gate(&self) {
        self.bucket.lock().unwrap().gate_until = None;
    }

    fn arm_gate(&self, retry_after_secs: Option<u32>) {
        let len = retry_after_secs
            .map_or(DEFAULT_GATE, |s| Duration::from_secs(s.into()))
            .clamp(MIN_GATE, MAX_GATE);
        let until = Instant::now() + len;
        let mut b = self.bucket.lock().unwrap();
        if b.gate_until.is_none_or(|g| until > g) {
            b.gate_until = Some(until);
            // Tokens must not accrue across the gate, or the full bucket spends the moment it lifts.
            b.refilled = until;
            b.tokens = b.tokens.min(1.0);
        }
    }

    /// Runs `f` under the budget. A 429 from `f` raises the global gate; nobody retries.
    pub async fn run<T, Fut>(
        &self,
        priority: Priority,
        f: impl FnOnce() -> Fut,
    ) -> Result<T, WfmError>
    where
        Fut: Future<Output = Result<T, WfmError>>,
    {
        self.run_kind(priority, Kind::Normal, f).await
    }

    pub async fn run_kind<T, Fut>(
        &self,
        priority: Priority,
        kind: Kind,
        f: impl FnOnce() -> Fut,
    ) -> Result<T, WfmError>
    where
        Fut: Future<Output = Result<T, WfmError>>,
    {
        if self.queued.fetch_add(1, Ordering::SeqCst) >= MAX_QUEUE {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            return Err(WfmError::invalid("WFM request queue full"));
        }
        let result = self.run_admitted(priority, kind, f).await;
        self.queued.fetch_sub(1, Ordering::SeqCst);
        result
    }

    /// Waits for a token of `bucket`; only a user's call gives up on a long wait.
    async fn wait_token(
        bucket: &Mutex<Bucket>,
        rate: f64,
        burst: f64,
        priority: Priority,
    ) -> Result<(), WfmError> {
        loop {
            let Some(wait) = bucket.lock().unwrap().take(rate, burst) else {
                return Ok(());
            };
            if priority == Priority::Interactive && wait > MAX_INTERACTIVE_WAIT {
                return Err(WfmError::RateLimited {
                    retry_after_secs: Some(wait.as_secs().max(1) as u32),
                });
            }
            sleep(wait).await;
        }
    }

    async fn run_admitted<T, Fut>(
        &self,
        priority: Priority,
        kind: Kind,
        f: impl FnOnce() -> Fut,
    ) -> Result<T, WfmError>
    where
        Fut: Future<Output = Result<T, WfmError>>,
    {
        let _slot = self.acquire(priority).await?;
        if kind == Kind::Contract {
            Self::wait_token(&self.contracts, CONTRACT_RATE, CONTRACT_BURST, priority).await?;
        }
        Self::wait_token(&self.bucket, RATE_PER_SECOND, BURST, priority).await?;
        let result = f().await;
        if let Err(WfmError::RateLimited { retry_after_secs }) = &result {
            self.arm_gate(*retry_after_secs);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use super::*;

    impl Scheduler {
        fn gate_left(&self) -> Option<Duration> {
            let b = self.bucket.lock().unwrap();
            b.gate_until
                .and_then(|g| g.checked_duration_since(Instant::now()))
                .filter(|d| !d.is_zero())
        }
        fn gated(&self) -> bool {
            self.gate_left().is_some()
        }
    }

    async fn ok() -> Result<u8, WfmError> {
        Ok(1)
    }

    #[tokio::test(start_paused = true)]
    async fn a_cold_burst_then_the_sustained_rate() {
        let s = Scheduler::default();
        let t0 = Instant::now();
        for _ in 0..5 {
            s.run(Priority::Interactive, ok).await.unwrap();
        }
        assert!(t0.elapsed() < Duration::from_millis(50), "burst is free");
        for _ in 0..5 {
            s.run(Priority::Interactive, ok).await.unwrap();
        }
        // 5 more at 2.5/s = 2 s, never faster than the documented 3/s.
        assert!(t0.elapsed() >= Duration::from_millis(1900));
    }

    #[tokio::test(start_paused = true)]
    async fn caps_concurrency() {
        let s = Arc::new(Scheduler::default());
        let (now, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let tasks = (0..8).map(|_| {
            let (s, now, peak) = (s.clone(), now.clone(), peak.clone());
            async move {
                s.run(Priority::Interactive, || async {
                    let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(n, Ordering::SeqCst);
                    sleep(Duration::from_millis(100)).await;
                    now.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
            }
        });
        for r in futures_util::future::join_all(tasks).await {
            r.unwrap();
        }
        assert_eq!(peak.load(Ordering::SeqCst), MAX_CONCURRENT);
    }

    #[tokio::test(start_paused = true)]
    async fn background_leaves_a_slot_for_the_user() {
        let s = Arc::new(Scheduler::default());
        let (now, peak) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let tasks = (0..6).map(|_| {
            let (s, now, peak) = (s.clone(), now.clone(), peak.clone());
            async move {
                s.run(Priority::Background, || async {
                    let n = now.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(n, Ordering::SeqCst);
                    sleep(Duration::from_millis(100)).await;
                    now.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
            }
        });
        futures_util::future::join_all(tasks).await;
        assert_eq!(peak.load(Ordering::SeqCst), MAX_BACKGROUND);
    }

    #[tokio::test(start_paused = true)]
    async fn a_429_gates_everyone_for_30s_without_retrying() {
        let s = Scheduler::default();
        let calls = AtomicUsize::new(0);
        let r = s
            .run(Priority::Interactive, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err::<u8, _>(WfmError::RateLimited {
                    retry_after_secs: None,
                })
            })
            .await;
        assert!(matches!(r, Err(WfmError::RateLimited { .. })));
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the scheduler never replays"
        );
        assert!(s.gated());
        // A user's call is refused at once rather than parked for 30 s...
        let t0 = Instant::now();
        let r = s.run(Priority::Interactive, ok).await;
        assert!(matches!(r, Err(WfmError::RateLimited { .. })));
        assert!(t0.elapsed() < Duration::from_secs(1));
        // ...while background work waits the gate out.
        s.run(Priority::Background, ok).await.unwrap();
        assert!(t0.elapsed() >= Duration::from_secs(29));
        assert!(!s.gated());
    }

    #[tokio::test(start_paused = true)]
    async fn the_server_stated_wait_sets_the_gate() {
        let s = Scheduler::default();
        let _ = s
            .run(Priority::Interactive, || async {
                Err::<u8, _>(WfmError::RateLimited {
                    retry_after_secs: Some(60),
                })
            })
            .await;
        let left = s.gate_left().unwrap();
        assert!(left > Duration::from_secs(55) && left <= Duration::from_secs(60));
    }

    #[tokio::test(start_paused = true)]
    async fn other_errors_do_not_gate() {
        let s = Scheduler::default();
        let _ = s
            .run(Priority::Interactive, || async {
                Err::<u8, _>(WfmError::Network { detail: "x".into() })
            })
            .await;
        assert!(!s.gated());
    }

    /// Starts `run` calls in a fixed arrival order and reports the order they got to run.
    async fn started_order(
        first: &[Priority],
        later: &[(&'static str, Priority)],
    ) -> Vec<&'static str> {
        let s = Arc::new(Scheduler::default());
        let log = Arc::new(Mutex::new(Vec::new()));
        // Fill every slot with a call that holds it for 100 ms.
        let holders: Vec<_> = first
            .iter()
            .map(|p| {
                let (s, p) = (s.clone(), *p);
                tokio::spawn(async move {
                    s.run(p, || async {
                        sleep(Duration::from_millis(100)).await;
                        Ok(())
                    })
                    .await
                })
            })
            .collect();
        sleep(Duration::from_millis(10)).await;
        let mut waiting = Vec::new();
        for (name, p) in later {
            let (s, log, name, p) = (s.clone(), log.clone(), *name, *p);
            waiting.push(tokio::spawn(async move {
                s.run(p, || async {
                    log.lock().unwrap().push(name);
                    Ok(())
                })
                .await
            }));
            sleep(Duration::from_millis(5)).await;
        }
        for h in holders.into_iter().chain(waiting) {
            h.await.unwrap().unwrap();
        }
        log.lock().unwrap().clone()
    }

    #[tokio::test(start_paused = true)]
    async fn the_most_urgent_waiter_is_served_first() {
        let order = started_order(
            &[Priority::Interactive; 3],
            &[
                ("bg", Priority::Background),
                ("job", Priority::Job),
                ("ui", Priority::Interactive),
            ],
        )
        .await;
        assert_eq!(order, ["ui", "job", "bg"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_job_waits_out_the_gate_where_a_click_fails_fast() {
        let s = Scheduler::default();
        let _ = s
            .run(Priority::Interactive, || async {
                Err::<u8, _>(WfmError::RateLimited {
                    retry_after_secs: Some(60),
                })
            })
            .await;
        let t0 = Instant::now();
        // One gate for all priorities: the click is refused, the job and the sweep wait.
        assert!(matches!(
            s.run(Priority::Interactive, ok).await,
            Err(WfmError::RateLimited { .. })
        ));
        s.run(Priority::Job, ok).await.unwrap();
        assert!(t0.elapsed() >= Duration::from_secs(59));
        s.run(Priority::Background, ok).await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn contract_search_has_its_own_slower_bucket() {
        let s = Scheduler::default();
        let t0 = Instant::now();
        for _ in 0..3 {
            s.run_kind(Priority::Job, Kind::Contract, ok).await.unwrap();
        }
        assert!(
            t0.elapsed() < Duration::from_millis(50),
            "contract burst is 3"
        );
        s.run_kind(Priority::Job, Kind::Contract, ok).await.unwrap();
        assert!(t0.elapsed() >= Duration::from_secs(4), "then 12 a minute");
        // Ordinary calls are not held back by it.
        let t1 = Instant::now();
        s.run(Priority::Job, ok).await.unwrap();
        assert!(t1.elapsed() < Duration::from_millis(50));
    }
}
