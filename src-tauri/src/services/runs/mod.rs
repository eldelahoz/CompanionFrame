//! EE.log runs persisted in SQLite (migrations 10-11). `store` is generic over the run kind;
//! `arbi` owns the arbitration tracker, importer and records.
pub mod arbi;
pub mod pt;
pub mod segment;
pub mod store;

use std::path::PathBuf;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;
use crate::services::trackers::LineTracker;
use arbi::{ArbiImportResult, ArbiRunRecord, ArbiRunsPayload, ArbiTracker};
use pt::{PtImportResult, PtRunRecord, PtRunsPayload, PtTracker};
use segment::{MAX_SEGMENT_BYTES, RunSource};

/// The live arbitration tracker for the hub; saved runs are announced as `arbi-run-saved`.
pub fn arbi_tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    let app = app.clone();
    Box::new(ArbiTracker::new(Box::new(move |finished| {
        let db = app.state::<Db>();
        let saved = arbi::save(&mut db.0.lock().unwrap(), finished, RunSource::Live);
        match saved {
            Ok(Some(run)) => {
                let _ = app.emit("arbi-run-saved", run);
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("[Arbi] saving a run failed: {e}"),
        }
    })))
}

#[tauri::command]
#[specta::specta]
pub fn arbi_runs_get(db: State<Db>) -> Result<ArbiRunsPayload, String> {
    arbi::payload(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn arbi_run_get(db: State<Db>, id: String) -> Option<ArbiRunRecord> {
    arbi::get(&db.0.lock().unwrap(), &id)
}

/// The stored record; None for an unknown id or a value outside 0..=10M.
#[tauri::command]
#[specta::specta]
pub fn arbi_run_set_vitus(db: State<Db>, id: String, vitus: Option<f64>) -> Option<ArbiRunRecord> {
    arbi::set_vitus(&db.0.lock().unwrap(), &id, vitus)
}

/// The normalized tags as stored; None for an unknown id.
#[tauri::command]
#[specta::specta]
pub fn run_set_tags(db: State<Db>, id: String, tags: Vec<String>) -> Option<Vec<String>> {
    store::set_tags(&db.0.lock().unwrap(), &id, &tags)
        .ok()
        .flatten()
}

/// The normalized note as stored ("" when cleared); None for an unknown id.
#[tauri::command]
#[specta::specta]
pub fn run_set_notes(db: State<Db>, id: String, notes: String) -> Option<String> {
    store::set_notes(&db.0.lock().unwrap(), &id, &notes)
        .ok()
        .flatten()
}

/// Drops the raw capture and keeps the record.
#[tauri::command]
#[specta::specta]
pub fn run_delete_log(db: State<Db>, id: String) -> bool {
    store::delete_log(&db.0.lock().unwrap(), &id).unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub fn run_delete(db: State<Db>, id: String) -> bool {
    let mut conn = db.0.lock().unwrap();
    let deleted = store::delete(&mut conn, &id).unwrap_or(false);
    if deleted {
        // Deleting the richer record must release the copy that pointed at it.
        let _ = arbi::refresh_duplicates(&conn);
        let _ = pt::refresh_duplicates(&conn);
    }
    deleted
}

#[tauri::command]
#[specta::specta]
pub async fn arbi_import_log(app: AppHandle, path: String) -> Result<ArbiImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        arbi::import_log(&PathBuf::from(path), &db.0, MAX_SEGMENT_BYTES)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The live Profit Taker tracker for the hub; saved runs are announced as `pt-run-saved`.
pub fn pt_tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    let app = app.clone();
    Box::new(PtTracker::new(Box::new(move |finished| {
        let db = app.state::<Db>();
        let saved = pt::save(&mut db.0.lock().unwrap(), finished, RunSource::Live);
        match saved {
            Ok(Some(run)) => {
                let _ = app.emit("pt-run-saved", run);
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("[PT] saving a run failed: {e}"),
        }
    })))
}

#[tauri::command]
#[specta::specta]
pub fn pt_runs_get(db: State<Db>) -> Result<PtRunsPayload, String> {
    pt::payload(&db.0.lock().unwrap()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn pt_run_get(db: State<Db>, id: String) -> Option<PtRunRecord> {
    pt::get(&db.0.lock().unwrap(), &id)
}

#[tauri::command]
#[specta::specta]
pub async fn pt_import_log(app: AppHandle, path: String) -> Result<PtImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Db>();
        pt::import_log(&PathBuf::from(path), &db.0, MAX_SEGMENT_BYTES)
    })
    .await
    .map_err(|e| e.to_string())?
}
