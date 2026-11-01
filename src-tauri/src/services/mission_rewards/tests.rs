//! Ported from WFHelper's `missionRewards` and `missionRewardsIpc` tests. The game, the clock and
//! the loaded inventory are faked; `advance` steps the clock and runs what a hub tick would.
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde_json::{Value, json};

use super::*;
use crate::services::game_memory::{GameInventoryCopy, InventoryScanStats};

const PLASTIDS: &str = "/Lotus/Types/Items/MiscItems/Plastids";
const RELIC: &str = "/Lotus/Types/Game/Projections/T1VoidProjectionTestBronze";
const T0: f64 = 1_800_000_000_000.0;
const EOM: &str = "Sys [Info]: EOM missionLocationUnlocked=1";
const ABORT: &str = "Script [Info]: TopMenu.lua: Abort: mission failed";
const HUB_ABORT_LINE: &str = "Script [Info]: TopMenu.lua: Abort: in hub (1)";
const SYNC_NODE: &str =
    "Sys [Info]: SyncAutoPopulatedConsumables for mission MT_SURVIVAL with location SolNode25";
const SYNC_HUB: &str =
    "Sys [Info]: SyncAutoPopulatedConsumables for mission MT_PVP with location CetusHub4";

fn sync_id(at: f64) -> String {
    format!("{:08x}{}", (at / 1000.0).floor() as u64, "0".repeat(16))
}

fn inv(plastids: u32, synced_at: Option<f64>, credits: u32) -> Value {
    let mut v = json!({
        "RegularCredits": credits, "FusionPoints": 0,
        "MiscItems": [{"ItemType": PLASTIDS, "ItemCount": plastids}],
    });
    if let Some(at) = synced_at {
        v["LastInventorySync"] = json!({"$oid": sync_id(at)});
    }
    v
}

#[derive(Clone)]
enum Mem {
    Empty,
    NotRunning,
    Copy(Value),
}

type Hook = Box<dyn Fn(usize) + Send + Sync>;

struct Fake {
    clock: Mutex<f64>,
    current: Mutex<Option<Arc<Value>>>,
    memory: Mutex<Mem>,
    reads: AtomicUsize,
    hook: Mutex<Option<Hook>>,
    gate: AtomicBool,
}

impl Deps for Fake {
    fn current_inventory(&self) -> Option<Arc<Value>> {
        self.current.lock().unwrap().clone()
    }
    fn read_game(&self) -> GameInventoryRead {
        let n = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(h) = &*self.hook.lock().unwrap() {
            h(n);
        }
        let (status, newest) = match self.memory.lock().unwrap().clone() {
            Mem::Empty => (GameInventoryStatus::NotFound, None),
            Mem::NotRunning => (GameInventoryStatus::ProcessNotFound, None),
            Mem::Copy(v) => {
                let id = inventory_sync_id(&v).unwrap().to_owned();
                let copy = GameInventoryCopy {
                    sync_time: sync_id_time(&id),
                    sync_id: id,
                    inventory: v,
                };
                (GameInventoryStatus::Ok, Some(copy))
            }
        };
        GameInventoryRead {
            status,
            newest,
            stats: InventoryScanStats::default(),
            scan_ms: 1,
            scanned_mb: 1,
            skipped_mb: 0,
            regions: 1,
        }
    }
    fn now(&self) -> f64 {
        *self.clock.lock().unwrap()
    }
    fn sleep(&self, ms: u64) {
        *self.clock.lock().unwrap() += ms as f64;
    }
    fn changed(&self) {}
    fn memory_allowed(&self) -> bool {
        self.gate.load(Ordering::SeqCst)
    }
    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        job();
    }
}

struct H {
    shared: Arc<Shared>,
    d: Arc<Fake>,
    saved: Arc<Mutex<Vec<MissionRewardSummary>>>,
}

