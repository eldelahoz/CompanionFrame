//! Arbitration schedule: the community `arbys.txt` rota, one-shot and per-node alerts.
//! The parsing, windowing and alert maths are pure; `ArbiSchedule` holds the state and
//! persists to `kv_cache` (no migration), and `setup` runs the fetch and alert loops.

use crate::services::clock::now_ms;
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::db::Db;
use crate::services::bundled_data::BundledData;
use crate::services::cache::{kv_get, kv_put};
use crate::services::item_db::ItemDbState;
use crate::services::net;
use crate::services::notify::channels::NotificationSource;
use crate::services::notify::{NotificationDispatch, Notifications, Notifier};
use crate::services::world_state::parser::title_case;
use crate::services::world_state::regions::{Ctx, faction_label};

const ARBYS_URL: &str = "https://browse.wf/arbys.txt";
const MAX_BYTES: usize = 1 << 20;
const REFRESH_SECS: u64 = 3600;
const STALE_AFTER_MS: i64 = 55 * 60_000;
const ALERT_CHECK_SECS: u64 = 30;
const FIRST_FETCH_DELAY_SECS: u64 = 5;
/// Keep entries from the still-active hour; drop older.
const PAST_WINDOW_MS: i64 = 3_600_000;
const YEAR_MS: i64 = 365 * 24 * 3_600_000;
/// Alert-state keys older than this are pruned.
const FIRED_RETENTION_MS: i64 = 2 * 3_600_000;
const MIN_LEAD: u32 = 1;
const MAX_LEAD: u32 = 120;
const DEFAULT_LEAD: u32 = 5;
const STORE_KEY: &str = "arbi_sched_alerts";
const CACHE_KEY: &str = "arbi_sched_raw";

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiScheduleEntry {
    pub epoch_ms: f64,
    /// Star chart node id, e.g. "SolNode149".
    pub node_id: String,
    /// Display label, e.g. "Casta (Ceres)".
    pub node: String,
    pub mission: String,
    pub faction: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiScheduleAlerts {
    /// One-shot bells keyed "epochMs:nodeId"; cleared after firing.
    pub occurrences: Vec<String>,
    /// Node ids that alert on every future occurrence.
    pub favorite_nodes: Vec<String>,
    pub minutes_before: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiSchedulePayload {
    pub entries: Vec<ArbiScheduleEntry>,
    /// Wall clock of the last successful fetch; null before the first one.
    pub fetched_at: Option<f64>,
    pub alerts: ArbiScheduleAlerts,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArbiRow {
    pub epoch: i64,
    pub node_id: String,
}

fn valid_node_id(s: &str) -> bool {
    (2..=120).contains(&s.len())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:-".contains(c))
}

/// `"<epoch ms>:<node id>"`, the shape of [`occurrence_key`].
fn valid_occurrence_key(s: &str) -> bool {
    s.split_once(':').is_some_and(|(epoch, node)| {
        (10..=17).contains(&epoch.len())
            && epoch.bytes().all(|b| b.is_ascii_digit())
            && valid_node_id(node)
    })
}

pub fn occurrence_key(epoch_ms: i64, node_id: &str) -> String {
    format!("{epoch_ms}:{node_id}")
}

pub fn parse_arbys_text(text: &str) -> Vec<ArbiRow> {
    text.lines()
        .filter_map(|line| {
            let (epoch, node) = line.trim().split_once(',')?;
            let node = node.split(',').next()?.trim();
            let epoch = epoch.trim().parse::<i64>().ok().filter(|e| *e > 0)?;
            valid_node_id(node).then(|| ArbiRow {
                epoch,
                node_id: node.to_owned(),
            })
        })
        .collect()
}

/// The node's own mission name from DE's data; the `MT_` tail when it has none.
fn mission_label(ctx: &Ctx, region: Option<&Value>) -> String {
    if let Some(name) = ctx.localized(region.and_then(|r| r.get("missionName"))) {
        return title_case(&name);
    }
    match region
        .and_then(|r| r.get("missionType"))
        .and_then(Value::as_str)
        .and_then(|mt| mt.strip_prefix("MT_"))
    {
        Some(tail) => title_case(&tail.replace('_', " ")),
        None => "Unknown".into(),
    }
}

pub fn build_entries(rows: &[ArbiRow], ctx: &Ctx) -> Vec<ArbiScheduleEntry> {
    rows.iter()
        .map(|row| {
            let region = ctx.region(&row.node_id);
            ArbiScheduleEntry {
                epoch_ms: (row.epoch * 1000) as f64,
                node_id: row.node_id.clone(),
                node: ctx.node_label(&row.node_id),
                mission: mission_label(ctx, region),
                faction: faction_label(
                    region
                        .and_then(|r| r.get("faction"))
                        .and_then(Value::as_str),
                ),
            }
        })
        .collect()
}

fn epoch_i64(e: &ArbiScheduleEntry) -> i64 {
    e.epoch_ms as i64
}

/// Site behaviour: the still-active hour plus up to a year ahead, ascending.
pub fn filter_window(entries: &[ArbiScheduleEntry], now_ms: i64) -> Vec<ArbiScheduleEntry> {
    let mut out: Vec<_> = entries
        .iter()
        .filter(|e| (now_ms - PAST_WINDOW_MS..=now_ms + YEAR_MS).contains(&epoch_i64(e)))
        .cloned()
        .collect();
    out.sort_by_key(epoch_i64);
    out
}

/// Unfired alerts inside their lead window, as `(entry, key)`.
pub fn due_alerts<'a>(
    entries: &'a [ArbiScheduleEntry],
    alerts: &ArbiScheduleAlerts,
    fired: &HashSet<String>,
    now_ms: i64,
) -> Vec<(&'a ArbiScheduleEntry, String)> {
    let lead = i64::from(alerts.minutes_before) * 60_000;
    entries
        .iter()
        .filter_map(|e| {
            let at = epoch_i64(e);
            if now_ms < at - lead || now_ms >= at {
                return None;
            }
            let key = occurrence_key(at, &e.node_id);
            let wanted =
                alerts.occurrences.contains(&key) || alerts.favorite_nodes.contains(&e.node_id);
            (wanted && !fired.contains(&key)).then_some((e, key))
        })
        .collect()
}

/// Drop alert-state keys whose occurrence is comfortably in the past.
pub fn prune_keys(keys: &mut Vec<String>, now_ms: i64) {
    keys.retain(|k| {
        k.split(':')
            .next()
            .and_then(|e| e.parse::<i64>().ok())
            .is_some_and(|e| e >= now_ms - FIRED_RETENTION_MS)
    });
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AlertStore {
    occurrences: Vec<String>,
    favorite_nodes: Vec<String>,
    minutes_before: u32,
    fired_keys: Vec<String>,
}

impl Default for AlertStore {
    fn default() -> Self {
        Self {
            occurrences: vec![],
            favorite_nodes: vec![],
            minutes_before: DEFAULT_LEAD,
            fired_keys: vec![],
        }
    }
}

impl AlertStore {
    /// Reads whatever was saved; anything malformed is dropped rather than trusted.
    fn revive(raw: &str) -> Self {
        let v: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
        let strings = |k: &str, ok: fn(&str) -> bool| -> Vec<String> {
            let mut seen = HashSet::new();
            v.get(k)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|s| ok(s) && seen.insert(s.to_owned()))
                .map(str::to_owned)
                .collect()
        };
        let lead = v
            .get("minutesBefore")
            .and_then(Value::as_f64)
            .filter(|m| (f64::from(MIN_LEAD)..=f64::from(MAX_LEAD)).contains(m))
            .map_or(DEFAULT_LEAD, |m| m as u32);
        Self {
            occurrences: strings("occurrences", valid_occurrence_key),
            favorite_nodes: strings("favoriteNodes", valid_node_id),
            minutes_before: lead,
            fired_keys: strings("firedKeys", valid_occurrence_key),
        }
    }

    fn alerts(&self) -> ArbiScheduleAlerts {
        ArbiScheduleAlerts {
            occurrences: self.occurrences.clone(),
            favorite_nodes: self.favorite_nodes.clone(),
            minutes_before: self.minutes_before,
        }
    }
}

fn toggle(list: &mut Vec<String>, item: &str, on: bool) {
    let has = list.iter().any(|k| k == item);
    if on && !has {
        list.push(item.to_owned());
    } else if !on {
        list.retain(|k| k != item);
    }
}

#[derive(Default)]
struct Inner {
    entries: Vec<ArbiScheduleEntry>,
    fetched_at: Option<i64>,
    store: AlertStore,
    loaded: bool,
}

#[derive(Default)]
pub struct ArbiSchedule {
    inner: Mutex<Inner>,
}

impl ArbiSchedule {
    fn payload(&self, now_ms: i64) -> ArbiSchedulePayload {
        let i = self.inner.lock().unwrap();
        ArbiSchedulePayload {
            entries: filter_window(&i.entries, now_ms),
            fetched_at: i.fetched_at.map(|t| t as f64),
            alerts: i.store.alerts(),
        }
    }

    fn stale(&self, now_ms: i64) -> bool {
        let i = self.inner.lock().unwrap();
        i.fetched_at.is_none_or(|t| now_ms - t > STALE_AFTER_MS)
    }

    /// Mutates the alert store and returns the alerts to persist (the caller saves outside the lock).
    fn edit(
        &self,
        f: impl FnOnce(&mut AlertStore) -> bool,
    ) -> Option<(ArbiScheduleAlerts, String)> {
        let mut i = self.inner.lock().unwrap();
        if !f(&mut i.store) {
            return None;
        }
        let json = serde_json::to_string(&i.store).ok()?;
        Some((i.store.alerts(), json))
    }

    /// Fires every due alert through `send`, consuming its bell. Returns the store JSON when
    /// anything changed. With `deliverable` false nothing is consumed: the bells wait.
    fn check(
        &self,
        now_ms: i64,
        deliverable: bool,
        mut send: impl FnMut(&str, String),
    ) -> Option<String> {
        let mut i = self.inner.lock().unwrap();
        if i.entries.is_empty() {
            return None;
        }
        let before = i.store.clone();
        prune_keys(&mut i.store.occurrences, now_ms);
        prune_keys(&mut i.store.fired_keys, now_ms);
        if deliverable {
            let fired: HashSet<String> = i.store.fired_keys.iter().cloned().collect();
            let alerts = i.store.alerts();
            let due: Vec<(ArbiScheduleEntry, String)> =
                due_alerts(&i.entries, &alerts, &fired, now_ms)
                    .into_iter()
                    .map(|(e, k)| (e.clone(), k))
                    .collect();
            for (e, key) in due {
                i.store.fired_keys.push(key.clone());
                i.store.occurrences.retain(|k| *k != key);
                let mins = ((epoch_i64(&e) - now_ms) as f64 / 60_000.0).round() as i64;
                let timing = if mins > 0 {
                    format!("starts in ~{mins} min")
                } else {
                    "starting now".into()
                };
                send(
                    "Arbitration Alert",
                    format!("{} on {} vs {} - {timing}.", e.mission, e.node, e.faction),
                );
            }
        }
        (i.store != before)
            .then(|| serde_json::to_string(&i.store).ok())
            .flatten()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCache {
    fetched_at: i64,
    raw: String,
}

fn load_ctx(app: &AppHandle) -> Result<Ctx, String> {
    let bundled = BundledData::from_app(app)?;
    Ok(Ctx::load(&bundled, app.state::<ItemDbState>().ui_code()))
}

/// Loads the saved alerts and the cached rota once, so the view has data before the network does.
fn ensure_loaded(app: &AppHandle) {
    let (state, db) = (app.state::<ArbiSchedule>(), app.state::<Db>());
    if std::mem::replace(&mut state.inner.lock().unwrap().loaded, true) {
        return;
    }
    let store = kv_get(&db, STORE_KEY)
        .ok()
        .flatten()
        .map(|raw| AlertStore::revive(&raw));
    let cached = kv_get(&db, CACHE_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<RawCache>(&raw).ok());
    let entries = cached.as_ref().and_then(|c| {
        let rows = parse_arbys_text(&c.raw);
        let ctx = load_ctx(app).ok()?;
        Some(build_entries(&rows, &ctx))
    });
    let mut i = state.inner.lock().unwrap();
    if let Some(s) = store {
        i.store = s;
    }
    if let (Some(c), Some(e)) = (cached, entries) {
        i.entries = e;
        i.fetched_at = Some(c.fetched_at);
    }
}

async fn refresh(app: &AppHandle) {
    let result: Result<(), String> = async {
        let client = net::client().map_err(|e| e.to_string())?;
        let net::Fetched::Modified { body, .. } =
            net::fetch(&client, ARBYS_URL, None, MAX_BYTES).await?
        else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(&body).into_owned();
        let rows = parse_arbys_text(&text);
        if rows.is_empty() {
            return Err("no valid arbitration entries".into());
        }
        let ctx = load_ctx(app)?;
        let entries = build_entries(&rows, &ctx);
        let at = now_ms();
        let saved = serde_json::to_string(&RawCache {
            fetched_at: at,
            raw: text,
        })
        .map_err(|e| e.to_string())?;
        kv_put(&app.state::<Db>(), CACHE_KEY, &saved).map_err(|e| e.to_string())?;
        let state = app.state::<ArbiSchedule>();
        let mut i = state.inner.lock().unwrap();
        i.entries = entries;
        i.fetched_at = Some(at);
        Ok(())
    }
    .await;
    // Keep serving the previous (possibly cached) rota.
    if let Err(e) = result {
        tracing::warn!("arbi schedule fetch failed: {e}");
    }
}

/// A fired alert consumes its bell, so only fire while some channel can carry it.
fn deliverable(n: &Notifications) -> bool {
    let s = n.state();
    let Some(route) = s.sources.get(&NotificationSource::ArbiSchedule) else {
        return false;
    };
    route.native || (route.webhook && s.webhooks.values().any(|w| w.configured))
}

pub fn setup(app: &tauri::App) {
    app.manage(ArbiSchedule::default());
    let h = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        ensure_loaded(&h);
        tokio::time::sleep(Duration::from_secs(FIRST_FETCH_DELAY_SECS)).await;
        let mut fetch_in = REFRESH_SECS;
        loop {
            if fetch_in >= REFRESH_SECS {
                fetch_in = 0;
                refresh(&h).await;
            }
            let (state, db, notif) = (
                h.state::<ArbiSchedule>(),
                h.state::<Db>(),
                h.state::<Notifications>(),
            );
            let json = state.check(now_ms(), deliverable(&notif), |title, body| {
                notif.notify(NotificationDispatch::new(
                    NotificationSource::ArbiSchedule,
                    title,
                    body,
                ));
            });
            if let Some(json) = json {
                let _ = kv_put(&db, STORE_KEY, &json);
            }
            tokio::time::sleep(Duration::from_secs(ALERT_CHECK_SECS)).await;
            fetch_in += ALERT_CHECK_SECS;
        }
    });
}

fn persist(db: &Db, saved: Option<(ArbiScheduleAlerts, String)>) -> Option<ArbiScheduleAlerts> {
    let (alerts, json) = saved?;
    let _ = kv_put(db, STORE_KEY, &json);
    Some(alerts)
}

#[tauri::command]
#[specta::specta]
pub async fn arbi_schedule_get(app: AppHandle) -> ArbiSchedulePayload {
    ensure_loaded(&app);
    if app.state::<ArbiSchedule>().stale(now_ms()) {
        refresh(&app).await;
    }
    app.state::<ArbiSchedule>().payload(now_ms())
}

#[tauri::command]
#[specta::specta]
pub fn arbi_schedule_set_occurrence(
    app: AppHandle,
    state: State<'_, ArbiSchedule>,
    db: State<'_, Db>,
    key: String,
    enabled: bool,
) -> Option<ArbiScheduleAlerts> {
    ensure_loaded(&app);
    if !valid_occurrence_key(&key) {
        return None;
    }
    persist(
        &db,
        state.edit(|s| {
            toggle(&mut s.occurrences, &key, enabled);
            true
        }),
    )
}

#[tauri::command]
#[specta::specta]
pub fn arbi_schedule_set_favorite(
    app: AppHandle,
    state: State<'_, ArbiSchedule>,
    db: State<'_, Db>,
    node_id: String,
    enabled: bool,
) -> Option<ArbiScheduleAlerts> {
    ensure_loaded(&app);
    if !valid_node_id(&node_id) {
        return None;
    }
    persist(
        &db,
        state.edit(|s| {
            toggle(&mut s.favorite_nodes, &node_id, enabled);
            true
        }),
    )
}

#[tauri::command]
#[specta::specta]
pub fn arbi_schedule_set_lead(
    app: AppHandle,
    state: State<'_, ArbiSchedule>,
    db: State<'_, Db>,
    minutes: f64,
) -> Option<ArbiScheduleAlerts> {
    ensure_loaded(&app);
    if !(f64::from(MIN_LEAD)..=f64::from(MAX_LEAD)).contains(&minutes) {
        return None;
    }
    persist(
        &db,
        state.edit(|s| {
            s.minutes_before = minutes as u32;
            true
        }),
    )
}

#[cfg(test)]
mod tests;
