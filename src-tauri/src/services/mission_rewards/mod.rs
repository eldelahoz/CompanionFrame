//! Mission rewards (port of WFHelper's `missionRewards`): after a mission ends, the inventory copy
//! the game keeps in memory is compared with the one from before it, and the gains are recorded.
//! The state machine is plain data driven by explicit clock values; `Deps` is everything outside.

pub mod diff;
pub mod history;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::{Db, db_err};
use crate::services::eelog::Source;
use crate::services::eelog_parsers::lines::is_mission_end_line;
use crate::services::game_memory::{
    self, GameInventoryRead, GameInventoryStatus, inventory_sync_id, sync_id_time,
};
use crate::services::trackers::{Line, LineTracker};
use diff::{Snapshot, diff_snapshots, snapshot_inventory};

// Owner's game, 2026-09-24: two full inventory copies were in memory 3 s and 10 s after the EOM
// line and gone at 20 s. A scan of the game's 1.6 GB heap took 3 to 5 s.
const READ_DELAY_MS: f64 = 3_000.0;
const READ_RETRY_DELAY_MS: u64 = 1_000;
const READ_ATTEMPTS: u32 = 2;
/// Joins an Abort to the EOM of the same mission; consecutive mission ends are 37 s apart at best.
const MISSION_END_JOIN_MS: f64 = 15_000.0;
/// A copy synced earlier than this before the end predates the mission (PC clock is ~1.5 s off DE's).
const SYNC_TOLERANCE_MS: f64 = 30_000.0;
/// The file delivers a line 13-26 s after DBWIN, so a regular load can bring a mission's rewards
/// before a file-only end line does.
const LATE_END_MS: f64 = 60_000.0;
/// DBWIN delivers a line at once and the file again 13-26 s later.
const ECHO_WINDOW_MS: f64 = 30_000.0;
const TRACKING_KEY: &str = "mission-tracking";

static HUB_NODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)hub").unwrap());
static HUB_ABORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"TopMenu\.lua: Abort: in hub").unwrap());
static SYNC_CONSUMABLES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"SyncAutoPopulatedConsumables for mission (MT_[A-Z_]+) with location (\S+)")
        .unwrap()
});
static STATE_STARTED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Game \[Info\]: OnStateStarted, mission type=(MT_[A-Z_]+)").unwrap()
});
static UPTIME_STAMP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(\d+\.\d+)\s+").unwrap());