impl H {
    fn new(current: Option<Value>, tracking: bool) -> Self {
        let saved = Arc::new(Mutex::new(vec![]));
        let sink = saved.clone();
        let shared = Arc::new(Shared::new(Engine::new(Box::new(move |s| {
            sink.lock().unwrap().push(s);
        }))));
        let d = Arc::new(Fake {
            clock: Mutex::new(T0),
            current: Mutex::new(current.map(Arc::new)),
            memory: Mutex::new(Mem::Empty),
            reads: AtomicUsize::new(0),
            hook: Mutex::new(None),
            gate: AtomicBool::new(true),
        });
        let h = Self { shared, d, saved };
        h.set_tracking(tracking);
        h
    }
    fn default() -> Self {
        Self::new(Some(inv(10, Some(T0 - 600_000.0), 1_000)), true)
    }
    fn now(&self) -> f64 {
        self.d.now()
    }
    fn e(&self) -> std::sync::MutexGuard<'_, Engine> {
        self.shared.engine.lock().unwrap()
    }
    fn set_tracking(&self, on: bool) {
        let cur = self.d.current_inventory();
        self.e().set_tracking(on, cur.as_deref(), self.now());
    }
    fn set_memory(&self, m: Mem) {
        *self.d.memory.lock().unwrap() = m;
    }
    fn set_current(&self, v: Value) {
        *self.d.current.lock().unwrap() = Some(Arc::new(v));
    }
    fn reads(&self) -> usize {
        self.d.reads.load(Ordering::SeqCst)
    }
    fn pump(&self) {
        let token = self.e().tick(self.now());
        if let Some(t) = token {
            read_loop(&self.shared, &*self.d, t);
        }
    }
    fn advance(&self, ms: f64) {
        let mut left = ms;
        while left > 0.0 {
            let step = left.min(250.0);
            *self.d.clock.lock().unwrap() += step;
            left -= step;
            self.pump();
        }
    }
    /// A mission end line at the current time (DBWIN-like: no lag).
    fn end_line(&self, line: &str) {
        self.end_at(line, self.now());
    }
    fn end_at(&self, line: &str, at: f64) {
        let cur = self.d.current_inventory();
        self.e()
            .on_mission_end(line, at, cur.as_deref(), self.now());
        self.pump();
    }
    fn end_mission(&self) -> f64 {
        let at = self.now();
        self.end_line(EOM);
        self.advance(READ_DELAY_MS);
        at
    }
    fn observe(&self, line: &str) {
        self.e().observe_line(line);
    }
    fn load(&self, v: &Value) {
        let now = self.now();
        self.e().on_inventory_loaded(v, now);
    }
    /// Newest first, like the store.
    fn history(&self) -> Vec<MissionRewardSummary> {
        self.saved.lock().unwrap().iter().rev().cloned().collect()
    }
    fn status(&self) -> MissionRewardsStatus {
        self.e().status()
    }
    fn items(&self) -> Vec<(String, u32)> {
        self.history()[0]
            .items
            .iter()
            .map(|i| (i.unique_name.clone(), i.count))
            .collect()
    }
}

fn plastids(n: u32) -> Vec<(String, u32)> {
    vec![(PLASTIDS.into(), n)]
}

fn idle() -> MissionRewardsStatus {
    MissionRewardsStatus {
        phase: MissionRewardsPhase::Idle,
        blocked: None,
        last_failure: None,
        pending_missions: 0,
    }
}

fn pending_failure(f: MissionRewardsFailure, n: u32) -> MissionRewardsStatus {
    MissionRewardsStatus {
        last_failure: Some(f),
        pending_missions: n,
        ..idle()
    }
}

// ---- reads from game memory -------------------------------------------------------------------

#[test]
fn reads_3s_after_the_end_and_records_a_copy_synced_5s_before_it() {
    let h = H::default();
    h.observe(SYNC_NODE);
    let ended_at = h.now();
    let mut copy = inv(14, Some(ended_at - 5_000.0), 26_000);
    copy["MiscItems"] =
        json!([{"ItemType": PLASTIDS, "ItemCount": 14}, {"ItemType": RELIC, "ItemCount": 1}]);
    h.set_memory(Mem::Copy(copy));

    h.end_line(EOM);
    assert_eq!(h.status().phase, MissionRewardsPhase::Waiting);
    h.advance(READ_DELAY_MS - 1.0);
    assert_eq!(h.reads(), 0);
    h.advance(1.0);

    assert_eq!(h.reads(), 1);
    let s = &h.history()[0];
    assert_eq!(
        (
            s.ended_at,
            s.mission_count,
            s.mission_type.as_deref(),
            s.node.as_deref()
        ),
        (ended_at, 1, Some("MT_SURVIVAL"), Some("SolNode25"))
    );
    assert_eq!((s.credits, s.endo), (25_000.0, 0.0));
    assert_eq!(h.items(), [(RELIC.into(), 1), (PLASTIDS.into(), 4)]);
    assert_eq!(
        s.baseline_at,
        Some(((ended_at - 600_000.0) / 1000.0).floor() * 1000.0)
    );
    assert_eq!(h.status(), idle());
}

