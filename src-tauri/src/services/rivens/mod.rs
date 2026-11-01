//! Riven services: weapon/stat data, grading and the per-weapon best attributes (port of
//! WFHelper's `rivenData`, `rivenGrading`, `rivenBestAttributes`; OCR text matching is Phase 6).

pub mod best_attributes;
pub mod data;
pub mod endo;
pub mod grading;
pub mod memory;
pub mod sessions;
#[cfg(test)]
mod tests;
pub mod vocab;

use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;
use crate::services::bundled_data::BundledData;
use crate::services::net;
use best_attributes::{GoodRolls, RivenBestAttributes};
use data::RivenData;
use sessions::RivenRollSession;

use crate::services::inventory::Inventory;
use crate::services::riven_scan::fingerprint::{DecodedRivens, decode_all_rivens};
use crate::services::riven_scan::locale::CardLabels;

const REFETCH_COOLDOWN: Duration = Duration::from_secs(10 * 60);

#[derive(Default)]
pub struct RivensState {
    data: OnceLock<Arc<RivenData>>,
    rolls: RwLock<Arc<GoodRolls>>,
    /// Card labels of the game language the last scan used.
    labels: Mutex<Option<Arc<CardLabels>>>,
    last_fetch: Mutex<Option<Instant>>,
}

pub fn setup(app: &tauri::App) {
    app.manage(RivensState::default());
}

impl RivensState {
    fn rolls(&self) -> Arc<GoodRolls> {
        self.rolls.read().unwrap().clone()
    }

    /// True when a fetch may start now (and records it): one attempt per cooldown.
    fn may_fetch(&self) -> bool {
        let mut last = self.last_fetch.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < REFETCH_COOLDOWN) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }
}

async fn fetch_rolls(app: &AppHandle) -> Result<Arc<GoodRolls>, String> {
    let db = app.state::<Db>();
    let client = net::client().map_err(|e| e.to_string())?;
    let fresh = Arc::new(best_attributes::refresh(&db, &client, best_attributes::SHEET_URL).await?);
    *app.state::<RivensState>().rolls.write().unwrap() = fresh.clone();
    Ok(fresh)
}

fn log_fetch(res: Result<Arc<GoodRolls>, String>) {
    if let Err(e) = res {
        tracing::warn!("riven good-rolls refresh failed: {e}");
    }
}

/// Weapon data (built once) and the good-roll sheet. A missing sheet is awaited; a stale one is
/// served while it refreshes in the background.
pub(crate) async fn snapshot(app: &AppHandle) -> Result<(Arc<RivenData>, Arc<GoodRolls>), String> {
    let st = app.state::<RivensState>();
    let data = match st.data.get() {
        Some(d) => d.clone(),
        None => {
            let bundled = BundledData::from_app(app)?;
            let built = tauri::async_runtime::spawn_blocking(move || RivenData::load(&bundled))
                .await
                .map_err(|e| e.to_string())?;
            st.data.get_or_init(|| Arc::new(built)).clone()
        }
    };
    if st.rolls().is_empty()
        && let Some(cached) = GoodRolls::load_cached(&app.state::<Db>())
    {
        *st.rolls.write().unwrap() = Arc::new(cached);
    }
    if !st.rolls().is_current() && st.may_fetch() {
        if st.rolls().is_empty() {
            log_fetch(fetch_rolls(app).await);
        } else {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { log_fetch(fetch_rolls(&app).await) });
        }
    }
    Ok((data, st.rolls()))
}

/// Stat labels and weapon names of the current game language, rebuilt when it changes.
pub(crate) async fn card_labels(app: &AppHandle) -> Result<Arc<CardLabels>, String> {
    let code = app
        .state::<crate::services::item_db::ItemDbState>()
        .locale_code();
    let st = app.state::<RivensState>();
    if let Some(l) = st
        .labels
        .lock()
        .unwrap()
        .clone()
        .filter(|l| l.code() == code)
    {
        return Ok(l);
    }
    let bundled = BundledData::from_app(app)?;
    let built = tauri::async_runtime::spawn_blocking(move || CardLabels::load(&bundled, code))
        .await
        .map_err(|e| e.to_string())?;
    let built = Arc::new(built);
    *st.labels.lock().unwrap() = Some(built.clone());
    Ok(built)
}

