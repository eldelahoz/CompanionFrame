//! Daily stats from the inventory: per-resource deltas for the local day, persisted in
//! `stats_daily`. Fed by the inventory sink, so it needs no EE.log tail.
mod store;
#[cfg(test)]
mod tests;
mod tracker;

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Listener, Manager, State};

use crate::db::Db;
use tracker::{DailyStatEntry, SessionStats, StatResourceDay, Tracker};

pub const MAX_IMPORT_ROWS: usize = 10_000;

#[derive(Default)]
pub struct Stats(Mutex<Tracker>);

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn setup(app: &tauri::App) {
    let stats = Stats::default();
    let day = today();
    let saved = store::load_all(&app.state::<Db>().lock()).unwrap_or_default();
    // Claiming the day at zero matters: the first payload would otherwise roll over and drop
    // the deltas resumed here.
    if let Some(entry) = saved.iter().find(|e| e.date == day) {
        stats.0.lock().unwrap().restore(entry);
    }
    app.manage(stats);
    let handle = app.handle().clone();
    app.listen(crate::services::trackers::TRADE_EVENT, move |_| {
        record(&handle, |t| t.add_trade(&today()));
    });
}

fn record(app: &AppHandle, f: impl FnOnce(&mut Tracker) -> DailyStatEntry) {
    let entry = f(&mut app.state::<Stats>().0.lock().unwrap());
    if let Err(e) = store::upsert(&app.state::<Db>().lock(), &entry) {
        tracing::warn!("stats save failed: {e}");
    }
}

/// Called by the inventory sink before it announces the payload.
pub fn on_inventory(app: &AppHandle, data: Option<&Value>) {
    if let Some(data) = data {
        record(app, |t| t.on_inventory(data, &today()));
    }
}

/// The running day, including what a restart resumed.
#[tauri::command]
#[specta::specta]
pub fn stats_get_session(stats: State<Stats>) -> SessionStats {
    stats.0.lock().unwrap().session()
}

/// Every stored day, oldest first.
#[tauri::command]
#[specta::specta]
pub fn stats_get_daily(db: State<Db>) -> Result<Vec<DailyStatEntry>, String> {
    db.timed_read("stats_load_all", store::load_all)
        .map_err(|e| e.to_string())
}

/// One day as exported by WFHelper: the six pre-map resources are flat fields.
#[derive(Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StatsImportEntry {
    date: String,
    #[serde(default)]
    plat_delta: f64,
    #[serde(default)]
    credits_delta: f64,
    #[serde(default)]
    endo_delta: f64,
    #[serde(default)]
    ducats_delta: f64,
    #[serde(default)]
    aya_delta: f64,
    #[serde(default)]
    vitus_delta: f64,
    #[serde(default)]
    abs_plat: Option<f64>,
    #[serde(default)]
    abs_credits: Option<f64>,
    #[serde(default)]
    abs_endo: Option<f64>,
    #[serde(default)]
    abs_ducats: Option<f64>,
    #[serde(default)]
    abs_aya: Option<f64>,
    #[serde(default)]
    abs_vitus: Option<f64>,
    #[serde(default)]
    relics_opened: u32,
    #[serde(default = "one")]
    days_played: u32,
    #[serde(default)]
    daily_trades: u32,
    #[serde(default)]
    resources: HashMap<String, StatResourceDay>,
}

fn one() -> u32 {
    1
}

#[derive(Serialize, specta::Type)]
pub struct StatsImportResult {
    pub ok: bool,
    pub count: u32,
}

fn valid_date(s: &str) -> bool {
    s.len() == 10 && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()
}

/// None when any row is malformed, so a bad file imports nothing. Unknown resource ids are
/// dropped; the flat fields fill the six pre-map resources unless the map already has them.
fn sanitize(rows: Vec<StatsImportEntry>) -> Option<Vec<DailyStatEntry>> {
    if rows.len() > MAX_IMPORT_ROWS {
        return None;
    }
    rows.into_iter()
        .map(|r| {
            let flat = [
                ("plat", r.plat_delta, r.abs_plat),
                ("credits", r.credits_delta, r.abs_credits),
                ("endo", r.endo_delta, r.abs_endo),
                ("ducats", r.ducats_delta, r.abs_ducats),
                ("aya", r.aya_delta, r.abs_aya),
                ("vitus", r.vitus_delta, r.abs_vitus),
            ];
            let mut resources: HashMap<String, StatResourceDay> = r
                .resources
                .into_iter()
                .filter(|(id, _)| tracker::is_known(id))
                .collect();
            for (id, delta, abs) in flat {
                resources
                    .entry(id.to_owned())
                    .or_insert(StatResourceDay { delta, abs });
            }
            let sound = resources
                .values()
                .all(|d| d.delta.is_finite() && d.abs.is_none_or(|a| a.is_finite() && a >= 0.0));
            (valid_date(&r.date) && sound).then_some(DailyStatEntry {
                date: r.date,
                relics_opened: r.relics_opened,
                days_played: r.days_played,
                daily_trades: r.daily_trades,
                resources,
            })
        })
        .collect()
}

/// Merges a WFHelper stats history; today stays with the live tracker.
#[tauri::command]
#[specta::specta]
pub fn stats_import(db: State<Db>, entries: Vec<StatsImportEntry>) -> StatsImportResult {
    let Some(rows) = sanitize(entries) else {
        return StatsImportResult {
            ok: false,
            count: 0,
        };
    };
    match store::import(&mut db.lock(), &rows, &today()) {
        Ok(count) => StatsImportResult { ok: true, count },
        Err(_) => StatsImportResult {
            ok: false,
            count: 0,
        },
    }
}

/// Forgets every stored day and the running session; the next payload starts today from scratch.
#[tauri::command]
#[specta::specta]
pub fn stats_reset(db: State<Db>, stats: State<Stats>) -> Result<(), String> {
    store::clear(&db.lock()).map_err(|e| e.to_string())?;
    *stats.0.lock().unwrap() = Tracker::default();
    Ok(())
}