#[test]
fn retries_once_a_second_after_an_empty_read_before_the_copy_leaves_memory() {
    let h = H::default();
    let ended_at = h.now();
    let d = h.d.clone();
    *h.d.hook.lock().unwrap() = Some(Box::new(move |n| {
        if n == 2 {
            *d.memory.lock().unwrap() = Mem::Copy(inv(15, Some(ended_at - 4_000.0), 1_000));
        }
    }));
    h.end_line(EOM);
    h.advance(READ_DELAY_MS);
    assert_eq!(h.reads(), 2);
    assert_eq!(
        h.now() - ended_at,
        READ_DELAY_MS + READ_RETRY_DELAY_MS as f64
    );
    assert_eq!(h.items(), plastids(5));
    assert_eq!(h.status(), idle());
    h.advance(60_000.0);
    assert_eq!(h.reads(), 2);
}

#[test]
fn does_not_wait_for_the_games_inventory_sync_or_reload_lines() {
    let h = H::default();
    h.set_memory(Mem::Copy(inv(13, Some(h.now() - 5_000.0), 1_000)));
    h.end_line(EOM);
    h.observe("Sys [Error]: Bad data from inventory.php:");
    h.observe("Script [Info]: Hub.lua: Inventory sync done");
    h.advance(READ_DELAY_MS - 1.0);
    assert_eq!(h.reads(), 0);
    h.advance(1.0);
    assert_eq!((h.reads(), h.items()), (1, plastids(3)));
    h.advance(60_000.0);
    assert_eq!(h.reads(), 1);
}

#[test]
fn reads_at_once_when_the_file_delivers_the_end_line_late() {
    let h = H::default();
    h.observe(SYNC_NODE);
    let ended_at = h.now();
    h.advance(5_000.0);
    h.set_memory(Mem::Copy(inv(12, Some(ended_at - 5_000.0), 1_000)));
    h.end_at(EOM, ended_at);
    assert_eq!(h.reads(), 1);
    assert_eq!(
        (h.history()[0].ended_at, h.items()),
        (ended_at, plastids(2))
    );
}

#[test]
fn gives_the_carried_mission_to_the_next_regular_inventory_load() {
    let h = H::default();
    let ended_at = h.end_mission();
    h.advance(60_000.0);
    assert_eq!(
        h.status(),
        pending_failure(MissionRewardsFailure::NoFreshCopy, 1)
    );

    h.load(&inv(10, Some(ended_at - 600_000.0), 1_000));
    assert!(h.history().is_empty());
    h.load(&inv(17, Some(ended_at + 400_000.0), 1_000));
    assert_eq!((h.history()[0].mission_count, h.items()), (1, plastids(7)));
    assert_eq!(h.status(), idle());
    assert_eq!(h.reads(), 2);
}

#[test]
fn does_not_retry_while_the_game_is_not_running() {
    let h = H::default();
    h.set_memory(Mem::NotRunning);
    h.end_mission();
    h.advance(60_000.0);
    assert_eq!(h.reads(), 1);
    assert_eq!(
        h.status(),
        pending_failure(MissionRewardsFailure::GameNotRunning, 1)
    );
}

#[test]
fn leaves_a_regular_load_during_the_wait_to_the_memory_read() {
    let h = H::default();
    let ended_at = h.now();
    h.end_line(EOM);
    h.load(&inv(13, Some(ended_at + 2_000.0), 1_000));
    assert!(h.history().is_empty());
    h.set_memory(Mem::Copy(inv(13, Some(ended_at - 4_000.0), 1_000)));
    h.advance(READ_DELAY_MS);
    assert_eq!((h.history().len(), h.items()), (1, plastids(3)));
}

