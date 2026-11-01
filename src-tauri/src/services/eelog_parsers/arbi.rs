//! Arbitration run parser: detects an arbi run in EE.log and derives the
//! drone / rotation / saturation stats from the lines fed between its markers.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use super::line_ts;

pub const ARBI_EARLY_WAVE_CAP: usize = 15;
pub const ARBI_SPAWN_DATA_VERSION: u32 = 2;
/// Enemy count a room counts as saturated at.
pub const ARBI_SATURATION_THRESHOLD: u32 = 15;

macro_rules! rx {
    ($name:ident, $p:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($p).unwrap());
    };
}

rx!(
    MISSION_NAME,
    r"Script \[Info\]: ThemedSquadOverlay\.lua: Mission name: (.*)"
);
rx!(SPAM_SKIP, r"Game \[Warning\]:|DamagePct");
rx!(AGENT_CREATED, r"OnAgentCreated");
rx!(
    AGENT_FULL,
    r"OnAgentCreated.*?/Npc/(.+?)(\d+)\s+.*?MonitoredTicking\s+(\d+)"
);
rx!(AGENT_NPC_NAME, r"/Npc/([A-Za-z0-9_]+)");
rx!(
    AGENT_EXCLUDE,
    r"(?i)(Replicant|RJCrew|petavatar|VoidClone|Turret|Dropship|CatbrowPetAgent|AllyAgent)"
);
rx!(DRONE, r"OnAgentCreated.*?CorpusEliteShieldDroneAgent");
rx!(
    DEFENSE_REWARD,
    r"Sys \[Info\]: Created /Lotus/Interface/DefenseReward\.swf"
);
rx!(
    SURVIVAL_REWARD,
    r"Sys \[Info\]: Created /Lotus/Interface/SurvivalReward\.swf"
);
rx!(MONITORED_TICKING, r"AI \[Info\]: .*?MonitoredTicking (\d+)");
rx!(WAVE_START_UNPAUSE, r"WaveDefend\.lua: Starting wave (\d+)");
rx!(DEFENSE_WAVE, r"WaveDefend\.lua: Defense wave: (\d+)");
rx!(SLEEP_BETWEEN, r"WaveDefend\.lua: _SleepBetweenWaves");
rx!(SLEEP_BETWEEN_3, r"WaveDefend\.lua: _SleepBetweenWaves\(3\)");
// Host-only defense line; the vector after "spawn point:" is the point, not the NPC.
rx!(
    SPAWN_POINT,
    r"WaveDefend\.lua: Spawned a .+? spawn point: (\S+) @ Vector\((-?[\d.]+), (-?[\d.]+), (-?[\d.]+)\)"
);
rx!(
    WAVE_COUNTDOWN,
    r"/Lotus/Interface/ProjectionsCountdown\.swf"
);
rx!(TERRITORY, r"Script \[Info\]: TerritoryMission\.lua");
rx!(
    TERRITORY_START,
    r"(?i)TerritoryMission\.lua: .*(control|captured)"
);
rx!(
    PENDING_SECTOR_PLAIN,
    r"(?:ThemedSquadOverlay\.lua: Pending mission:|MapRedux\.lua: Confirm sector) (\S+)"
);
rx!(
    PENDING_SECTOR_JSON,
    r#"Set squad mission.*?"name":"([^"]+)""#
);
rx!(ELITE_SECTOR, r"^(SolNode\d+)_EliteAlert$");
rx!(
    CLIENT_MISSION_JOIN,
    r#"Client (?:joining mission in-progress|loaded)[^{]*\{"name":"([^"]+)"\}"#
);
rx!(
    CACHED_MISSION_NAME,
    r"ThemedSquadOverlay\.lua: Cached mission name=(.+) \((SolNode\d+)\)"
);
rx!(
    SYNC_CONSUMABLES,
    r"SyncAutoPopulatedConsumables for mission (MT_[A-Z_]+) with location (\S+)"
);
rx!(
    STATE_STARTED,
    r"Game \[Info\]: OnStateStarted, mission type=(MT_[A-Z_]+)"
);
rx!(
    LOADOUT_LOADED,
    r"Game \[Info\]: (.+?) loadout loader finished"
);
rx!(ABORT_CONFIRMED, r"TopMenu\.lua: Abort:");
rx!(EOM_COMMIT, r"Sys \[Info\]: EOM missionLocationUnlocked=");
rx!(
    DISRUPTION_ROUND_START,
    r"SentientArtifactMission\.lua: Disruption: State change: ARTIFACT_ROUND$"
);
rx!(
    DISRUPTION_ROUND_DONE,
    r"SentientArtifactMission\.lua: Disruption: State change: ARTIFACT_ROUND_DONE"
);
rx!(NAME_SUFFIX, r"\s+-\s+Arbitration$");

