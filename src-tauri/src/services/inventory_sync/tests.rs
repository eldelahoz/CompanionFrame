use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use super::*;

#[derive(Default)]
struct FakeRunner {
    start: AtomicUsize,
    stop: AtomicUsize,
    login: AtomicUsize,
}

impl SyncRunner for FakeRunner {
    fn start_polling(&self, _: Arc<dyn Fn(bool) + Send + Sync>) {
        self.start.fetch_add(1, SeqCst);
    }
    fn stop_polling(&self) {
        self.stop.fetch_add(1, SeqCst);
    }
    fn run_after_game_login(&self) {
        self.login.fetch_add(1, SeqCst);
    }
}

fn sync(memory: bool, auto: bool) -> (InventorySync, Arc<FakeRunner>) {
    let r = Arc::new(FakeRunner::default());
    let s = InventorySync::with_runner(r.clone(), Box::new(move || memory), Box::new(move || auto));
    (s, r)
}

fn counts(r: &FakeRunner) -> (usize, usize, usize) {
    (
        r.start.load(SeqCst),
        r.stop.load(SeqCst),
        r.login.load(SeqCst),
    )
}

#[test]
fn skip_reason_allows_only_memory_mode_with_sync_on() {
    assert_eq!(auto_sync_skip_reason(true, true), None);
    assert_eq!(
        auto_sync_skip_reason(true, false).as_deref(),
        Some("automatic inventory sync is off")
    );
    assert_eq!(
        auto_sync_skip_reason(false, true).as_deref(),
        Some("game-memory mode is off")
    );
}

#[test]
fn starts_polling_in_memory_mode() {
    let (s, r) = sync(true, true);
    s.apply("startup");
    assert_eq!(counts(&r), (1, 0, 0));
}

#[test]
fn passive_mode_never_polls_or_retries_login() {
    for auto in [true, false] {
        let (s, r) = sync(false, auto);
        s.apply("startup");
        s.on_game_login();
        assert_eq!(counts(&r), (0, 1, 0), "auto={auto}");
    }
}

#[test]
fn switching_modes_starts_and_stops_at_runtime() {
    let memory = Arc::new(Mutex::new(false));
    let r = Arc::new(FakeRunner::default());
    let s = InventorySync::with_runner(
        r.clone(),
        Box::new({
            let memory = memory.clone();
            move || *memory.lock().unwrap()
        }),
        Box::new(|| true),
    );
    s.apply("startup");
    assert_eq!(r.start.load(SeqCst), 0);
    *memory.lock().unwrap() = true;
    s.apply("settings");
    assert_eq!(r.start.load(SeqCst), 1);
    *memory.lock().unwrap() = false;
    s.apply("settings");
    assert_eq!(r.stop.load(SeqCst), 2);
}

#[test]
fn game_login_rescans_only_in_memory_mode_with_sync_on() {
    for (memory, auto, expected) in [(true, true, 1), (false, true, 0), (true, false, 0)] {
        let (s, r) = sync(memory, auto);
        s.on_game_login();
        assert_eq!(
            r.login.load(SeqCst),
            expected,
            "memory={memory} auto={auto}"
        );
    }
}