/// Runs `during` inside the first memory read, as a load arriving while the scan is open.
fn during_first_read(h: &H, during: impl Fn(&H) + Send + Sync + 'static) {
    let (shared, d) = (h.shared.clone(), h.d.clone());
    *h.d.hook.lock().unwrap() = Some(Box::new(move |n| {
        if n == 1 {
            during(&H {
                shared: shared.clone(),
                d: d.clone(),
                saved: Arc::default(),
            });
        }
    }));
}

#[test]
fn falls_back_to_a_fresh_load_that_arrived_while_the_reads_failed() {
    let h = H::default();
    let ended_at = h.now();
    during_first_read(&h, move |h| {
        let loaded = inv(14, Some(ended_at + 2_000.0), 1_000);
        h.set_current(loaded.clone());
        h.load(&loaded);
    });
    h.end_mission();
    assert_eq!(h.reads(), 2);
    assert_eq!(
        (h.history().len(), h.history()[0].mission_count, h.items()),
        (1, 1, plastids(4))
    );
    assert_eq!(h.status(), idle());
}

#[test]
fn records_once_when_the_read_succeeds_after_a_fresh_load_arrived_during_it() {
    let h = H::default();
    let ended_at = h.now();
    h.set_memory(Mem::Copy(inv(13, Some(ended_at - 4_000.0), 1_000)));
    during_first_read(&h, move |h| {
        let loaded = inv(14, Some(ended_at + 2_000.0), 1_000);
        h.set_current(loaded.clone());
        h.load(&loaded);
    });
    h.end_mission();
    h.advance(60_000.0);
    assert_eq!(h.reads(), 1);
    assert_eq!((h.history().len(), h.items()), (1, plastids(3)));
}

#[test]
fn publishes_nothing_from_a_read_that_finishes_after_tracking_was_turned_off() {
    let h = H::default();
    let ended_at = h.now();
    during_first_read(&h, move |h| {
        let loaded = inv(14, Some(ended_at + 2_000.0), 1_000);
        h.set_current(loaded.clone());
        h.load(&loaded);
        h.set_tracking(false);
    });
    h.end_mission();
    h.advance(60_000.0);
    assert_eq!(h.reads(), 1);
    assert!(h.history().is_empty());
    assert_eq!(h.status().pending_missions, 0);
    assert_eq!(h.status().phase, MissionRewardsPhase::Idle);
}

// ---- freshness --------------------------------------------------------------------------------

#[test]
fn never_attributes_a_copy_synced_long_before_the_end() {
    let h = H::new(Some(inv(10, None, 1_000)), true);
    h.set_memory(Mem::Copy(inv(10, Some(h.now() - 300_000.0), 1_000)));
    h.end_line(EOM);
    h.advance(60_000.0);
    assert_eq!(h.reads(), 2);
    assert!(h.history().is_empty());
    assert_eq!(
        h.status(),
        pending_failure(MissionRewardsFailure::NoFreshCopy, 1)
    );
}

#[test]
fn rejects_a_copy_that_is_not_newer_than_the_baseline() {
    let h = H::default();
    let loaded = inv(12, Some(h.now() - 10_000.0), 1_000);
    h.load(&loaded);
    h.set_memory(Mem::Copy(loaded));
    h.end_line(EOM);
    h.advance(60_000.0);
    assert_eq!(h.reads(), 2);
    assert!(h.history().is_empty());
    assert_eq!(
        h.status(),
        pending_failure(MissionRewardsFailure::NoFreshCopy, 1)
    );
}

#[test]
fn keeps_out_a_copy_from_a_reload_before_the_mission_and_takes_the_next_one() {
    let h = H::default();
    h.advance(60_000.0);
    // A relay trade reloads the inventory a minute before the mission ends.
    h.set_memory(Mem::Copy(inv(12, Some(h.now()), 1_000)));
    h.advance(60_000.0);
    let ended_at = h.now();
    let d = h.d.clone();
    *h.d.hook.lock().unwrap() = Some(Box::new(move |n| {
        if n == 2 {
            *d.memory.lock().unwrap() = Mem::Copy(inv(15, Some(ended_at - 5_000.0), 1_000));
        }
    }));
    h.end_line(EOM);
    h.advance(READ_DELAY_MS);
    assert_eq!(h.reads(), 2);
    assert_eq!(h.items(), plastids(5));
}

// ---- mission ends -----------------------------------------------------------------------------