// ---- wire types -------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardItem {
    pub unique_name: String,
    pub count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardSummary {
    pub id: String,
    pub ended_at: f64,
    pub read_at: f64,
    /// Above 1 when failed reads carried earlier missions into this one.
    pub mission_count: u32,
    pub mission_type: Option<String>,
    pub node: Option<String>,
    pub items: Vec<MissionRewardItem>,
    pub credits: f64,
    pub endo: f64,
    /// Sync time of the inventory the rewards were compared with.
    pub baseline_at: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum MissionRewardsPhase {
    Idle,
    Waiting,
    Reading,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum MissionRewardsBlocked {
    TrackingOff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum MissionRewardsFailure {
    GameNotRunning,
    AccessDenied,
    NoFreshCopy,
    InventoryUnreadable,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardsStatus {
    pub phase: MissionRewardsPhase,
    pub blocked: Option<MissionRewardsBlocked>,
    pub last_failure: Option<MissionRewardsFailure>,
    /// Mission ends the next successful read will cover.
    pub pending_missions: u32,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardsPayload {
    pub summaries: Vec<MissionRewardSummary>,
    pub status: MissionRewardsStatus,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardsQuery {
    pub offset: f64,
    pub limit: f64,
    /// Earliest `endedAt` included; the UI resolves "today" in its own time zone.
    pub since: Option<f64>,
    /// Local midnight for the page's today count, which ignores the other filters.
    pub today_since: Option<f64>,
    pub mission_type: Option<String>,
    /// A mission matches when it received any of these.
    pub unique_names: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardsTotals {
    /// Missions the matched summaries cover, carried reads included.
    pub missions: u32,
    pub credits: f64,
    pub endo: f64,
    pub items: Vec<MissionRewardItem>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MissionRewardsPage {
    pub summaries: Vec<MissionRewardSummary>,
    /// Summaries matching the query; the page is a slice of them.
    pub matched: u32,
    pub totals: MissionRewardsTotals,
    pub latest: Option<MissionRewardSummary>,
    pub recorded: u32,
    /// Missions since the query's `todaySince`, carried reads included; 0 without it.
    pub today: u32,
    pub mission_types: Vec<String>,
    /// Every item any recorded mission brought, for the UI's name search.
    pub item_types: Vec<String>,
    pub status: MissionRewardsStatus,
}

// ---- state machine ----------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub mission_type: Option<String>,
    pub node: Option<String>,
}

fn parse_info(line: &str) -> Option<Info> {
    if let Some(c) = SYNC_CONSUMABLES.captures(line) {
        return Some(Info {
            mission_type: Some(c[1].into()),
            node: Some(c[2].into()),
        });
    }
    STATE_STARTED.captures(line).map(|c| Info {
        mission_type: Some(c[1].into()),
        node: None,
    })
}

#[derive(Default)]
struct Pending {
    count: u32,
    ended_at: f64,
    info: Option<Info>,
}

#[derive(Clone)]
struct Baseline {
    snapshot: Snapshot,
    sync_time: Option<f64>,
    taken_at: f64,
}

#[derive(PartialEq)]
enum Freshness {
    Fresh,
    NotNewer,
    TooOld,
}

fn baseline_of(inventory: &Value, now: f64) -> Option<Baseline> {
    Some(Baseline {
        snapshot: snapshot_inventory(inventory)?,
        sync_time: inventory_sync_id(inventory).map(|id| sync_id_time(id) as f64),
        taken_at: now,
    })
}

pub struct Engine {
    sink: Box<dyn FnMut(MissionRewardSummary) + Send>,
    tracking: bool,
    baseline: Option<Baseline>,
    replaced: Vec<Baseline>,
    mission_info: Option<Info>,
    pending: Pending,
    phase: MissionRewardsPhase,
    wait_until: f64,
    token: u64,
    last_failure: Option<MissionRewardsFailure>,
    last_end_at: Option<f64>,
}

impl Engine {
    pub fn new(sink: Box<dyn FnMut(MissionRewardSummary) + Send>) -> Self {
        Self {
            sink,
            tracking: false,
            baseline: None,
            replaced: vec![],
            mission_info: None,
            pending: Pending::default(),
            phase: MissionRewardsPhase::Idle,
            wait_until: 0.0,
            token: 0,
            last_failure: None,
            last_end_at: None,
        }
    }

    fn reset_run_state(&mut self) {
        self.token += 1;
        self.baseline = None;
        self.replaced.clear();
        self.mission_info = None;
        self.pending = Pending::default();
        self.phase = MissionRewardsPhase::Idle;
        self.last_failure = None;
        self.last_end_at = None;
    }

    pub fn status(&self) -> MissionRewardsStatus {
        MissionRewardsStatus {
            phase: self.phase,
            blocked: (!self.tracking).then_some(MissionRewardsBlocked::TrackingOff),
            last_failure: self.last_failure,
            pending_missions: self.pending.count,
        }
    }

    /// Off drops any mission in flight; on starts from the loaded inventory.
    pub fn set_tracking(&mut self, enabled: bool, current: Option<&Value>, now: f64) -> bool {
        if enabled == self.tracking {
            return false;
        }
        self.tracking = enabled;
        self.reset_run_state();
        tracing::info!(
            "[MissionRewards] Mission tracking {}",
            if enabled { "on" } else { "off" }
        );
        if enabled {
            self.baseline = current.and_then(|inv| baseline_of(inv, now));
        }
        true
    }

    pub fn is_tracking(&self) -> bool {
        self.tracking
    }

    pub fn observe_line(&mut self, line: &str) {
        if !self.tracking {
            return;
        }
        let Some(info) = parse_info(line) else { return };
        // OnStateStarted repeats the type the SyncAutoPopulatedConsumables line before it named
        // with its node; a different type starts another mission, as Cetus to the Plains. A hub
        // node is never carried, or it would drop the next mission's end as a hub end.
        let keep_node = info.mission_type
            == self
                .mission_info
                .as_ref()
                .and_then(|i| i.mission_type.clone())
            && !self
                .mission_info
                .as_ref()
                .and_then(|i| i.node.as_deref())
                .is_some_and(|n| HUB_NODE.is_match(n));
        if info.node.is_some() || !keep_node {
            self.mission_info = Some(info);
        }
    }

    fn start_waiting(&mut self, end_at: f64) {
        self.token += 1;
        self.phase = MissionRewardsPhase::Waiting;
        self.wait_until = end_at + READ_DELAY_MS;
    }

    /// `ended_at` is the line's wall time: now for DBWIN, now minus the file lag otherwise.
    pub fn on_mission_end(
        &mut self,
        line: &str,
        ended_at: f64,
        current: Option<&Value>,
        now: f64,
    ) -> bool {
        if !self.tracking {
            return false;
        }
        if HUB_ABORT.is_match(line) {
            tracing::info!("[MissionRewards] Abort in a hub ignored");
            return false;
        }
        if self
            .last_end_at
            .is_some_and(|last| (ended_at - last).abs() < MISSION_END_JOIN_MS)
        {
            self.last_end_at = Some(ended_at);
            // The same mission's later line restarts the read, even after the first one failed.
            if self.pending.count > 0 {
                self.start_waiting(ended_at);
                return true;
            }
            return false;
        }
        let info = self.mission_info.take();
        if info
            .as_ref()
            .and_then(|i| i.node.as_deref())
            .is_some_and(|n| HUB_NODE.is_match(n))
        {
            tracing::info!("[MissionRewards] Mission end in a hub ignored");
            return false;
        }
        self.last_end_at = Some(ended_at);
        self.rewind_baseline(ended_at);
        if self.baseline.is_none() {
            self.baseline = current.and_then(|inv| baseline_of(inv, now));
        }
        self.pending = Pending {
            count: self.pending.count + 1,
            ended_at,
            info,
        };
        self.start_waiting(ended_at);
        tracing::info!(
            "[MissionRewards] Mission end ({}); reading game memory in {:.1} s",
            self.pending
                .info
                .as_ref()
                .and_then(|i| i.mission_type.as_deref())
                .unwrap_or("type unknown"),
            (ended_at + READ_DELAY_MS - now).max(0.0) / 1000.0
        );
        true
    }

    /// Starts the read once the wait is over; the returned token identifies it.
    pub fn tick(&mut self, now: f64) -> Option<u64> {
        if self.phase != MissionRewardsPhase::Waiting || now < self.wait_until {
            return None;
        }
        self.token += 1;
        self.phase = MissionRewardsPhase::Reading;
        Some(self.token)
    }

    fn adopt_baseline(&mut self, inventory: &Value, sync_time: f64, now: f64) {
        if self
            .baseline
            .as_ref()
            .and_then(|b| b.sync_time)
            .is_some_and(|b| sync_time < b)
        {
            return;
        }
        let Some(snapshot) = snapshot_inventory(inventory) else {
            return;
        };
        if let Some(old) = self.baseline.take() {
            self.replaced.push(old);
            // Keep each replaced baseline while a late end could still need it.
            let times: Vec<f64> = self
                .replaced
                .iter()
                .map(|b| b.taken_at)
                .chain([now])
                .collect();
            let mut i = 0;
            self.replaced.retain(|_| {
                i += 1;
                times[i] > now - LATE_END_MS
            });
        }
        self.baseline = Some(Baseline {
            snapshot,
            sync_time: Some(sync_time),
            taken_at: now,
        });
    }

    /// Steps back over regular loads that arrived after the end and were synced fresh for it.
    fn rewind_baseline(&mut self, ended_at: f64) {
        while let Some(b) = &self.baseline {
            let Some(sync) = b.sync_time else { break };
            if !(b.taken_at > ended_at && sync >= ended_at - SYNC_TOLERANCE_MS) {
                break;
            }
            let Some(earlier) = self.replaced.pop() else {
                break;
            };
            self.baseline = Some(earlier);
        }
        self.replaced.clear();
    }

    fn freshness(&self, sync_time: f64) -> Freshness {
        if self
            .baseline
            .as_ref()
            .and_then(|b| b.sync_time)
            .is_some_and(|b| sync_time <= b)
        {
            Freshness::NotNewer
        } else if sync_time >= self.pending.ended_at - SYNC_TOLERANCE_MS {
            Freshness::Fresh
        } else {
            Freshness::TooOld
        }
    }

    fn attribute(&mut self, inventory: &Value, sync_time: f64, source: &str, now: f64) -> bool {
        let Some(after) = snapshot_inventory(inventory) else {
            return false;
        };
        let before = self.baseline.replace(Baseline {
            snapshot: after.clone(),
            sync_time: Some(sync_time),
            taken_at: now,
        });
        let batch = std::mem::take(&mut self.pending);
        self.last_failure = None;
        self.phase = MissionRewardsPhase::Idle;
        let Some(before) = before else {
            tracing::info!("[MissionRewards] No earlier inventory to compare; baseline taken");
            return true;
        };
        let delta = diff_snapshots(&before.snapshot, &after);
        tracing::info!(
            "[MissionRewards] {} mission(s) from the {source}: {} item types, credits +{}, endo +{}",
            batch.count,
            delta.items.len(),
            delta.credits,
            delta.endo
        );
        (self.sink)(MissionRewardSummary {
            id: format!("{}-{}", batch.ended_at as i64, now as i64),
            ended_at: batch.ended_at,
            read_at: now,
            mission_count: batch.count,
            mission_type: batch.info.as_ref().and_then(|i| i.mission_type.clone()),
            node: batch.info.and_then(|i| i.node),
            items: delta.items,
            credits: delta.credits,
            endo: delta.endo,
            baseline_at: before.sync_time,
        });
        true
    }

    fn attribute_load(&mut self, inventory: &Value, now: f64) -> bool {
        // Without a sync time a file cannot show it was saved after the mission.
        let Some(sync_time) = inventory_sync_id(inventory).map(|id| sync_id_time(id) as f64) else {
            return false;
        };
        if self.freshness(sync_time) != Freshness::Fresh {
            tracing::info!("[MissionRewards] Loaded inventory predates the pending end");
            return false;
        }
        self.attribute(inventory, sync_time, "inventory load", now)
    }

    fn evaluate(
        &mut self,
        read: &GameInventoryRead,
        label: u32,
        now: f64,
    ) -> Option<MissionRewardsFailure> {
        use MissionRewardsFailure::*;
        tracing::info!(
            "[MissionRewards] Memory scan {label}: {:?}, {} ms, {} MB in {} regions, {} copies, parsed {}/{}",
            read.status,
            read.scan_ms,
            read.scanned_mb,
            read.regions,
            read.stats.copies,
            read.stats.extractions - read.stats.failed_extractions,
            read.stats.extractions
        );
        match read.status {
            GameInventoryStatus::ProcessNotFound => return Some(GameNotRunning),
            GameInventoryStatus::AccessDenied => return Some(AccessDenied),
            GameInventoryStatus::Unavailable => return Some(Error),
            _ => {}
        }
        let Some(copy) = read.newest.as_ref() else {
            return Some(NoFreshCopy);
        };
        let sync_time = copy.sync_time as f64;
        if self.freshness(sync_time) != Freshness::Fresh {
            tracing::info!("[MissionRewards] Newest copy is not fresh for the pending end");
            return Some(NoFreshCopy);
        }
        if !self.attribute(&copy.inventory, sync_time, "game memory", now) {
            return Some(InventoryUnreadable);
        }
        None
    }

    /// A regular inventory load: attributes missions memory could not, else refreshes the baseline.
    pub fn on_inventory_loaded(&mut self, inventory: &Value, now: f64) -> bool {
        if !self.tracking {
            return false;
        }
        if self.pending.count > 0 {
            return self.phase == MissionRewardsPhase::Idle && self.attribute_load(inventory, now);
        }
        match inventory_sync_id(inventory).map(|id| sync_id_time(id) as f64) {
            None => {
                if self.baseline.is_none() {
                    self.baseline = baseline_of(inventory, now);
                }
            }
            Some(sync_time) => self.adopt_baseline(inventory, sync_time, now),
        }
        false
    }
}

// ---- reading ----------------------------------------------------------------------------------

/// Everything outside the state machine, so tests can fake the game, the clock and the inventory.
pub trait Deps: Send + Sync {
    fn current_inventory(&self) -> Option<Arc<Value>>;
    fn read_game(&self) -> GameInventoryRead;
    fn now(&self) -> f64;
    fn sleep(&self, ms: u64);
    fn changed(&self);
    fn spawn(&self, job: Box<dyn FnOnce() + Send>);
    /// Memory mode with consent (`memory_gate`); the tracker does nothing otherwise.
    fn memory_allowed(&self) -> bool {
        crate::services::memory_gate::allowed()
    }
}

pub struct Shared {
    pub engine: Mutex<Engine>,
    /// One memory scan at a time.
    scan: Mutex<()>,
}

impl Shared {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine: Mutex::new(engine),
            scan: Mutex::new(()),
        }
    }
}

/// Scans, retrying once after an empty read; falls back to a regular load that arrived meanwhile.
pub fn read_loop(shared: &Shared, deps: &dyn Deps, mine: u64) {
    for attempt in 1.. {
        let read = {
            let _scan = shared.scan.lock().unwrap();
            deps.read_game()
        };
        let current = deps.current_inventory();
        let now = deps.now();
        let mut e = shared.engine.lock().unwrap();
        if e.token != mine {
            return;
        }
        let Some(failure) = e.evaluate(&read, attempt, now) else {
            drop(e);
            deps.changed();
            return;
        };
        let retryable = matches!(
            failure,
            MissionRewardsFailure::NoFreshCopy | MissionRewardsFailure::InventoryUnreadable
        );
        if !retryable || attempt >= READ_ATTEMPTS {
            // A regular load during the read was left to it; the loader never announces the same
            // content twice.
            if !current.is_some_and(|inv| e.attribute_load(&inv, now)) {
                e.last_failure = Some(failure);
                e.phase = MissionRewardsPhase::Idle;
                tracing::warn!(
                    "[MissionRewards] No fresh inventory in game memory ({failure:?}); {} mission(s) wait for the next read or inventory load",
                    e.pending.count
                );
            }
            drop(e);
            deps.changed();
            return;
        }
        drop(e);
        deps.sleep(READ_RETRY_DELAY_MS);
        if shared.engine.lock().unwrap().token != mine {
            return;
        }
    }
}

// ---- line tracker -----------------------------------------------------------------------------

struct Sighting {
    source: Source,
    stamp: Option<String>,
    at: f64,
    paired: bool,
}

pub struct MissionTracker {
    shared: Arc<Shared>,
    deps: Arc<dyn Deps>,
    sightings: HashMap<String, Vec<Sighting>>,
    seen_inventory: Option<Arc<Value>>,
}

impl MissionTracker {
    pub fn new(shared: Arc<Shared>, deps: Arc<dyn Deps>) -> Self {
        Self {
            shared,
            deps,
            sightings: HashMap::new(),
            seen_inventory: None,
        }
    }

    /// Whether the file copy of a DBWIN line (or a repeat) was already seen. DBWIN text may lack
    /// the uptime stamp, so a file copy pairs with the oldest unpaired DBWIN copy of the same text.
    fn is_echo(&mut self, line: &str, source: Source, now: f64) -> bool {
        self.sightings.retain(|_, seen| {
            seen.retain(|s| now - s.at < ECHO_WINDOW_MS);
            !seen.is_empty()
        });
        let stamp = UPTIME_STAMP.captures(line).map(|c| c[1].to_owned());
        let text = UPTIME_STAMP.replace(line, "").trim().to_owned();
        let seen = self.sightings.entry(text).or_default();
        let original = stamp
            .as_ref()
            .and_then(|st| seen.iter().position(|s| s.stamp.as_ref() == Some(st)))
            .or_else(|| {
                (source == Source::File)
                    .then(|| {
                        seen.iter()
                            .position(|s| s.source == Source::Dbwin && !s.paired)
                    })
                    .flatten()
            });
        if let Some(i) = original {
            seen[i].paired = true;
        }
        seen.push(Sighting {
            source,
            stamp,
            at: now,
            paired: original.is_some(),
        });
        original.is_some()
    }

    fn pump(&self, now: f64) {
        let token = self.shared.engine.lock().unwrap().tick(now);
        if let Some(token) = token {
            self.deps.changed();
            let (shared, deps) = (self.shared.clone(), self.deps.clone());
            self.deps
                .spawn(Box::new(move || read_loop(&shared, &*deps, token)));
        }
    }
}

impl LineTracker for MissionTracker {
    fn on_line(&mut self, line: &Line) {
        // Passive mode: no mission is armed, so nothing below ever reads game memory.
        if !self.deps.memory_allowed() {
            return;
        }
        let end = is_mission_end_line(line.text);
        if !end && parse_info(line.text).is_none() {
            return;
        }
        if self.is_echo(line.text, line.source, line.now_ms) {
            return;
        }
        if end {
            let current = self.deps.current_inventory();
            let started = self.shared.engine.lock().unwrap().on_mission_end(
                line.text,
                line.now_ms - line.staleness_ms,
                current.as_deref(),
                line.now_ms,
            );
            if started {
                self.deps.changed();
                // A file line can be late enough that the read is already due.
                self.pump(line.now_ms);
            }
        } else {
            self.shared.engine.lock().unwrap().observe_line(line.text);
        }
    }

    fn on_reset(&mut self) {
        self.sightings.clear();
        self.shared.engine.lock().unwrap().mission_info = None;
    }

    fn on_tick(&mut self, now_ms: f64) {
        if !self.deps.memory_allowed() || !self.shared.engine.lock().unwrap().is_tracking() {
            return;
        }
        // The inventory loader announces a load by swapping the shared value.
        let current = self.deps.current_inventory();
        if let Some(inv) = current.filter(|inv| {
            !self
                .seen_inventory
                .as_ref()
                .is_some_and(|seen| Arc::ptr_eq(seen, inv))
        }) {
            self.seen_inventory = Some(inv.clone());
            if self
                .shared
                .engine
                .lock()
                .unwrap()
                .on_inventory_loaded(&inv, now_ms)
            {
                self.deps.changed();
            }
        }
        self.pump(now_ms);
    }
}

// ---- app wiring -------------------------------------------------------------------------------

struct AppDeps {
    app: AppHandle,
    shared: Arc<Shared>,
}

impl Deps for AppDeps {
    fn current_inventory(&self) -> Option<Arc<Value>> {
        // The inventory is managed after the trackers, so it is looked up on use.
        self.app
            .try_state::<Arc<crate::services::inventory::Inventory>>()?
            .current()
    }

    fn read_game(&self) -> GameInventoryRead {
        game_memory::read_game_inventory()
    }

    fn now(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_millis() as f64)
    }

    fn sleep(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }

    fn changed(&self) {
        let status = self.shared.engine.lock().unwrap().status();
        let db = self.app.state::<Db>();
        let summaries =
            history::recent(&db.0.lock().unwrap(), history::RECENT_LIMIT).unwrap_or_default();
        let _ = self.app.emit(
            "mission-reward",
            MissionRewardsPayload { summaries, status },
        );
    }

    fn spawn(&self, job: Box<dyn FnOnce() + Send>) {
        tauri::async_runtime::spawn_blocking(job);
    }
}

/// Managed state for the commands.
pub struct MissionRewards {
    shared: Arc<Shared>,
    deps: Arc<dyn Deps>,
}

fn tracking_saved(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT value FROM kv_cache WHERE key = ?1",
        [TRACKING_KEY],
        |r| r.get::<_, String>(0),
    )
    .is_ok_and(|v| v == "1")
}

fn save_tracking(conn: &Connection, on: bool) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO kv_cache (key, value, updated_at) VALUES (?1, ?2, unixepoch())
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        rusqlite::params![TRACKING_KEY, if on { "1" } else { "0" }],
    )
    .map(drop)
}

/// The hub's tracker. Manages `MissionRewards`; tracking is off until the user turns it on (the
/// memory read is the part antivirus software may question).
pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    let sink_app = app.clone();
    let shared = Arc::new(Shared::new(Engine::new(Box::new(move |summary| {
        let db = sink_app.state::<Db>();
        if let Err(e) = history::append(&db.0.lock().unwrap(), &summary) {
            tracing::warn!("[MissionRewards] saving a mission failed: {e}");
        }
    }))));
    let deps: Arc<dyn Deps> = Arc::new(AppDeps {
        app: app.clone(),
        shared: shared.clone(),
    });
    if tracking_saved(&app.state::<Db>().0.lock().unwrap()) {
        shared
            .engine
            .lock()
            .unwrap()
            .set_tracking(true, None, deps.now());
    }
    app.manage(MissionRewards {
        shared: shared.clone(),
        deps: deps.clone(),
    });
    Box::new(MissionTracker::new(shared, deps))
}

#[tauri::command]
#[specta::specta]
pub fn mission_rewards_page(
    db: State<Db>,
    mr: State<MissionRewards>,
    query: MissionRewardsQuery,
) -> Result<MissionRewardsPage, String> {
    let q = history::normalize_query(query)?;
    let status = mr.shared.engine.lock().unwrap().status();
    history::query(&db.0.lock().unwrap(), &q, status).map_err(db_err)
}

#[tauri::command]
#[specta::specta]
pub fn mission_rewards_set_tracking(
    db: State<Db>,
    mr: State<MissionRewards>,
    enabled: bool,
) -> Result<MissionRewardsStatus, String> {
    if enabled {
        crate::services::memory_gate::require()?;
    }
    save_tracking(&db.0.lock().unwrap(), enabled).map_err(db_err)?;
    let current = mr.deps.current_inventory();
    let status = {
        let mut e = mr.shared.engine.lock().unwrap();
        e.set_tracking(enabled, current.as_deref(), mr.deps.now());
        e.status()
    };
    mr.deps.changed();
    Ok(status)
}