/// The sheet's good stats and tolerated curses for a weapon; `None` when it rates none.
#[tauri::command]
#[specta::specta]
pub async fn riven_best_attributes(
    app: AppHandle,
    weapon_name: String,
    melee: bool,
) -> Result<Option<RivenBestAttributes>, String> {
    let (data, rolls) = snapshot(&app).await?;
    Ok(rolls.best_attributes(&data, &weapon_name, melee))
}

/// Age and size of the good-roll sheet in use.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenGoodRollsInfo {
    /// RFC 3339 time of the last successful download; `None` when no sheet is loaded.
    pub updated_at: Option<String>,
    pub weapons: u32,
}

impl RivenGoodRollsInfo {
    fn of(rolls: &GoodRolls) -> Self {
        Self {
            updated_at: rolls.updated_at().map(str::to_owned),
            weapons: rolls.len() as u32,
        }
    }
}

/// Which good-roll sheet is in use (for the "updated N days ago" label).
#[tauri::command]
#[specta::specta]
pub async fn rivens_good_rolls_info(app: AppHandle) -> Result<RivenGoodRollsInfo, String> {
    let (_, rolls) = snapshot(&app).await?;
    Ok(RivenGoodRollsInfo::of(&rolls))
}

/// Re-downloads the sheet now, ignoring the background cooldown. A failed download keeps (and
/// reports) the cached sheet, so an unchanged `updated_at` tells the UI nothing was refreshed.
#[tauri::command]
#[specta::specta]
pub async fn rivens_refresh_good_rolls(app: AppHandle) -> Result<RivenGoodRollsInfo, String> {
    snapshot(&app).await?;
    let rolls = fetch_rolls(&app).await?;
    Ok(RivenGoodRollsInfo::of(&rolls))
}

/// The live tracker for the hub; finished roll sessions are announced as `riven-session-saved`.
pub fn tracker(app: &AppHandle) -> Box<dyn crate::services::trackers::LineTracker> {
    let (app, overlay) = (app.clone(), app.clone());
    Box::new(crate::services::trackers::RivenTracker::new(
        Box::new(move |session| {
            let db = app.state::<Db>();
            let saved = sessions::save(&db.0.lock().unwrap(), &session);
            match saved {
                Ok(()) => {
                    let _ = app.emit("riven-session-saved", session);
                }
                Err(e) => tracing::warn!("[Riven] saving a session failed: {e}"),
            }
        }),
        Box::new(move |e| crate::services::overlay::riven::on_event(&overlay, e)),
    ))
}

/// Stored roll sessions, pinned first then newest first.
#[tauri::command]
#[specta::specta]
pub fn rivens_get(db: State<Db>) -> Result<Vec<RivenRollSession>, String> {
    sessions::list(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

/// The note as stored ("" when cleared); None for an unknown id.
#[tauri::command]
#[specta::specta]
pub fn rivens_set_note(db: State<Db>, id: String, note: String) -> Option<String> {
    sessions::set_note(&db.0.lock().unwrap(), &id, &note)
        .ok()
        .flatten()
        .map(Option::unwrap_or_default)
}

#[tauri::command]
#[specta::specta]
pub fn rivens_pin(db: State<Db>, id: String, pinned: bool) -> bool {
    sessions::set_pinned(&db.0.lock().unwrap(), &id, pinned).unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub fn rivens_delete(db: State<Db>, id: String) -> bool {
    sessions::delete(&db.0.lock().unwrap(), &id).unwrap_or(false)
}

/// The user's inventory rivens decoded from their fingerprints (stats, rolls, grades); empty when no inventory is loaded.
#[tauri::command]
#[specta::specta]
pub async fn rivens_decoded(app: AppHandle) -> Result<DecodedRivens, String> {
    let Some(inventory) = app.state::<Arc<Inventory>>().get() else {
        return Ok(DecodedRivens::default());
    };
    let (data, rolls) = snapshot(&app).await?;
    Ok(decode_all_rivens(&inventory, &data, &rolls))
}