#[test]
fn joins_an_abort_and_the_eom_of_one_mission_into_a_single_read_after_the_eom() {
    let h = H::default();
    h.set_memory(Mem::Copy(inv(11, Some(h.now() + 1_000.0), 1_000)));
    h.end_line(ABORT);
    h.advance(2_000.0);
    h.end_line(EOM);
    assert_eq!(h.status().pending_missions, 1);
    h.advance(READ_DELAY_MS - 1.0);
    assert_eq!(h.reads(), 0);
    h.advance(1.0);
    assert_eq!(
        (h.reads(), h.history().len(), h.history()[0].mission_count),
        (1, 1, 1)
    );
}

#[test]
fn reads_again_for_the_eom_that_follows_its_abort_after_the_first_reads_failed() {
    let h = H::default();
    h.end_line(ABORT);
    h.advance(READ_DELAY_MS + READ_RETRY_DELAY_MS as f64);
    assert_eq!(h.reads(), 2);
    assert_eq!(h.status().pending_missions, 1);
    assert_eq!(h.status().phase, MissionRewardsPhase::Idle);

    h.set_memory(Mem::Copy(inv(12, Some(h.now()), 1_000)));
    h.end_line(EOM);
    assert_eq!(h.status().phase, MissionRewardsPhase::Waiting);
    h.advance(READ_DELAY_MS);
    assert_eq!(h.reads(), 3);
    assert_eq!((h.history()[0].mission_count, h.items()), (1, plastids(2)));
}

#[test]
fn carries_a_mission_whose_read_failed_into_the_next_one() {
    let h = H::default();
    h.end_mission();
    h.advance(60_000.0);
    let second = h.now();
    h.set_memory(Mem::Copy(inv(16, Some(second - 5_000.0), 1_000)));
    h.end_line(EOM);
    assert_eq!(h.status().pending_missions, 2);
    h.advance(READ_DELAY_MS);
    assert_eq!(h.reads(), 3);
    assert_eq!(
        (h.history()[0].mission_count, h.history()[0].ended_at),
        (2, second)
    );
}

#[test]
fn ignores_an_abort_in_a_hub_and_a_mission_end_logged_in_a_town() {
    let h = H::default();
    h.end_line(HUB_ABORT_LINE);
    h.observe(SYNC_HUB);
    h.end_line(EOM);
    h.advance(60_000.0);
    assert_eq!(h.reads(), 0);
    assert_eq!(h.status(), idle());
}

#[test]
fn drops_a_hubs_own_end_but_does_not_carry_its_node_into_the_next_mission() {
    let h = H::default();
    h.observe(SYNC_HUB);
    h.end_line(EOM);
    h.advance(60_000.0);
    assert_eq!(h.reads(), 0);

    // Cetus to the Plains logs the free-roam type before the free-roam node.
    h.observe(SYNC_HUB);
    h.observe("Game [Info]: OnStateStarted, mission type=MT_LANDSCAPE");
    h.set_memory(Mem::Copy(inv(12, Some(h.now()), 1_000)));
    h.end_mission();
    let s = &h.history()[0];
    assert_eq!(
        (s.mission_type.as_deref(), s.node.as_deref()),
        (Some("MT_LANDSCAPE"), None)
    );
}

#[test]
fn does_not_turn_a_mission_that_logs_its_hubs_type_into_a_hub_end() {
    let h = H::default();
    h.observe(SYNC_HUB);
    h.observe("Game [Info]: OnStateStarted, mission type=MT_PVP");
    h.set_memory(Mem::Copy(inv(12, Some(h.now()), 1_000)));
    h.end_mission();
    let s = &h.history()[0];
    assert_eq!(
        (s.mission_type.as_deref(), s.node.as_deref()),
        (Some("MT_PVP"), None)
    );
}

#[test]
fn keeps_the_node_past_the_type_only_line_that_repeats_the_same_type() {
    let h = H::default();
    h.set_memory(Mem::Copy(inv(12, Some(h.now()), 1_000)));
    h.observe(SYNC_NODE);
    h.observe("Game [Info]: OnStateStarted, mission type=MT_SURVIVAL");
    h.end_mission();
    assert_eq!(h.history()[0].node.as_deref(), Some("SolNode25"));
}