const SPAWN_POINT_HINT: &str = "spawn point:";
/// Distinct points kept per run; a defense tileset never approaches this.
const MAX_SPAWN_POINTS: usize = 300;
/// Decorative agents that never tick are excluded, except these.
const FORCED_VALID_AGENTS: [&str; 1] = ["CorpusEliteShieldDroneAgent"];
const REWARD_DEBOUNCE_SEC: f64 = 30.0;
/// Mirror Defense nodes run 2 waves per rotation instead of 3.
const MIRROR_DEFENSE_NODES: [&str; 2] = ["munio", "tyana"];
const DISRUPTION_CONDUITS_PER_ROUND: u32 = 4;
const SATURATION_BUCKET_WIDTH: u32 = 3;
const SATURATION_MAX_COUNT: u32 = 30;
/// Tick-sample gaps longer than this are load screens/host stalls, not gameplay.
const SATURATION_MAX_SEGMENT_SEC: f64 = 29.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ArbiMissionType {
    Defense,
    Interception,
    Disruption,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunEndReason {
    MissionEnd,
    Aborted,
    NewMission,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArbiParserEvent {
    RunStart {
        mission_name: String,
        node: String,
        mission_type: ArbiMissionType,
        game_time_sec: f64,
    },
    RunEnd {
        reason: RunEndReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ArbiInterval {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiSaturationBucket {
    pub min_count: u32,
    pub label: String,
    pub seconds: f64,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiWaveEntry {
    pub index: u32,
    pub duration_sec: f64,
    pub saturation_pct: f64,
}

/// One WaveDefend spawn point, aggregated over the run. Engine coordinates: y is up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ArbiSpawnPoint {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub count: u32,
    /// Spawns in waves 1..=ARBI_EARLY_WAVE_CAP, index 0 = wave 1; absent when the point never fired that early.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub early: Option<Vec<u32>>,
}

/// Spawns this point produced from `min_wave` on. Past the tracked window the
/// result is the upper bound, so it never rises with `min_wave`.
#[cfg(test)]
pub fn count_from_wave(point: &ArbiSpawnPoint, min_wave: usize) -> u32 {
    let Some(early) = &point.early else {
        return point.count;
    };
    if min_wave <= 1 {
        return point.count;
    }
    let last_cut = (min_wave - 1).min(ARBI_EARLY_WAVE_CAP);
    let cut: u32 = early.iter().take(last_cut).sum();
    point.count.saturating_sub(cut)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiRunStats {
    pub kills_per_drone: f64,
    pub avg_drone_interval_sec: Option<f64>,
    pub expected_vitus_mean: f64,
    pub expected_vitus_std: f64,
    pub vitus_per_min: f64,
    pub waves_per_rotation: u32,
    pub drone_timestamps: Vec<f64>,
    pub reward_timestamps: Vec<f64>,
    pub precise_start_sec: Option<f64>,
    pub last_activity_sec: f64,
    pub saturation_buckets: Vec<ArbiSaturationBucket>,
    pub waves: Option<Vec<ArbiWaveEntry>>,
    pub pause_intervals: Vec<ArbiInterval>,
    pub idle_intervals: Vec<ArbiInterval>,
    pub rotation_saturation_pct: Vec<f64>,
    pub spawn_points: Vec<ArbiSpawnPoint>,
    pub spawn_data_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArbiParsedRun {
    pub mission_name: String,
    pub node: String,
    pub mission_type: ArbiMissionType,
    pub mission_type_raw: Option<String>,
    pub sol_node: Option<String>,
    pub run_start_sec: f64,
    /// Timestamp of the end marker that closed the run; None when it ended implicitly.
    pub run_end_sec: Option<f64>,
    pub last_activity_sec: f64,
    pub duration_sec: f64,
    pub rotations: u32,
    pub drones: u32,
    pub total_enemies: u32,
    /// False for client-side runs, whose logs carry no AI data to analyze.
    pub host_telemetry: bool,
    /// Squad member names in load order, the local player included.
    pub players: Vec<String>,
    pub stats: Option<ArbiRunStats>,
}

/// The run in progress, for the live overlay.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiLive {
    pub node: String,
    pub mission_type: ArbiMissionType,
    pub mission_type_raw: Option<String>,
    pub elapsed_sec: f64,
    pub rotations: u32,
    pub drones: u32,
    pub enemies: u32,
    /// Drone kills per minute of combat.
    pub dpm: f64,
    /// Share of combat time spent at `ARBI_SATURATION_THRESHOLD`+ enemies.
    pub saturation_pct: f64,
    /// Latest enemy count of the host's ticking monitor.
    pub current_enemies: Option<u32>,
}

/// Normal-approximation Vitus model ported from svesk.github.io/arbi: (mean, std).
pub fn compute_vitus_model(rotations: u32, waves_per_rotation: u32, drones: u32) -> (f64, f64) {
    const DROP: f64 = 0.15;
    const RETRIEVER: f64 = 0.18;
    const ROTATION: f64 = 0.1;
    let (rot, waves, drones) = (rotations as f64, waves_per_rotation as f64, drones as f64);
    let mean_val = 4.0 * RETRIEVER + 2.0 * (1.0 - RETRIEVER);
    let expect_sq = 16.0 * RETRIEVER + 4.0 * (1.0 - RETRIEVER);
    let var_val = expect_sq - mean_val * mean_val;
    let rot_mean = rot + rot * ROTATION * waves;
    let rot_var = rot * ROTATION * (1.0 - ROTATION) * waves.powi(2);
    let mean_drops = drones * DROP;
    let var_drops = drones * DROP * (1.0 - DROP);
    let mean = rot_mean + mean_drops * mean_val;
    let variance = rot_var + mean_drops * var_val + mean_val * mean_val * var_drops;
    (mean, variance.max(0.0).sqrt())
}

/// Drop empty windows, sort, merge overlapping and touching ones, clamped to `bounds`.
pub fn merge_intervals(list: &[ArbiInterval], bounds: Option<ArbiInterval>) -> Vec<ArbiInterval> {
    let (lo, hi) = bounds.map_or((f64::NEG_INFINITY, f64::INFINITY), |b| (b.start, b.end));
    let mut clamped: Vec<ArbiInterval> = list
        .iter()
        .map(|iv| ArbiInterval {
            start: iv.start.max(lo),
            end: iv.end.min(hi),
        })
        .filter(|iv| iv.end > iv.start)
        .collect();
    clamped.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut out: Vec<ArbiInterval> = Vec::new();
    for iv in clamped {
        match out.last_mut() {
            Some(last) if iv.start <= last.end => last.end = last.end.max(iv.end),
            _ => out.push(iv),
        }
    }
    out
}

struct SpawnEvent {
    name: Option<String>,
    tick: Option<u32>,
}

struct TickSample {
    t: f64,
    val: u32,
}

struct SpawnPointTally {
    x: f64,
    y: f64,
    z: f64,
    count: u32,
    early: [u32; ARBI_EARLY_WAVE_CAP],
}

struct RunState {
    mission_name: String,
    node: String,
    mission_type: ArbiMissionType,
    /// Engine mission type (MT_*); once seen it outranks all name heuristics.
    mission_type_raw: Option<String>,
    sol_node: Option<String>,
    waves_per_rotation: u32,
    run_start_sec: f64,
    mission_start_sec: Option<f64>,
    run_end_sec: Option<f64>,
    last_activity_sec: f64,
    event_count: u32,
    /// Saw an AI spawn line. Only the host logs those, so false means client-side.
    host_telemetry: bool,
    players: Vec<String>,
    rotations: u32,
    last_reward_sec: f64,
    reward_timestamps: Vec<f64>,
    drone_timestamps: Vec<f64>,
    spawn_events: Vec<SpawnEvent>,
    tick_samples: Vec<TickSample>,
    pause_intervals: Vec<ArbiInterval>,
    current_pause_start: Option<f64>,
    spawn_points: HashMap<String, SpawnPointTally>,
    /// Wave the spawn lines belong to; spawns logged before wave 1 count as wave 1.
    current_wave: u32,
    wave_starts: BTreeMap<u32, f64>,
    wave_ends: Vec<f64>,
    wave_countdowns: Vec<f64>,
    /// Disruption only: ARTIFACT_ROUND / ARTIFACT_ROUND_DONE timestamps.
    round_starts: Vec<f64>,
    round_ends: Vec<f64>,
    precise_start_sec: Option<f64>,
}

fn classify_mission(mission_name: &str) -> (String, ArbiMissionType, u32) {
    // Both name shapes exist: legacy "Arbitration: Casta (Ceres)" and "Oestrus (Eris) - Arbitration".
    let stripped = mission_name.replacen("Arbitration:", "", 1);
    let node = NAME_SUFFIX.replace(&stripped, "").trim().to_string();
    let lower = node.to_lowercase();
    let mirror = MIRROR_DEFENSE_NODES.iter().any(|m| lower.contains(m));
    let kind = if lower.contains("defense") || mirror {
        ArbiMissionType::Defense
    } else if lower.contains("interception") {
        ArbiMissionType::Interception
    } else {
        ArbiMissionType::Other
    };
    (node, kind, if mirror { 2 } else { 3 })
}

fn round1(v: f64) -> f64 {
    // floor(x + .5) mirrors JS Math.round on negatives.
    (v * 10.0 + 0.5).floor() / 10.0
}

impl RunState {
    fn apply_mission_type_raw(&mut self, mt: &str) {
        if self.mission_type_raw.is_some() {
            return;
        }
        self.mission_type_raw = Some(mt.to_string());
        self.mission_type = match mt {
            "MT_DEFENSE" => ArbiMissionType::Defense,
            "MT_TERRITORY" => ArbiMissionType::Interception,
            "MT_ARTIFACT" => ArbiMissionType::Disruption,
            _ => ArbiMissionType::Other,
        };
        // A disruption round is four conduits; node names never say "Disruption".
        if self.mission_type == ArbiMissionType::Disruption {
            self.waves_per_rotation = DISRUPTION_CONDUITS_PER_ROUND;
        }
    }

    fn has_full_stats(&self) -> bool {
        self.mission_type != ArbiMissionType::Other
    }

    fn record_spawn_point(&mut self, m: &Captures) {
        let wave = self.current_wave as usize;
        let bump = |t: &mut SpawnPointTally| {
            t.count += 1;
            if (1..=ARBI_EARLY_WAVE_CAP).contains(&wave) {
                t.early[wave - 1] += 1;
            }
        };
        if let Some(t) = self.spawn_points.get_mut(&m[1]) {
            bump(t);
            return;
        }
        if self.spawn_points.len() >= MAX_SPAWN_POINTS {
            return;
        }
        let num = |i: usize| round1(m[i].parse::<f64>().unwrap_or(0.0));
        let mut fresh = SpawnPointTally {
            x: num(2),
            y: num(3),
            z: num(4),
            count: 0,
            early: [0; ARBI_EARLY_WAVE_CAP],
        };
        bump(&mut fresh);
        self.spawn_points.insert(m[1].to_string(), fresh);
    }

    fn build_spawn_points(&self) -> Vec<ArbiSpawnPoint> {
        let mut out: Vec<ArbiSpawnPoint> = self
            .spawn_points
            .iter()
            .map(|(id, p)| ArbiSpawnPoint {
                id: id.clone(),
                x: p.x,
                y: p.y,
                z: p.z,
                count: p.count,
                early: p.early.iter().any(|&n| n > 0).then(|| p.early.to_vec()),
            })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.id.cmp(&b.id)));
        out
    }

    /// Tick segments inside the window that count as gameplay.
    fn each_saturation_segment(&self, start: f64, end: f64, mut visit: impl FnMut(u32, f64)) {
        for pair in self.tick_samples.windows(2) {
            let (cur, next) = (&pair[0], &pair[1]);
            let seg_start = cur.t.max(start);
            let seg_end = next.t.min(end);
            let dur = seg_end - seg_start;
            if dur <= 0.0 || dur > SATURATION_MAX_SEGMENT_SEC {
                continue;
            }
            let paused = self.pause_intervals.iter().any(|p| {
                (seg_start < p.start && seg_end > p.end)
                    || (seg_start >= p.start && seg_start < p.end)
            });
            if !paused {
                visit(cur.val, dur);
            }
        }
    }

    fn saturation_pct_in(&self, start: f64, end: f64) -> f64 {
        let (mut total, mut above) = (0.0, 0.0);
        self.each_saturation_segment(start, end, |enemies, dur| {
            total += dur;
            if enemies >= ARBI_SATURATION_THRESHOLD {
                above += dur;
            }
        });
        if total > 0.0 {
            above / total * 100.0
        } else {
            0.0
        }
    }

    /// One share per reward, each window running from the previous reward.
    fn build_rotation_saturation(&self, start: f64) -> Vec<f64> {
        let mut window_start = start;
        self.reward_timestamps
            .iter()
            .map(|&end| {
                let pct = self.saturation_pct_in(window_start, end);
                window_start = end;
                pct
            })
            .collect()
    }

    fn build_saturation(&self, start: f64, end: f64) -> Vec<ArbiSaturationBucket> {
        let n = SATURATION_MAX_COUNT.div_ceil(SATURATION_BUCKET_WIDTH) as usize;
        let mut seconds = vec![0.0; n];
        let mut total = 0.0;
        self.each_saturation_segment(start, end, |enemies, dur| {
            let idx = ((enemies / SATURATION_BUCKET_WIDTH) as usize).min(n - 1);
            seconds[idx] += dur;
            total += dur;
        });
        seconds
            .iter()
            .enumerate()
            .map(|(i, &sec)| {
                let lo = i as u32 * SATURATION_BUCKET_WIDTH;
                let label = if i == n - 1 {
                    format!("{lo}+")
                } else {
                    format!("{lo}-{}", lo + SATURATION_BUCKET_WIDTH - 1)
                };
                ArbiSaturationBucket {
                    min_count: lo,
                    label,
                    seconds: sec,
                    pct: if total > 0.0 {
                        sec / total * 100.0
                    } else {
                        0.0
                    },
                }
            })
            .collect()
    }

    /// Tick-stream stalls: the same gap width the saturation pass treats as non-gameplay.
    fn build_idle_intervals(&self, start: f64, end: f64) -> Vec<ArbiInterval> {
        let raw: Vec<ArbiInterval> = self
            .tick_samples
            .windows(2)
            .filter(|p| p[1].t - p[0].t > SATURATION_MAX_SEGMENT_SEC)
            .map(|p| ArbiInterval {
                start: p[0].t,
                end: p[1].t,
            })
            .collect();
        merge_intervals(&raw, Some(ArbiInterval { start, end }))
    }

    fn build_waves(&self) -> Vec<ArbiWaveEntry> {
        let mut out = Vec::new();
        let (mut end_idx, mut countdown_idx) = (0, 0);
        for (&wave, &start) in &self.wave_starts {
            while end_idx < self.wave_ends.len() && self.wave_ends[end_idx] <= start {
                end_idx += 1;
            }
            let dur;
            if wave % 3 == 0 {
                // Every 3rd wave has no sleep line; the reward countdown fires ~5s after clear.
                while countdown_idx < self.wave_countdowns.len()
                    && self.wave_countdowns[countdown_idx] <= start
                {
                    countdown_idx += 1;
                }
                if countdown_idx >= self.wave_countdowns.len() {
                    continue;
                }
                dur = self.wave_countdowns[countdown_idx] - 5.0 - start;
                countdown_idx += 1;
            } else {
                if end_idx >= self.wave_ends.len() {
                    continue;
                }
                dur = self.wave_ends[end_idx] - start;
                end_idx += 1;
            }
            out.push(ArbiWaveEntry {
                index: wave,
                duration_sec: dur,
                saturation_pct: self.saturation_pct_in(start, start + dur),
            });
        }
        out
    }

    /// Disruption rounds: each ARTIFACT_ROUND paired with the next ROUND_DONE.
    fn build_rounds(&self) -> Vec<ArbiWaveEntry> {
        let mut out = Vec::new();
        let mut end_idx = 0;
        for (i, &start) in self.round_starts.iter().enumerate() {
            while end_idx < self.round_ends.len() && self.round_ends[end_idx] <= start {
                end_idx += 1;
            }
            let Some(&end) = self.round_ends.get(end_idx) else {
                break;
            };
            out.push(ArbiWaveEntry {
                index: i as u32 + 1,
                duration_sec: end - start,
                saturation_pct: self.saturation_pct_in(start, end),
            });
            end_idx += 1;
        }
        out
    }

    /// An agent name is confirmed ticking if any consecutive named pair shows its
    /// tick counter advancing; names only ever seen non-advancing are decorative.
    fn count_valid_enemies(&self) -> u32 {
        let named: Vec<&SpawnEvent> = self
            .spawn_events
            .iter()
            .filter(|s| s.name.is_some())
            .collect();
        let (mut confirmed, mut suspected) = (HashSet::new(), HashSet::new());
        for pair in named.windows(2) {
            let (prev, curr) = (pair[0], pair[1]);
            if let (Some(pt), Some(ct), Some(name)) = (prev.tick, curr.tick, &prev.name)
                && !name.is_empty()
            {
                if ct > pt {
                    confirmed.insert(name.as_str());
                } else {
                    suspected.insert(name.as_str());
                }
            }
        }
        let non_ticking: HashSet<&str> = suspected
            .into_iter()
            .filter(|n| !confirmed.contains(n) && !FORCED_VALID_AGENTS.contains(n))
            .collect();
        self.spawn_events
            .iter()
            .filter(|s| {
                s.name
                    .as_deref()
                    .is_none_or(|n| n.is_empty() || !non_ticking.contains(n))
            })
            .count() as u32
    }
}

#[derive(Default)]
pub struct ArbiParser {
    run: Option<RunState>,
    /// Internal sector of the most recent mission select (e.g. "SolNode167_EliteAlert").
    pending_sector: Option<String>,
    /// Squad-overlay (name, solNode), the only name source when joining in progress.
    cached_mission: Option<(String, String)>,
}

impl ArbiParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_run_active(&self) -> bool {
        self.run.is_some()
    }

    pub fn last_activity_sec(&self) -> f64 {
        self.run.as_ref().map_or(0.0, |r| r.last_activity_sec)
    }

    /// Monotonic combat activity count; orbiter lines cannot keep a run alive.
    pub fn activity_count(&self) -> u32 {
        self.run.as_ref().map_or(0, |r| r.event_count)
    }

    /// The running totals; None when idle. Costs a pass over the spawns, so poll it, don't call per line.
    pub fn snapshot(&self) -> Option<ArbiLive> {
        let r = self.run.as_ref()?;
        let drones = r.drone_timestamps.len() as u32;
        let last_tick = r.tick_samples.last();
        let start = r
            .precise_start_sec
            .or(r.drone_timestamps.first().copied())
            .or(r.mission_start_sec)
            .unwrap_or(r.run_start_sec);
        let last = r.last_activity_sec.max(last_tick.map_or(0.0, |t| t.t));
        let elapsed = (last - start).max(0.0);
        Some(ArbiLive {
            node: r.node.clone(),
            mission_type: r.mission_type,
            mission_type_raw: r.mission_type_raw.clone(),
            elapsed_sec: elapsed,
            rotations: r.rotations,
            drones,
            enemies: r.count_valid_enemies() + drones,
            dpm: if elapsed > 0.0 {
                drones as f64 / (elapsed / 60.0)
            } else {
                0.0
            },
            saturation_pct: r.saturation_pct_in(start, last),
            current_enemies: last_tick.map(|t| t.val),
        })
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn start_run(&mut self, mission_name: &str, game_time_sec: f64) -> ArbiParserEvent {
        let (node, mission_type, waves_per_rotation) = classify_mission(mission_name);
        // Consume the sector so a stale _EliteAlert can't mark a later mission as arbi.
        let sol_node = self
            .pending_sector
            .take()
            .and_then(|s| ELITE_SECTOR.captures(&s).map(|c| c[1].to_string()));
        self.run = Some(RunState {
            mission_name: mission_name.to_string(),
            node: node.clone(),
            mission_type,
            mission_type_raw: None,
            sol_node,
            waves_per_rotation,
            run_start_sec: game_time_sec,
            mission_start_sec: None,
            run_end_sec: None,
            last_activity_sec: game_time_sec,
            event_count: 0,
            host_telemetry: false,
            players: Vec::new(),
            rotations: 0,
            last_reward_sec: 0.0,
            reward_timestamps: Vec::new(),
            drone_timestamps: Vec::new(),
            spawn_events: Vec::new(),
            tick_samples: Vec::new(),
            pause_intervals: Vec::new(),
            current_pause_start: None,
            spawn_points: HashMap::new(),
            current_wave: 1,
            wave_starts: BTreeMap::new(),
            wave_ends: Vec::new(),
            wave_countdowns: Vec::new(),
            round_starts: Vec::new(),
            round_ends: Vec::new(),
            precise_start_sec: None,
        });
        ArbiParserEvent::RunStart {
            mission_name: mission_name.to_string(),
            node,
            mission_type,
            game_time_sec,
        }
    }

    pub fn feed_line(&mut self, line: &str) -> Option<ArbiParserEvent> {
        if line.is_empty() || SPAM_SKIP.is_match(line) {
            return None;
        }
        let ts = line_ts(line);

        if let Some(c) = PENDING_SECTOR_PLAIN
            .captures(line)
            .or_else(|| PENDING_SECTOR_JSON.captures(line))
        {
            self.pending_sector = Some(c[1].to_string());
        }
        if let Some(c) = CACHED_MISSION_NAME.captures(line) {
            self.cached_mission = Some((c[1].trim().to_string(), c[2].to_string()));
        }

        // Joining a squad mid-mission never logs a "Mission name:" line; the client
        // load is the only start signal. Its sector still carries _EliteAlert.
        if self.run.is_none()
            && let Some(c) = CLIENT_MISSION_JOIN.captures(line)
            && let Some(joined) = ELITE_SECTOR.captures(&c[1])
        {
            self.pending_sector = Some(c[1].to_string());
            let name = match &self.cached_mission {
                Some((name, sol)) if *sol == joined[1] => name.clone(),
                _ => joined[1].to_string(),
            };
            return Some(self.start_run(&name, ts));
        }

        if let Some(c) = MISSION_NAME.captures(line) {
            let name = c[1].trim().to_string();
            let is_arbi = name.contains("Arbitration")
                || self
                    .pending_sector
                    .as_deref()
                    .is_some_and(|s| ELITE_SECTOR.is_match(s));
            let Some(run) = self.run.as_mut() else {
                return is_arbi.then(|| self.start_run(&name, ts));
            };
            // Host-migration replay guard: the log can repeat the arbi mission-name
            // line with an older timestamp after a migration - not a new run.
            if is_arbi && ts > 0.0 && run.last_activity_sec > 0.0 && ts < run.last_activity_sec {
                return None;
            }
            if ts > 0.0 {
                run.run_end_sec = Some(ts);
            }
            return Some(ArbiParserEvent::RunEnd {
                reason: RunEndReason::NewMission,
            });
        }

        let run = self.run.as_mut()?;

        for (pat, reason) in [
            (&ABORT_CONFIRMED, RunEndReason::Aborted),
            (&EOM_COMMIT, RunEndReason::MissionEnd),
        ] {
            if pat.is_match(line) {
                if ts > 0.0 {
                    run.run_end_sec = Some(ts);
                }
                return Some(ArbiParserEvent::RunEnd { reason });
            }
        }

        if let Some(c) = LOADOUT_LOADED.captures(line) {
            let player = c[1].trim();
            if !player.is_empty() && !run.players.iter().any(|p| p == player) {
                run.players.push(player.to_string());
            }
        }

        if let Some(c) = SYNC_CONSUMABLES.captures(line) {
            run.apply_mission_type_raw(&c[1]);
            if run.sol_node.is_none() {
                run.sol_node = Some(c[2].to_string());
            }
        }
        if let Some(c) = STATE_STARTED.captures(line) {
            run.apply_mission_type_raw(&c[1]);
            if run.mission_start_sec.is_none() && ts > 0.0 {
                run.mission_start_sec = Some(ts);
            }
        }

        // Pause bookkeeping (used to exclude between-wave downtime from saturation).
        if SLEEP_BETWEEN.is_match(line) || DEFENSE_REWARD.is_match(line) {
            run.event_count += 1;
            if run.current_pause_start.is_none() && ts > 0.0 {
                run.current_pause_start = Some(ts);
            }
        }
        let mut is_unpause = false;
        if WAVE_START_UNPAUSE.is_match(line) {
            run.event_count += 1;
            if run.mission_type == ArbiMissionType::Defense {
                is_unpause = true;
            }
        }
        if TERRITORY.is_match(line) {
            run.event_count += 1;
            if run.mission_type_raw.is_none() && run.mission_type == ArbiMissionType::Other {
                run.mission_type = ArbiMissionType::Interception;
                run.waves_per_rotation = 3;
            }
            is_unpause = true;
        }
        if is_unpause
            && ts > 0.0
            && let Some(start) = run.current_pause_start.take()
        {
            run.pause_intervals.push(ArbiInterval { start, end: ts });
        }

        // Thousands of these per run, so keep the regex behind a substring test.
        if line.contains(SPAWN_POINT_HINT)
            && let Some(c) = SPAWN_POINT.captures(line)
        {
            run.record_spawn_point(&c);
        }

        if let Some(c) = DEFENSE_WAVE.captures(line) {
            // Wave lines outrank the mission-name heuristic (but not the engine MT_).
            run.event_count += 1;
            if run.mission_type_raw.is_none() {
                run.mission_type = ArbiMissionType::Defense;
            }
            let wave: u32 = c[1].parse().unwrap_or(0);
            if wave > 0 {
                run.current_wave = wave;
            }
            if ts > 0.0 {
                run.wave_starts.insert(wave, ts);
                run.last_activity_sec = run.last_activity_sec.max(ts);
                if run.precise_start_sec.is_none() && wave == 1 {
                    run.precise_start_sec = Some(ts);
                }
            }
        } else if run.precise_start_sec.is_none() && ts > 0.0 && TERRITORY_START.is_match(line) {
            run.precise_start_sec = Some(ts);
        }

        if run.mission_type == ArbiMissionType::Disruption && ts > 0.0 {
            if DISRUPTION_ROUND_DONE.is_match(line) {
                run.event_count += 1;
                run.rotations += 1;
                run.round_ends.push(ts);
                run.reward_timestamps.push(ts);
                run.last_activity_sec = run.last_activity_sec.max(ts);
                // The interval before the next round is downtime, like a reward screen.
                run.current_pause_start.get_or_insert(ts);
            } else if DISRUPTION_ROUND_START.is_match(line) {
                run.event_count += 1;
                run.round_starts.push(ts);
                run.last_activity_sec = run.last_activity_sec.max(ts);
                run.precise_start_sec.get_or_insert(ts);
                if let Some(start) = run.current_pause_start.take() {
                    run.pause_intervals.push(ArbiInterval { start, end: ts });
                }
            }
        }

        // The survival reward UI also gets created in other modes (seen 25s before an
        // interception's DefenseReward) - only trust it in actual survivals.
        let is_survival_reward = run.mission_type_raw.as_deref() == Some("MT_SURVIVAL")
            && SURVIVAL_REWARD.is_match(line);
        if is_survival_reward {
            run.event_count += 1;
        }
        if (is_survival_reward || DEFENSE_REWARD.is_match(line))
            && ts - run.last_reward_sec > REWARD_DEBOUNCE_SEC
        {
            run.rotations += 1;
            run.last_reward_sec = ts;
            run.reward_timestamps.push(ts);
            run.last_activity_sec = run.last_activity_sec.max(ts);
            run.current_pause_start.get_or_insert(ts);
        }

        if run.has_full_stats()
            && ts > 0.0
            && let Some(c) = MONITORED_TICKING.captures(line)
        {
            run.event_count += 1;
            run.tick_samples.push(TickSample {
                t: ts,
                val: c[1].parse().unwrap_or(0),
            });
        }

        if DRONE.is_match(line) {
            run.event_count += 1;
            run.host_telemetry = true;
            if ts > 0.0 {
                run.drone_timestamps.push(ts);
                run.last_activity_sec = run.last_activity_sec.max(ts);
            }
        } else if AGENT_CREATED.is_match(line) {
            run.event_count += 1;
            run.host_telemetry = true;
            if !AGENT_EXCLUDE.is_match(line) {
                run.spawn_events.push(match AGENT_FULL.captures(line) {
                    Some(c) => SpawnEvent {
                        name: Some(c[1].to_string()),
                        tick: c[3].parse().ok(),
                    },
                    None => SpawnEvent {
                        name: AGENT_NPC_NAME.captures(line).map(|c| c[1].to_string()),
                        tick: None,
                    },
                });
            }
        }

        if ts > 0.0 && SLEEP_BETWEEN_3.is_match(line) {
            run.wave_ends.push(ts);
        }
        if ts > 0.0 && WAVE_COUNTDOWN.is_match(line) {
            run.event_count += 1;
            run.wave_countdowns.push(ts);
        }
        None
    }

    /// Closes the active run and returns its summary; None when idle.
    pub fn finalize(&mut self) -> Option<ArbiParsedRun> {
        let r = self.run.take()?;
        let drones = r.drone_timestamps.len() as u32;
        let total_enemies = r.count_valid_enemies() + drones;

        let start = r
            .precise_start_sec
            .or(r.drone_timestamps.first().copied())
            .or(r.mission_start_sec)
            .unwrap_or(r.run_start_sec);
        let mut duration = (r.last_activity_sec - start).max(0.0);
        // No real combat window (early abort; load-time drone spawns span a few ms):
        // the end marker pins the actual mission window.
        if duration < 1.0
            && let Some(end) = r.run_end_sec
        {
            duration = (end - start).max(0.0);
        }

        let stats = r.has_full_stats().then(|| {
            let avg_drone_interval_sec = (drones > 1).then(|| {
                let sum: f64 = r.drone_timestamps.windows(2).map(|p| p[1] - p[0]).sum();
                sum / (drones - 1) as f64
            });
            let (mean, std) = compute_vitus_model(r.rotations, r.waves_per_rotation, drones);
            // A run that ends on a reward screen never logs the unpause that closes it.
            let mut pauses = r.pause_intervals.clone();
            if let Some(s) = r.current_pause_start {
                pauses.push(ArbiInterval {
                    start: s,
                    end: r.last_activity_sec,
                });
            }
            let last = r.last_activity_sec;
            ArbiRunStats {
                kills_per_drone: if drones > 0 {
                    total_enemies as f64 / drones as f64
                } else {
                    0.0
                },
                avg_drone_interval_sec,
                expected_vitus_mean: mean,
                expected_vitus_std: std,
                vitus_per_min: if duration > 0.0 {
                    mean / (duration / 60.0)
                } else {
                    0.0
                },
                waves_per_rotation: r.waves_per_rotation,
                drone_timestamps: r.drone_timestamps.clone(),
                reward_timestamps: r.reward_timestamps.clone(),
                precise_start_sec: r.precise_start_sec,
                last_activity_sec: last,
                saturation_buckets: r.build_saturation(start, last),
                waves: match r.mission_type {
                    ArbiMissionType::Defense => Some(r.build_waves()),
                    ArbiMissionType::Disruption => Some(r.build_rounds()),
                    _ => None,
                },
                pause_intervals: merge_intervals(&pauses, Some(ArbiInterval { start, end: last })),
                idle_intervals: r.build_idle_intervals(start, last),
                rotation_saturation_pct: r.build_rotation_saturation(start),
                spawn_points: r.build_spawn_points(),
                spawn_data_version: ARBI_SPAWN_DATA_VERSION,
            }
        });

        Some(ArbiParsedRun {
            mission_name: r.mission_name,
            node: r.node,
            mission_type: r.mission_type,
            mission_type_raw: r.mission_type_raw,
            sol_node: r.sol_node,
            run_start_sec: r.run_start_sec,
            run_end_sec: r.run_end_sec,
            last_activity_sec: r.last_activity_sec,
            duration_sec: duration,
            rotations: r.rotations,
            drones,
            total_enemies,
            host_telemetry: r.host_telemetry,
            players: r.players,
            stats,
        })
    }
}

#[cfg(test)]
mod tests;
