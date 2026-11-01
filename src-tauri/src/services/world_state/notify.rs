//! World-state notifications: Baro / Varzia arrivals, enabled cycle changes, cycle lead-time
//! heads-ups and fissures matching the user's alert rules. [`WorldNotifier`] sits in front of the
//! real notifier, compares each refreshed state with the last one it saw and sends
//! `NotificationSource::WorldState` dispatches (so channel routing, history and sound all apply).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{Fissure, WorldState};
use crate::services::atomic::write_atomic;
use crate::services::notify::channels::NotificationSource;
use crate::services::notify::{NotificationDispatch, Notifier};

const MAX_FISSURE_RULES: usize = 50;
const MAX_LEAD_MINUTES: u32 = 120;
/// A pre-cycle key is forgotten once its cycle ended this long ago.
const EVICT_AFTER_MS: i64 = 300_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldCycleAlerts {
    pub earth: bool,
    pub cetus: bool,
    pub vallis: bool,
    pub cambion: bool,
    pub duviri: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WorldSteelPathFilter {
    #[default]
    Any,
    Normal,
    Steel,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldFissureAlert {
    pub id: String,
    /// `any` or a tier name (Lith, Meso, Neo, Axi, Requiem, Omnia).
    pub tier: String,
    /// `any` or a mission label as the parser emits it.
    pub mission_type: String,
    pub steel_path: WorldSteelPathFilter,
    /// `any`/empty or a planet name matched against the node's `(Planet)` suffix.
    pub planet: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldNotifyPrefs {
    /// The legacy "world notifications" switch: mutes the desktop toast only, webhooks are routed
    /// per source by the channel layer.
    pub enabled: bool,
    pub cycle_alerts: WorldCycleAlerts,
    /// Minutes before a cycle ends to send a heads-up; 0 disables.
    pub cycle_alert_minutes_before: u32,
    pub fissure_alerts: Vec<WorldFissureAlert>,
}

impl Default for WorldNotifyPrefs {
    fn default() -> Self {
        Self {
            enabled: true,
            cycle_alerts: WorldCycleAlerts::default(),
            cycle_alert_minutes_before: 3,
            fissure_alerts: Vec::new(),
        }
    }
}

impl WorldNotifyPrefs {
    fn sanitized(mut self) -> Self {
        self.cycle_alert_minutes_before = self.cycle_alert_minutes_before.min(MAX_LEAD_MINUTES);
        self.fissure_alerts.truncate(MAX_FISSURE_RULES);
        self
    }
}

/// Atomic JSON file, like `Store`; a missing or corrupt file means defaults.
pub struct WorldNotifyStore {
    path: PathBuf,
    prefs: Mutex<WorldNotifyPrefs>,
}

impl WorldNotifyStore {
    pub fn load(path: PathBuf) -> Self {
        let prefs = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<WorldNotifyPrefs>(&b).ok())
            .unwrap_or_default()
            .sanitized();
        Self {
            path,
            prefs: Mutex::new(prefs),
        }
    }

    pub fn get(&self) -> WorldNotifyPrefs {
        self.prefs.lock().unwrap().clone()
    }

    pub fn set(&self, new: WorldNotifyPrefs) -> std::io::Result<WorldNotifyPrefs> {
        let new = new.sanitized();
        let mut cur = self.prefs.lock().unwrap();
        write_atomic(&self.path, &serde_json::to_vec_pretty(&new)?)?;
        *cur = new.clone();
        Ok(new)
    }
}

fn parse_ms(iso: &Option<String>) -> Option<i64> {
    DateTime::parse_from_rfc3339(iso.as_deref()?)
        .ok()
        .map(|d| d.timestamp_millis())
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn label_key(label: &str) -> String {
    let n = label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    // Rules saved before the parser switched to DE's wording keep working.
    match n.as_str() {
        "extermination" => "exterminate".into(),
        "open world" => "free roam".into(),
        _ => n,
    }
}

/// True when a rule's stored mission type covers an emitted fissure label.
pub fn mission_type_matches(rule: &str, emitted: &str) -> bool {
    rule.is_empty() || rule == "any" || label_key(rule) == label_key(emitted)
}

fn rule_matches(rule: &WorldFissureAlert, f: &Fissure) -> bool {
    let tier_ok = rule.tier == "any" || rule.tier.eq_ignore_ascii_case(&f.tier);
    let sp_ok = match rule.steel_path {
        WorldSteelPathFilter::Any => true,
        WorldSteelPathFilter::Steel => f.is_hard,
        WorldSteelPathFilter::Normal => !f.is_hard,
    };
    let planet_ok = rule.planet.is_empty()
        || rule.planet == "any"
        || f.node
            .to_lowercase()
            .contains(&format!("({})", rule.planet.to_lowercase()));
    tier_ok && mission_type_matches(&rule.mission_type, &f.mission_type) && sp_ok && planet_ok
}

fn fissure_id(f: &Fissure) -> String {
    format!(
        "{}|{}|{}|{}",
        f.tier,
        f.node,
        f.expiry.as_deref().unwrap_or(""),
        if f.is_hard { "1" } else { "0" }
    )
}

type Body = fn(&str) -> String;
/// (enabled, prev, next, title, body) of one cycle transition rule.
type Transition = (bool, Option<String>, Option<String>, &'static str, Body);

/// What the next refresh is compared against.
struct Snapshot {
    baro_active: bool,
    varzia_expiry: Option<String>,
    varzia_location: String,
    earth: Option<bool>,
    earth_expiry: Option<String>,
    cetus: Option<bool>,
    cetus_expiry: Option<String>,
    vallis_warm: Option<bool>,
    vallis_expiry: Option<String>,
    cambion: Option<String>,
    cambion_expiry: Option<String>,
    duviri: Option<String>,
    duviri_expiry: Option<String>,
    fissures: HashSet<String>,
}

impl Snapshot {
    fn build(s: &WorldState, now_ms: i64) -> Self {
        let baro_active = s.void_trader.as_ref().is_some_and(|t| {
            let Some(expiry) = parse_ms(&t.expiry) else {
                return false;
            };
            parse_ms(&t.activation).is_none_or(|a| now_ms >= a) && now_ms < expiry
        });
        let varzia = s.vault_trader.as_ref();
        Self {
            baro_active,
            varzia_expiry: varzia.and_then(|t| t.expiry.clone()),
            varzia_location: varzia
                .map(|t| t.location.clone())
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| "Varzia".into()),
            earth: s.earth_cycle.as_ref().map(|c| c.is_day),
            earth_expiry: s.earth_cycle.as_ref().map(|c| c.expiry.clone()),
            cetus: s.cetus_cycle.as_ref().map(|c| c.is_day),
            cetus_expiry: s.cetus_cycle.as_ref().map(|c| c.expiry.clone()),
            vallis_warm: s.vallis_cycle.as_ref().map(|c| c.is_warm),
            vallis_expiry: s.vallis_cycle.as_ref().map(|c| c.expiry.clone()),
            cambion: s.cambion_cycle.as_ref().map(|c| c.active.to_lowercase()),
            cambion_expiry: s.cambion_cycle.as_ref().map(|c| c.expiry.clone()),
            duviri: s
                .duviri_cycle
                .as_ref()
                .and_then(|c| c.state.as_ref())
                .map(|v| v.to_lowercase()),
            duviri_expiry: s.duviri_cycle.as_ref().and_then(|c| c.expiry.clone()),
            fissures: s
                .fissures
                .iter()
                .filter(|f| !f.expired)
                .map(fissure_id)
                .collect(),
        }
    }
}

/// Compares successive world states and notifies through `inner`.
pub struct WorldNotifier {
    inner: Arc<dyn Notifier>,
    prefs: Arc<WorldNotifyStore>,
    last: Mutex<Option<Snapshot>>,
    /// `{cycle}:{expiry}` keys a heads-up already went out for.
    pre_notified: Mutex<HashSet<String>>,
}

impl WorldNotifier {
    pub fn new(inner: Arc<dyn Notifier>, prefs: Arc<WorldNotifyStore>) -> Self {
        Self {
            inner,
            prefs,
            last: Mutex::new(None),
            pre_notified: Mutex::new(HashSet::new()),
        }
    }

    fn send(&self, prefs: &WorldNotifyPrefs, title: impl Into<String>, body: impl Into<String>) {
        let mut d = NotificationDispatch::new(NotificationSource::WorldState, title, body);
        d.native_enabled = prefs.enabled;
        self.inner.notify(d);
    }

    /// Diffs `next` against the previous snapshot. The first state only seeds it.
    pub fn step(&self, next: &WorldState, now_ms: i64) {
        let snap = Snapshot::build(next, now_ms);
        let prev = self.last.lock().unwrap().replace(snap);
        if let Some(prev) = prev {
            let snap = self.last.lock().unwrap();
            self.diff(&prev, snap.as_ref().unwrap(), next);
        }
        self.check_pre_cycle(now_ms);
    }

    fn diff(&self, prev: &Snapshot, next: &Snapshot, state: &WorldState) {
        let prefs = self.prefs.get();

        if !prev.baro_active && next.baro_active {
            let location = state
                .void_trader
                .as_ref()
                .map(|t| t.location.as_str())
                .filter(|l| !l.is_empty())
                .unwrap_or("Relay");
            self.send(
                &prefs,
                "Baro Ki'Teer Arrived",
                format!("Now available at {location}."),
            );
        }

        if prev.varzia_expiry.is_some()
            && next.varzia_expiry.is_some()
            && prev.varzia_expiry != next.varzia_expiry
        {
            self.send(
                &prefs,
                "Prime Resurgence Rotation Updated",
                format!("New rotation at {}.", next.varzia_location),
            );
        }

        let day = |v: bool| if v { "1" } else { "0" }.to_string();
        let flag = |v: Option<bool>| v.map(day);
        let a = &prefs.cycle_alerts;
        let transitions: [Transition; 5] = [
            (
                a.earth,
                flag(prev.earth),
                flag(next.earth),
                "Earth Cycle",
                |v| {
                    if v == "1" {
                        "Day has begun."
                    } else {
                        "Night has begun."
                    }
                    .into()
                },
            ),
            (
                a.cetus,
                flag(prev.cetus),
                flag(next.cetus),
                "Cetus Cycle",
                |v| {
                    if v == "1" {
                        "Day has begun."
                    } else {
                        "Night has begun."
                    }
                    .into()
                },
            ),
            (
                a.vallis,
                flag(prev.vallis_warm),
                flag(next.vallis_warm),
                "Orb Vallis Cycle",
                |v| {
                    if v == "1" {
                        "Warm cycle has begun."
                    } else {
                        "Cold cycle has begun."
                    }
                    .into()
                },
            ),
            (
                a.cambion,
                prev.cambion.clone(),
                next.cambion.clone(),
                "Cambion Drift Cycle",
                |v| format!("{} cycle has begun.", v.to_uppercase()),
            ),
            (
                a.duviri,
                prev.duviri.clone(),
                next.duviri.clone(),
                "Duviri Cycle",
                |v| format!("{} mood has begun.", capitalize(v)),
            ),
        ];
        for (enabled, p, n, title, body) in &transitions {
            if let (true, Some(p), Some(n)) = (enabled, p, n)
                && p != n
            {
                self.send(&prefs, *title, body(n));
            }
        }

        // Only fissures that were not there last time.
        if prefs.fissure_alerts.is_empty() {
            return;
        }
        for f in state.fissures.iter().filter(|f| !f.expired) {
            if prev.fissures.contains(&fissure_id(f))
                || !prefs.fissure_alerts.iter().any(|r| rule_matches(r, f))
            {
                continue;
            }
            let sp = if f.is_hard { " (Steel Path)" } else { "" };
            let node = if f.node.is_empty() {
                "Unknown Node"
            } else {
                &f.node
            };
            self.send(
                &prefs,
                "Fissure Alert",
                format!("{} {}{sp} - {node}", f.tier, f.mission_type),
            );
        }
    }

    /// Lead-time heads-up for enabled cycles, from the last snapshot. Cheap, so a timer calls it
    /// between refreshes to catch the moment a cycle enters its window.
    pub fn check_pre_cycle(&self, now_ms: i64) {
        let prefs = self.prefs.get();
        if prefs.cycle_alert_minutes_before == 0 {
            return;
        }
        let lead_ms = i64::from(prefs.cycle_alert_minutes_before) * 60_000;
        let guard = self.last.lock().unwrap();
        let Some(s) = guard.as_ref() else { return };
        let a = &prefs.cycle_alerts;
        let upcoming = [
            (
                "earth",
                a.earth,
                &s.earth_expiry,
                if s.earth == Some(true) {
                    "Night"
                } else {
                    "Day"
                }
                .to_string(),
            ),
            (
                "cetus",
                a.cetus,
                &s.cetus_expiry,
                if s.cetus == Some(true) {
                    "Night"
                } else {
                    "Day"
                }
                .to_string(),
            ),
            (
                "vallis",
                a.vallis,
                &s.vallis_expiry,
                if s.vallis_warm == Some(true) {
                    "Cold"
                } else {
                    "Warm"
                }
                .to_string(),
            ),
            (
                "cambion",
                a.cambion,
                &s.cambion_expiry,
                if s.cambion.as_deref() == Some("fass") {
                    "VOME"
                } else {
                    "FASS"
                }
                .to_string(),
            ),
            (
                "duviri",
                a.duviri,
                &s.duviri_expiry,
                s.duviri
                    .as_deref()
                    .map(capitalize)
                    .unwrap_or_else(|| "Unknown".into()),
            ),
        ];
        let mut done = self.pre_notified.lock().unwrap();
        for (key, enabled, expiry, label) in upcoming {
            let (true, Some(expiry_iso)) = (enabled, expiry) else {
                continue;
            };
            let Some(expiry_ms) = parse_ms(&Some(expiry_iso.clone())) else {
                continue;
            };
            let remaining = expiry_ms - now_ms;
            let pre_key = format!("{key}:{expiry_iso}");
            if remaining > 0 && remaining <= lead_ms && done.insert(pre_key.clone()) {
                let mins = (remaining + 59_999) / 60_000;
                self.send(
                    &prefs,
                    format!("{} Cycle", capitalize(key)),
                    format!(
                        "{label} in ~{mins} min{}.",
                        if mins == 1 { "" } else { "s" }
                    ),
                );
            }
            // Evict old entries to prevent memory growth.
            if remaining < -EVICT_AFTER_MS {
                done.remove(&pre_key);
            }
        }
    }
}

impl Notifier for WorldNotifier {
    fn notify(&self, d: NotificationDispatch) {
        self.inner.notify(d);
    }

    fn world_state_changed(&self, _prev: Option<&WorldState>, next: &WorldState) {
        self.step(next, chrono::Utc::now().timestamp_millis());
    }
}

#[tauri::command]
#[specta::specta]
pub fn world_notify_get(store: State<'_, Arc<WorldNotifyStore>>) -> WorldNotifyPrefs {
    store.get()
}

#[tauri::command]
#[specta::specta]
pub fn world_notify_set(
    store: State<'_, Arc<WorldNotifyStore>>,
    prefs: WorldNotifyPrefs,
) -> Result<WorldNotifyPrefs, String> {
    store.set(prefs).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::world_state::{CambionCycle, DayCycle, Trader, VallisCycle};

    #[derive(Default)]
    struct Recorder(Mutex<Vec<NotificationDispatch>>);

    impl Notifier for Recorder {
        fn notify(&self, d: NotificationDispatch) {
            self.0.lock().unwrap().push(d);
        }
    }

    struct Rig {
        n: WorldNotifier,
        out: Arc<Recorder>,
        store: Arc<WorldNotifyStore>,
    }

    impl Rig {
        fn new(prefs: WorldNotifyPrefs) -> Self {
            let dir = std::env::temp_dir().join(format!("cf-wn-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            let store = Arc::new(WorldNotifyStore::load(dir.join("w.json")));
            store.set(prefs).unwrap();
            let out = Arc::new(Recorder::default());
            Self {
                n: WorldNotifier::new(out.clone(), store.clone()),
                out,
                store,
            }
        }

        fn sent(&self) -> Vec<(String, String)> {
            self.out
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|d| (d.title.clone(), d.body.clone()))
                .collect()
        }
    }

    fn quiet() -> WorldNotifyPrefs {
        WorldNotifyPrefs {
            cycle_alert_minutes_before: 0,
            ..Default::default()
        }
    }

    fn rule(
        tier: &str,
        mission: &str,
        sp: WorldSteelPathFilter,
        planet: &str,
    ) -> WorldFissureAlert {
        WorldFissureAlert {
            id: "r".into(),
            tier: tier.into(),
            mission_type: mission.into(),
            steel_path: sp,
            planet: planet.into(),
        }
    }

    fn fissure(tier: &str, mission: &str, node: &str, hard: bool) -> Fissure {
        Fissure {
            tier: tier.into(),
            mission_type: mission.into(),
            node: node.into(),
            expiry: Some("2026-04-29T12:00:00.000Z".into()),
            is_hard: hard,
            ..Default::default()
        }
    }

    fn with_fissures(f: Vec<Fissure>) -> WorldState {
        WorldState {
            fissures: f,
            ..Default::default()
        }
    }

    fn earth(is_day: bool, expiry: &str) -> WorldState {
        WorldState {
            earth_cycle: Some(DayCycle {
                is_day,
                time_left: String::new(),
                expiry: expiry.into(),
            }),
            ..Default::default()
        }
    }

    const NOW: i64 = 1_777_000_000_000;

    #[test]
    fn fires_when_a_matching_fissure_appears() {
        let rig = Rig::new(WorldNotifyPrefs {
            fissure_alerts: vec![rule("Axi", "Capture", WorldSteelPathFilter::Normal, "any")],
            ..quiet()
        });
        rig.n.step(&with_fissures(vec![]), NOW);
        rig.n.step(
            &with_fissures(vec![fissure("Axi", "Capture", "Marduk (Void)", false)]),
            NOW,
        );
        let sent = rig.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "Fissure Alert");
        assert_eq!(sent[0].1, "Axi Capture - Marduk (Void)");
    }

    #[test]
    fn fires_for_a_rule_saved_with_the_old_extermination_wording() {
        let rig = Rig::new(WorldNotifyPrefs {
            fissure_alerts: vec![
                rule("any", "Extermination", WorldSteelPathFilter::Any, "any"),
                rule("any", "spy", WorldSteelPathFilter::Any, "any"),
            ],
            ..quiet()
        });
        rig.n.step(&with_fissures(vec![]), NOW);
        rig.n.step(
            &with_fissures(vec![
                fissure("Lith", "Exterminate", "Gaia (Earth)", false),
                fissure("Meso", "Spy", "Ose (Europa)", false),
                fissure("Neo", "Interception", "Umbriel (Uranus)", false),
            ]),
            NOW,
        );
        let bodies: Vec<_> = rig.sent().into_iter().map(|s| s.1).collect();
        assert_eq!(bodies.len(), 2);
        assert!(bodies[0].contains("Lith Exterminate") && bodies[1].contains("Meso Spy"));
    }

    #[test]
    fn respects_every_filter() {
        let rig = Rig::new(WorldNotifyPrefs {
            fissure_alerts: vec![rule(
                "Lith",
                "Survival",
                WorldSteelPathFilter::Steel,
                "Earth",
            )],
            ..quiet()
        });
        rig.n.step(&with_fissures(vec![]), NOW);
        rig.n.step(
            &with_fissures(vec![
                fissure("Meso", "Survival", "Gaia (Earth)", true),
                fissure("Lith", "Defense", "Gaia (Earth)", true),
                fissure("Lith", "Survival", "Gaia (Earth)", false),
                fissure("Lith", "Survival", "Ose (Europa)", true),
                fissure("Lith", "Survival", "Cervantes (Earth)", true),
            ]),
            NOW,
        );
        assert_eq!(
            rig.sent(),
            [(
                "Fissure Alert".into(),
                "Lith Survival (Steel Path) - Cervantes (Earth)".into()
            )]
        );
    }

    #[test]
    fn does_not_repeat_a_fissure_on_refresh_and_skips_expired_ones() {
        let rig = Rig::new(WorldNotifyPrefs {
            fissure_alerts: vec![rule("any", "any", WorldSteelPathFilter::Any, "any")],
            ..quiet()
        });
        let mut gone = fissure("Axi", "Capture", "Old (Void)", false);
        gone.expired = true;
        let live = with_fissures(vec![
            fissure("Axi", "Capture", "Marduk (Void)", false),
            gone,
        ]);
        rig.n.step(&with_fissures(vec![]), NOW);
        rig.n.step(&live, NOW);
        rig.n.step(&live, NOW);
        rig.n.step(&live, NOW);
        assert_eq!(rig.sent().len(), 1);
    }

    #[test]
    fn the_first_state_only_seeds_the_snapshot() {
        let rig = Rig::new(WorldNotifyPrefs {
            fissure_alerts: vec![rule("any", "any", WorldSteelPathFilter::Any, "any")],
            ..quiet()
        });
        rig.n.step(
            &with_fissures(vec![fissure("Axi", "Capture", "Marduk (Void)", false)]),
            NOW,
        );
        assert!(rig.sent().is_empty());
    }

    #[test]
    fn fires_when_an_enabled_cycle_changes_and_only_then() {
        let rig = Rig::new(WorldNotifyPrefs {
            cycle_alerts: WorldCycleAlerts {
                earth: true,
                ..Default::default()
            },
            ..quiet()
        });
        rig.n.step(&earth(true, "2026-04-29T12:00:00.000Z"), NOW);
        rig.n.step(&earth(true, "2026-04-29T12:00:00.000Z"), NOW);
        rig.n.step(&earth(false, "2026-04-29T16:00:00.000Z"), NOW);
        assert_eq!(
            rig.sent(),
            [("Earth Cycle".into(), "Night has begun.".into())]
        );

        let off = Rig::new(quiet());
        off.n.step(&earth(true, "x"), NOW);
        off.n.step(&earth(false, "x"), NOW);
        assert!(off.sent().is_empty());
    }

    #[test]
    fn words_the_other_cycles() {
        let rig = Rig::new(WorldNotifyPrefs {
            cycle_alerts: WorldCycleAlerts {
                vallis: true,
                cambion: true,
                ..Default::default()
            },
            ..quiet()
        });
        let state = |warm, active: &str| WorldState {
            vallis_cycle: Some(VallisCycle {
                is_warm: warm,
                time_left: String::new(),
                expiry: "x".into(),
            }),
            cambion_cycle: Some(CambionCycle {
                active: active.into(),
                time_left: String::new(),
                expiry: "x".into(),
            }),
            ..Default::default()
        };
        rig.n.step(&state(true, "fass"), NOW);
        rig.n.step(&state(false, "vome"), NOW);
        assert_eq!(
            rig.sent(),
            [
                ("Orb Vallis Cycle".into(), "Cold cycle has begun.".into()),
                ("Cambion Drift Cycle".into(), "VOME cycle has begun.".into()),
            ]
        );
    }

    #[test]
    fn announces_baro_arriving_and_a_new_varzia_rotation() {
        let rig = Rig::new(quiet());
        let trader = |act: i64, exp: i64, loc: &str| Trader {
            activation: Some(iso(act)),
            expiry: Some(iso(exp)),
            location: loc.into(),
            inventory: vec![],
        };
        let state = |baro: Trader, varzia: Trader| WorldState {
            void_trader: Some(baro),
            vault_trader: Some(varzia),
            ..Default::default()
        };
        let h = 3_600_000;
        rig.n.step(
            &state(
                trader(NOW + h, NOW + 3 * h, "Larunda Relay"),
                trader(NOW - h, NOW + h, "Maroo"),
            ),
            NOW,
        );
        rig.n.step(
            &state(
                trader(NOW + h, NOW + 3 * h, "Larunda Relay"),
                trader(NOW - h, NOW + h, "Maroo"),
            ),
            NOW,
        );
        assert!(rig.sent().is_empty());
        rig.n.step(
            &state(
                trader(NOW + h, NOW + 3 * h, "Larunda Relay"),
                trader(NOW, NOW + 2 * h, "Maroo"),
            ),
            NOW + h,
        );
        rig.n.step(
            &state(
                trader(NOW + h, NOW + 3 * h, "Larunda Relay"),
                trader(NOW, NOW + 2 * h, "Maroo"),
            ),
            NOW + h,
        );
        assert_eq!(
            rig.sent(),
            [
                (
                    "Baro Ki'Teer Arrived".into(),
                    "Now available at Larunda Relay.".into()
                ),
                (
                    "Prime Resurgence Rotation Updated".into(),
                    "New rotation at Maroo.".into()
                ),
            ]
        );
    }

    fn iso(ms: i64) -> String {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    #[test]
    fn sends_one_heads_up_inside_the_lead_window_then_the_transition() {
        let rig = Rig::new(WorldNotifyPrefs {
            cycle_alerts: WorldCycleAlerts {
                vallis: true,
                ..Default::default()
            },
            cycle_alert_minutes_before: 3,
            ..Default::default()
        });
        let expiry = NOW + 4 * 60_000;
        let state = |warm: bool, exp: i64| WorldState {
            vallis_cycle: Some(VallisCycle {
                is_warm: warm,
                time_left: String::new(),
                expiry: iso(exp),
            }),
            ..Default::default()
        };
        rig.n.step(&state(false, expiry), NOW);
        assert!(rig.sent().is_empty(), "4 min out is outside a 3 min window");
        rig.n.check_pre_cycle(NOW + 75_000);
        rig.n.check_pre_cycle(NOW + 90_000);
        assert_eq!(
            rig.sent(),
            [("Vallis Cycle".into(), "Warm in ~3 mins.".into())]
        );
        rig.n
            .step(&state(true, expiry + 30 * 60_000), expiry + 1_000);
        assert_eq!(
            rig.sent()[1],
            ("Orb Vallis Cycle".into(), "Warm cycle has begun.".into())
        );
        assert_eq!(rig.sent().len(), 2);
    }

    #[test]
    fn the_legacy_switch_mutes_the_toast_only() {
        let rig = Rig::new(WorldNotifyPrefs {
            enabled: false,
            cycle_alerts: WorldCycleAlerts {
                earth: true,
                ..Default::default()
            },
            ..quiet()
        });
        rig.n.step(&earth(true, "x"), NOW);
        rig.n.step(&earth(false, "y"), NOW);
        let d = rig.out.0.lock().unwrap();
        assert_eq!(d.len(), 1);
        assert!(!d[0].native_enabled);
        assert_eq!(d[0].source, NotificationSource::WorldState);
    }

    #[test]
    fn prefs_persist_are_bounded_and_survive_a_corrupt_file() {
        let rig = Rig::new(quiet());
        let saved = rig
            .store
            .set(WorldNotifyPrefs {
                cycle_alert_minutes_before: 9999,
                fissure_alerts: vec![rule("any", "any", WorldSteelPathFilter::Any, ""); 80],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(saved.cycle_alert_minutes_before, 120);
        assert_eq!(saved.fissure_alerts.len(), 50);
        let again = WorldNotifyStore::load(rig.store.path.clone());
        assert_eq!(again.get(), saved);
        std::fs::write(&rig.store.path, b"{").unwrap();
        assert_eq!(
            WorldNotifyStore::load(rig.store.path.clone()).get(),
            WorldNotifyPrefs::default()
        );
    }

    #[test]
    fn mission_labels_match_case_and_spacing_insensitively() {
        assert!(mission_type_matches("any", "Spy"));
        assert!(mission_type_matches("", "Spy"));
        assert!(mission_type_matches(
            "  mobile   DEFENSE ",
            "Mobile Defense"
        ));
        assert!(mission_type_matches("Open World", "Free Roam"));
        assert!(!mission_type_matches("Spy", "Sabotage"));
    }
}