#[test]
fn does_not_carry_one_missions_info_into_the_next() {
    let h = H::default();
    h.observe(SYNC_NODE);
    h.set_memory(Mem::Copy(inv(10, Some(h.now()), 1_000)));
    h.end_mission();
    h.advance(60_000.0);
    h.set_memory(Mem::Copy(inv(10, Some(h.now()), 1_000)));
    h.end_mission();
    let history = h.history();
    assert_eq!(history[1].mission_type.as_deref(), Some("MT_SURVIVAL"));
    assert_eq!(history[0].mission_type, None);
}

// ---- tracking switch --------------------------------------------------------------------------

#[test]
fn does_nothing_at_all_while_tracking_is_off() {
    let h = H::new(Some(inv(10, Some(T0 - 600_000.0), 1_000)), false);
    h.set_memory(Mem::Copy(inv(15, Some(h.now()), 1_000)));
    h.observe(SYNC_NODE);
    h.end_line(EOM);
    h.load(&inv(20, Some(h.now() + 5_000.0), 1_000));
    h.advance(120_000.0);
    assert_eq!(h.reads(), 0);
    assert!(h.history().is_empty());
    assert_eq!(
        h.status(),
        MissionRewardsStatus {
            blocked: Some(MissionRewardsBlocked::TrackingOff),
            ..idle()
        }
    );
}

#[test]
fn starts_from_the_inventory_loaded_when_turned_on_without_a_memory_scan() {
    let h = H::new(None, false);
    h.set_current(inv(30, Some(h.now() - 60_000.0), 1_000));
    h.set_tracking(true);
    h.advance(60_000.0);
    assert_eq!(h.reads(), 0);
    h.set_memory(Mem::Copy(inv(33, Some(h.now()), 1_000)));
    h.end_mission();
    assert_eq!(h.items(), plastids(3));
}

#[test]
fn drops_a_mission_in_flight_when_turned_off() {
    let h = H::default();
    h.end_line(EOM);
    h.set_tracking(false);
    h.advance(60_000.0);
    assert_eq!(h.reads(), 0);
    assert_eq!(h.status().pending_missions, 0);
}

// ---- baselines --------------------------------------------------------------------------------

#[test]
fn takes_the_first_read_as_the_baseline_when_no_inventory_was_loaded() {
    let h = H::new(None, true);
    h.set_memory(Mem::Copy(inv(10, Some(h.now()), 1_000)));
    h.end_mission();
    assert!(h.history().is_empty());
    h.advance(60_000.0);
    h.set_memory(Mem::Copy(inv(13, Some(h.now()), 1_000)));
    h.end_mission();
    assert_eq!(h.items(), plastids(3));
}

#[test]
fn moves_the_baseline_to_a_regular_load_between_missions() {
    let h = H::default();
    h.load(&inv(110, Some(h.now() - 60_000.0), 1_000));
    h.load(&inv(50, Some(h.now() - 120_000.0), 1_000));
    h.set_memory(Mem::Copy(inv(115, Some(h.now()), 1_000)));
    h.end_mission();
    assert_eq!(h.items(), plastids(5));
}

#[test]
fn keeps_a_late_file_ends_rewards_when_a_regular_load_brought_them_first() {
    let h = H::default();
    h.observe(SYNC_NODE);
    let ended_at = h.now();
    let after_first = inv(14, Some(ended_at - 2_000.0), 1_000);
    h.set_memory(Mem::Copy(after_first.clone()));
    h.advance(10_000.0);
    h.set_current(after_first.clone());
    h.load(&after_first);
    h.advance(10_000.0);
    h.end_at(EOM, ended_at);
    h.advance(60_000.0);

    h.set_memory(Mem::Copy(inv(17, Some(h.now()), 1_000)));
    h.end_mission();
    let history = h.history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].ended_at, ended_at);
    assert_eq!(
        history[1].items,
        [MissionRewardItem {
            unique_name: PLASTIDS.into(),
            count: 4
        }]
    );
    assert_eq!(
        history[0].items,
        [MissionRewardItem {
            unique_name: PLASTIDS.into(),
            count: 3
        }]
    );
}

#[test]
fn keeps_a_load_synced_well_before_a_late_end_as_that_ends_baseline() {
    let h = H::default();
    h.observe(SYNC_NODE);
    let ended_at = h.now();
    h.advance(10_000.0);
    h.load(&inv(12, Some(ended_at - 60_000.0), 1_000));
    h.advance(10_000.0);
    h.set_memory(Mem::Copy(inv(15, Some(ended_at - 2_000.0), 1_000)));
    h.end_at(EOM, ended_at);
    h.advance(READ_DELAY_MS);
    assert_eq!(h.items(), plastids(3));
}

