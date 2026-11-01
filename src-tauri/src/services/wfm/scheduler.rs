//! Global budget for background WFM reads (port of WFHelper's `wfmScheduler`): a token bucket under
//! the documented 3 req/s, a concurrency cap, and a global gate after a 429.
//!
//! Nothing here retries: `wfm/client.rs` decided against it, and a gate is what stops a 429 loop.
//! Callers get the error and fall back to their cache.

use std::future::Future;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::time::{Instant, sleep};

use super::errors::WfmError;

/// `wfm-api.md` §3 documents 3/s; stay under it.
const RATE_PER_SECOND: f64 = 2.5;
const BURST: f64 = 5.0;
const MAX_CONCURRENT: usize = 3;
/// Background never takes the last slot, so a user's request waits for the budget, not a sweep.
const MAX_BACKGROUND: usize = 2;
const MAX_QUEUE: usize = 64;
const DEFAULT_GATE: Duration = Duration::from_secs(30);
const MIN_GATE: Duration = Duration::from_secs(1);
const MAX_GATE: Duration = Duration::from_secs(300);
/// Holding a user's call longer than this is worse than the rate-limit error it would get.
const MAX_INTERACTIVE_WAIT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Priority {
    Interactive,
    Background,
}

struct Bucket {
    tokens: f64,
    refilled: Instant,
    gate_until: Option<Instant>,
}

pub struct Scheduler {
    bucket: Mutex<Bucket>,
    slots: Semaphore,
    background: Semaphore,
    queued: AtomicUsize,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            bucket: Mutex::new(Bucket {
                tokens: BURST,
                refilled: Instant::now(),
                gate_until: None,
            }),
            slots: Semaphore::new(MAX_CONCURRENT),
            background: Semaphore::new(MAX_BACKGROUND),
            queued: AtomicUsize::new(0),
        }
    }
}

impl Scheduler {
    /// How long until a token is free; takes it when it is now.
    fn take_token(&self) -> Option<Duration> {
        let mut b = self.bucket.lock().unwrap();
        let now = Instant::now();
        if let Some(left) = b
            .gate_until
            .and_then(|g| g.checked_duration_since(now))
            .filter(|d| !d.is_zero())
        {
            return Some(left);
        }
        let gained = now.saturating_duration_since(b.refilled).as_secs_f64() * RATE_PER_SECOND;
        b.tokens = (b.tokens + gained).min(BURST);
        b.refilled = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            return None;
        }
        Some(Duration::from_secs_f64((1.0 - b.tokens) / RATE_PER_SECOND))
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
        if self.queued.fetch_add(1, Ordering::SeqCst) >= MAX_QUEUE {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            return Err(WfmError::invalid("WFM request queue full"));
        }
        let result = self.run_admitted(priority, f).await;
        self.queued.fetch_sub(1, Ordering::SeqCst);
        result
    }

    async fn run_admitted<T, Fut>(
        &self,
        priority: Priority,
        f: impl FnOnce() -> Fut,
    ) -> Result<T, WfmError>
    where
        Fut: Future<Output = Result<T, WfmError>>,
    {
        let closed = || WfmError::invalid("scheduler closed");
        let _bg = match priority {
            Priority::Background => Some(self.background.acquire().await.map_err(|_| closed())?),
            Priority::Interactive => None,
        };
        let _slot = self.slots.acquire().await.map_err(|_| closed())?;
        while let Some(wait) = self.take_token() {
            if priority == Priority::Interactive && wait > MAX_INTERACTIVE_WAIT {
                return Err(WfmError::RateLimited {
                    retry_after_secs: Some(wait.as_secs().max(1) as u32),
                });
            }
            sleep(wait).await;
        }
        let result = f().await;
        if let Err(WfmError::RateLimited { retry_after_secs }) = &result {
            self.arm_gate(*retry_after_secs);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
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
}
