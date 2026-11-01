//! Decides when background inventory acquisition runs: only in memory mode (`memory_gate`) with
//! automatic sync on. Port of WFHelper's `inventorySync.ts`. The manual "Run helper now" never
//! goes through here (it checks the gate itself).

use std::sync::{Arc, Mutex};

use super::api_helper::runner::{DEFAULT_POLL_INTERVAL, Runner};

pub trait SyncRunner: Send + Sync {
    fn start_polling(&self, on_complete: Arc<dyn Fn(bool) + Send + Sync>);
    fn stop_polling(&self);
    fn run_after_game_login(&self);
}

impl SyncRunner for Arc<Runner> {
    fn start_polling(&self, on_complete: Arc<dyn Fn(bool) + Send + Sync>) {
        Runner::start_polling(self, DEFAULT_POLL_INTERVAL, on_complete);
    }
    fn stop_polling(&self) {
        Runner::stop_polling(self);
    }
    fn run_after_game_login(&self) {
        Runner::run_after_game_login(self);
    }
}

/// Automatic acquisition needs memory mode (with consent) and the auto-sync setting.
pub fn auto_sync_skip_reason(memory_mode: bool, auto_sync_enabled: bool) -> Option<String> {
    if !memory_mode {
        return Some("game-memory mode is off".to_owned());
    }
    (!auto_sync_enabled).then(|| "automatic inventory sync is off".to_owned())
}

pub struct InventorySync {
    runner: Arc<dyn SyncRunner>,
    memory_mode: Box<dyn Fn() -> bool + Send + Sync>,
    auto_enabled: Box<dyn Fn() -> bool + Send + Sync>,
    logged_skip: Mutex<Option<String>>,
}

impl InventorySync {
    pub fn new(
        runner: Arc<Runner>,
        memory_mode: Box<dyn Fn() -> bool + Send + Sync>,
        auto_enabled: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self::with_runner(Arc::new(runner), memory_mode, auto_enabled)
    }

    pub fn with_runner(
        runner: Arc<dyn SyncRunner>,
        memory_mode: Box<dyn Fn() -> bool + Send + Sync>,
        auto_enabled: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        Self {
            runner,
            memory_mode,
            auto_enabled,
            logged_skip: Mutex::default(),
        }
    }

    fn skip_reason(&self) -> Option<String> {
        auto_sync_skip_reason((self.memory_mode)(), (self.auto_enabled)())
    }

    // Settings saves re-apply on every change; only log a reason once per change.
    fn log_skip(&self, context: &str, reason: String) {
        let mut last = self.logged_skip.lock().unwrap();
        if last.as_ref() != Some(&reason) {
            tracing::info!("automatic inventory acquisition skipped ({context}) - {reason}");
            *last = Some(reason);
        }
    }

    /// Starts or stops background acquisition to match the mode and the setting.
    pub fn apply(&self, context: &str) {
        if let Some(reason) = self.skip_reason() {
            self.log_skip(context, reason);
            self.runner.stop_polling();
            return;
        }
        *self.logged_skip.lock().unwrap() = None;
        self.runner.start_polling(Arc::new(|ok| {
            tracing::debug!(ok, "helper run finished");
        }));
    }

    /// EE.log saw a Warframe login; rescan unless acquisition is user-owned.
    pub fn on_game_login(&self) {
        match self.skip_reason() {
            Some(reason) => self.log_skip("game login", reason),
            None => self.runner.run_after_game_login(),
        }
    }
}

#[cfg(test)]
mod tests;