#[test]
fn records_an_empty_summary_when_the_mission_brought_nothing() {
    let h = H::default();
    h.set_memory(Mem::Copy(inv(10, Some(h.now()), 1_000)));
    h.end_mission();
    let s = &h.history()[0];
    assert_eq!((s.items.len(), s.credits, s.endo), (0, 0.0, 0.0));
}

#[test]
fn never_scans_memory_without_a_pending_mission() {
    let h = H::new(None, true);
    h.set_memory(Mem::Copy(inv(10, Some(h.now()), 1_000)));
    h.observe("Sys [Error]: Bad data from inventory.php:");
    h.observe("Script [Info]: Hub.lua: Inventory sync done");
    h.advance(120_000.0);
    assert_eq!(h.reads(), 0);
}

#[test]
fn never_counts_a_load_without_a_sync_time_as_a_missions_rewards() {
    let h = H::default();
    h.end_mission();
    h.advance(60_000.0);
    h.load(&inv(99, None, 1_000));
    assert!(h.history().is_empty());
    assert_eq!(h.status().pending_missions, 1);
    assert_eq!(h.reads(), 2);
}

// ---- the tracker over hub lines (DBWIN vs file echoes) ----------------------------------------

const ECHO_EOM: &str = "1281.547 Sys [Info]: EOM missionLocationUnlocked=1";
const ECHO_SYNC: &str = "1269.269 Sys [Info]: SyncAutoPopulatedConsumables for mission MT_SURVIVAL with location SolNode25";

struct T {
    h: H,
    tracker: MissionTracker,
}

impl T {
    fn new() -> Self {
        let h = H::default();
        h.set_memory(Mem::Copy(inv(13, Some(h.now() + 5_000.0), 1_000)));
        let tracker = MissionTracker::new(h.shared.clone(), h.d.clone());
        Self { h, tracker }
    }
    fn line(&mut self, text: &str, source: Source, staleness_ms: f64) {
        let now_ms = self.h.now();
        self.tracker.on_line(&Line {
            text,
            source,
            now_ms,
            staleness_ms,
            realtime_active: false,
        });
    }
    fn advance(&mut self, ms: f64) {
        let mut left = ms;
        while left > 0.0 {
            let step = left.min(250.0);
            *self.h.d.clock.lock().unwrap() += step;
            left -= step;
            let now = self.h.now();
            self.tracker.on_tick(now);
        }
    }
    fn missions(&self) -> Vec<u32> {
        self.h.history().iter().map(|s| s.mission_count).collect()
    }
}

#[test]
fn reads_3s_after_a_dbwin_end_and_ignores_the_files_echo_of_it() {
    let mut t = T::new();
    t.line(ECHO_SYNC, Source::Dbwin, 0.0);
    t.line(ECHO_EOM, Source::Dbwin, 0.0);
    t.advance(2_000.0);
    t.line(ECHO_SYNC, Source::File, 0.0);
    t.line(ECHO_EOM, Source::File, 0.0);
    t.advance(1_000.0);
    assert_eq!(t.h.reads(), 1);

    t.advance(20_000.0);
    t.line(ECHO_EOM, Source::File, 0.0);
    t.advance(60_000.0);
    assert_eq!(t.h.reads(), 1);
    assert_eq!(t.missions(), [1]);
    assert_eq!(t.h.history()[0].node.as_deref(), Some("SolNode25"));
    assert_eq!(t.h.status(), idle());
}

#[test]
fn ignores_the_file_echo_when_the_dbwin_text_lacks_the_uptime_stamp() {
    let mut t = T::new();
    t.line(
        "Sys [Info]: EOM missionLocationUnlocked=1",
        Source::Dbwin,
        0.0,
    );
    t.advance(2_000.0);
    t.line(ECHO_EOM, Source::File, 0.0);
    t.advance(60_000.0);
    assert_eq!((t.h.reads(), t.missions()), (1, vec![1]));
}

