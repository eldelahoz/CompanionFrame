//! The polling state machine: when to run acquisition, what the last run says, fast retries for
//! local failures and the rescan after a game login. What a run does is behind `Backend`.

use crate::services::clock::now_ms_f64;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::{HelperRunReason, HelperStatus, next_poll_delay_ms, should_retry_after_game_login};

pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// EE.log prints the login line slightly before the game's auth string is in memory.
const LOGIN_RETRY_DELAY: Duration = Duration::from_millis(250);

pub type RunResult = (bool, Option<HelperRunReason>);
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Backend: Send + Sync + 'static {
    /// `Some(false)` only when a sample answered that the game is not running; unknown still runs.
    fn game_running(&self) -> Option<bool>;
    /// Epoch milliseconds of `inventory.json`.
    fn inventory_mtime_ms(&self) -> Option<f64>;
    fn exe_found(&self) -> bool;
    /// Re-discovers the helper binary; called when polling starts.
    fn refresh(&self);
    /// One acquisition attempt. Does not touch the runner's state.
    fn run_once(&self) -> BoxFuture<'_, RunResult>;
}

#[derive(Default)]
struct State {
    running: bool,
    last_run_at: Option<f64>,
    last_ok: Option<bool>,
    last_reason: Option<HelperRunReason>,
    poller: Option<JoinHandle<()>>,
    login_pending: bool,
    waiting_for_game: bool,
}

pub struct Runner {
    backend: Arc<dyn Backend>,
    rt: Handle,
    st: Mutex<State>,
    wake: Notify,
}

impl Runner {
    pub fn new(backend: Arc<dyn Backend>, rt: Handle) -> Arc<Self> {
        Arc::new(Self {
            backend,
            rt,
            st: Mutex::default(),
            wake: Notify::new(),
        })
    }

    pub fn status(&self) -> HelperStatus {
        let s = self.st.lock().unwrap();
        HelperStatus {
            exe_found: self.backend.exe_found(),
            running: s.running,
            last_run_at: s.last_run_at,
            last_run_ok: s.last_ok,
            last_run_reason: s.last_reason,
            inventory_last_modified: self.backend.inventory_mtime_ms(),
            installer_auto_install_helper: None,
        }
    }

    fn settle(&self, ok: bool, reason: Option<HelperRunReason>) {
        let mut s = self.st.lock().unwrap();
        s.running = false;
        s.last_ok = Some(ok);
        s.last_reason = if ok {
            None
        } else {
            Some(reason.unwrap_or(HelperRunReason::Error))
        };
        s.last_run_at = Some(now_ms_f64());
    }

    /// One run, gated only against a run already in flight. The work happens in its own task so
    /// stopping the poller never abandons it half way with `running` stuck on.
    pub async fn run_once(self: &Arc<Self>) -> bool {
        {
            let mut s = self.st.lock().unwrap();
            if s.running {
                return false;
            }
            s.running = true;
        }
        let me = self.clone();
        self.rt
            .spawn(async move {
                let (ok, reason) = me.backend.run_once().await;
                me.settle(ok, reason);
                ok
            })
            .await
            .unwrap_or_else(|_| {
                self.settle(false, None);
                false
            })
    }

    /// Preserves the inventory mtime cooldown across restarts. Idempotent while polling.
    pub fn start_polling(
        self: &Arc<Self>,
        interval: Duration,
        on_complete: Arc<dyn Fn(bool) + Send + Sync>,
    ) {
        let mut s = self.st.lock().unwrap();
        if s.poller.is_some() {
            return;
        }
        self.backend.refresh();
        let mtime = self.backend.inventory_mtime_ms();
        let age = mtime.map_or(Duration::MAX, |m| {
            Duration::from_secs_f64(((now_ms_f64() - m) / 1000.0).max(0.0))
        });
        let first_in = interval.saturating_sub(age);
        if !first_in.is_zero() {
            // The existing inventory.json counts as the last successful run, so the status
            // reflects reality instead of "data missing".
            s.last_run_at = mtime;
            s.last_ok = Some(true);
            tracing::info!(
                age_min = age.as_secs_f64() / 60.0,
                "inventory.json is recent - deferring the first helper run"
            );
        }
        let me = self.clone();
        s.poller = Some(
            self.rt
                .spawn(async move { me.poll(interval, first_in, on_complete).await }),
        );
    }

    async fn poll(
        self: Arc<Self>,
        interval: Duration,
        first_in: Duration,
        on_complete: Arc<dyn Fn(bool) + Send + Sync>,
    ) {
        let mut delay = first_in;
        let mut woken = false;
        loop {
            if !(delay.is_zero() && !woken) {
                woken = self.wait(delay).await;
            }
            if woken {
                // A login-triggered rescan skips the "game closed" gate: the game just logged in.
                self.st.lock().unwrap().waiting_for_game = false;
                on_complete(self.run_once().await);
            } else if self.should_wait_for_game() {
                if !std::mem::replace(&mut self.st.lock().unwrap().waiting_for_game, true) {
                    tracing::info!("Warframe is not running - helper runs wait for the game");
                }
                // Settles as the helper's own "game not found" would, so a later login still
                // gets its fast fetch; only the spawn is saved.
                self.settle(false, Some(HelperRunReason::GameNotRunning));
            } else {
                self.st.lock().unwrap().waiting_for_game = false;
                on_complete(self.run_once().await);
            }
            woken = false;
            let reason = self.st.lock().unwrap().last_reason;
            delay = Duration::from_millis(next_poll_delay_ms(reason, interval.as_millis() as u64));
        }
    }

    fn should_wait_for_game(&self) -> bool {
        !self.st.lock().unwrap().running && self.backend.game_running() == Some(false)
    }

    /// Sleeps `delay`; true when a login rescan that is still worth running cut it short.
    async fn wait(&self, delay: Duration) -> bool {
        let deadline = Instant::now() + delay;
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(deadline) => return false,
                () = self.wake.notified() => {
                    let s = self.st.lock().unwrap();
                    if should_retry_after_game_login(s.running, s.last_ok, s.last_reason) {
                        return true;
                    }
                }
            }
        }
    }

    /// EE.log saw a login: rescan soon if the last run could be fixed by it.
    pub fn run_after_game_login(self: &Arc<Self>) {
        {
            let mut s = self.st.lock().unwrap();
            if s.poller.is_none()
                || s.login_pending
                || !should_retry_after_game_login(s.running, s.last_ok, s.last_reason)
            {
                return;
            }
            s.login_pending = true;
        }
        let me = self.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(LOGIN_RETRY_DELAY).await;
            me.st.lock().unwrap().login_pending = false;
            me.wake.notify_one();
        });
    }

    pub fn stop_polling(&self) {
        let mut s = self.st.lock().unwrap();
        if let Some(p) = s.poller.take() {
            p.abort();
        }
        s.waiting_for_game = false;
    }
}