#[test]
fn reads_once_for_each_of_two_missions_37s_apart_whose_file_copies_trail_by_26s() {
    for stamped in [true, false] {
        let mut t = T::new();
        let dbwin = |s: &str| {
            if stamped {
                s.to_owned()
            } else {
                s.split_once(' ').unwrap().1.to_owned()
            }
        };
        let second = "1318.547 Sys [Info]: EOM missionLocationUnlocked=1";
        t.line(&dbwin(ECHO_EOM), Source::Dbwin, 0.0);
        t.advance(26_000.0);
        t.line(ECHO_EOM, Source::File, 0.0);
        t.advance(11_000.0);
        t.h.set_memory(Mem::Copy(inv(16, Some(t.h.now() + 5_000.0), 1_000)));
        t.line(&dbwin(second), Source::Dbwin, 0.0);
        t.advance(26_000.0);
        t.line(second, Source::File, 0.0);
        t.line(second, Source::File, 0.0);
        t.advance(60_000.0);
        assert_eq!(t.h.reads(), 2, "stamped: {stamped}");
        assert_eq!(t.missions(), [1, 1]);
        assert_eq!(t.h.status(), idle());
    }
}

#[test]
fn passive_mode_never_reads_memory_after_a_mission_end() {
    let mut t = T::new();
    t.h.d.gate.store(false, Ordering::SeqCst);
    t.line(ECHO_EOM, Source::Dbwin, 0.0);
    t.advance(60_000.0);
    assert_eq!(t.h.reads(), 0);
    assert!(t.missions().is_empty());
}

#[test]
fn still_reads_for_a_dbwin_end_that_follows_a_file_only_one() {
    let mut t = T::new();
    t.line(
        "1281.000 Script [Info]: Background load started",
        Source::File,
        0.0,
    );
    t.advance(26_000.0);
    // The hub dates the file line by its lag: 26 s behind the DBWIN stream.
    t.line(ECHO_EOM, Source::File, 26_000.0);
    t.advance(11_000.0);
    t.h.set_memory(Mem::Copy(inv(16, Some(t.h.now() + 5_000.0), 1_000)));
    t.line(
        "1318.547 Sys [Info]: EOM missionLocationUnlocked=1",
        Source::Dbwin,
        0.0,
    );
    t.advance(3_000.0);
    assert_eq!(t.h.reads(), 2);
}

#[test]
fn counts_a_later_file_only_end_after_a_dbwin_end_as_a_second_mission() {
    let mut t = T::new();
    t.line(ECHO_EOM, Source::Dbwin, 0.0);
    t.advance(60_000.0);
    t.h.set_memory(Mem::Copy(inv(16, Some(t.h.now() + 5_000.0), 1_000)));
    t.line(
        "1341.547 Sys [Info]: EOM missionLocationUnlocked=1",
        Source::File,
        0.0,
    );
    t.advance(3_000.0);
    assert_eq!(t.h.reads(), 2);
    assert_eq!(t.missions().len(), 2);
    assert_eq!(t.h.items(), plastids(3));
}

#[test]
fn dates_a_late_file_only_end_by_the_files_lag() {
    let mut t = T::new();
    let start = t.h.now();
    t.advance(20_000.0);
    t.line(
        "1270.269 Sys [Info]: EOM missionLocationUnlocked=1",
        Source::File,
        19_000.0,
    );
    assert_eq!(t.h.reads(), 1);
    assert_eq!(t.h.history()[0].ended_at, start + 1_000.0);
}

#[test]
fn hands_regular_inventory_loads_to_the_tracker_on_its_tick() {
    let mut t = T::new();
    t.h.set_memory(Mem::Empty);
    t.line(ECHO_EOM, Source::Dbwin, 0.0);
    t.advance(60_000.0);
    assert!(t.h.history().is_empty());
    let loaded = inv(16, Some(t.h.now()), 1_000);
    t.h.set_current(loaded);
    t.advance(500.0);
    assert_eq!(t.missions(), [1]);
}

#[test]
fn a_hub_reset_forgets_the_mission_in_progress_and_the_echo_memory() {
    let mut t = T::new();
    t.line(ECHO_SYNC, Source::Dbwin, 0.0);
    t.tracker.on_reset();
    t.line(ECHO_EOM, Source::Dbwin, 0.0);
    t.advance(READ_DELAY_MS);
    assert_eq!(t.h.history()[0].node, None);
}
