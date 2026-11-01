//! Tauri glue for the inventory core: events, file dialogs, the snapshot cache and the watcher thread.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::{ExportError, Inventory, InventoryStatus, Sink, delta};
use crate::db::Db;
use crate::services::atomic::write_atomic;
use crate::services::item_db::ItemDbState;

const POLL_INTERVAL: Duration = Duration::from_millis(500);

struct EventSink(AppHandle);

impl Sink for EventSink {
    /// The payload is the inventory JSON text (or null): the UI parses it itself, which avoids
    /// teaching the bindings about arbitrary JSON.
    fn updated(&self, data: Option<&Value>) {
        crate::services::stats::on_inventory(&self.0, data);
        let _ = self.0.emit("inventory-updated", data.map(Value::to_string));
    }

    fn status(&self, status: &InventoryStatus) {
        let _ = self.0.emit("inventory-status-updated", status);
    }
}

pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let inv = Arc::new(Inventory::new(Arc::new(EventSink(app.handle().clone()))));
    app.manage(inv.clone());
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        let db = handle.state::<Db>();
        inv.load(&db);
        loop {
            std::thread::sleep(POLL_INTERVAL);
            inv.poll(&db);
        }
    });
    Ok(())
}

fn text(d: Option<Arc<Value>>) -> Option<String> {
    d.map(|d| d.to_string())
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_inventory(inv: State<Arc<Inventory>>) -> Option<String> {
    text(inv.get())
}

#[tauri::command(async)]
#[specta::specta]
pub fn get_inventory_status(inv: State<Arc<Inventory>>) -> InventoryStatus {
    inv.status()
}

/// Imports a dropped or typed path: `.dat` as an AlecaFrame cache, anything else as inventory JSON.
#[tauri::command(async)]
#[specta::specta]
pub fn import_inventory_path(
    path: String,
    inv: State<Arc<Inventory>>,
    db: State<Db>,
) -> Option<String> {
    text(inv.import_path(&db, Path::new(&path)))
}

/// Picks an inventory JSON (the `inventory.json` shape) to import.
#[tauri::command(async)]
#[specta::specta]
pub fn open_inventory_file(
    app: AppHandle,
    inv: State<Arc<Inventory>>,
    db: State<Db>,
) -> Option<String> {
    let path = app
        .dialog()
        .file()
        .set_title("Select an inventory JSON")
        .add_filter("JSON Files", &["json"])
        .blocking_pick_file()?
        .into_path()
        .ok()?;
    text(inv.import_path(&db, &path))
}

#[tauri::command(async)]
#[specta::specta]
pub fn open_alecaframe_inventory_file(
    app: AppHandle,
    inv: State<Arc<Inventory>>,
    db: State<Db>,
) -> Option<String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Select AlecaFrame lastData.dat")
        .add_filter("AlecaFrame inventory", &["dat"]);
    if let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| Path::new(&d).join("AlecaFrame")) {
        dialog = dialog.set_directory(dir);
    }
    let path = dialog.blocking_pick_file()?.into_path().ok()?;
    text(inv.import_path(&db, &path))
}

/// AlecaFrame's `lastData.dat` in its default folder, when it is there, so Setup can offer it without a picker.
#[tauri::command(async)]
#[specta::specta]
pub fn find_alecaframe_cache() -> Option<String> {
    let path = Path::new(&std::env::var_os("LOCALAPPDATA")?)
        .join("AlecaFrame")
        .join("lastData.dat");
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

/// Forgets the imported snapshot and the deltas applied to it.
#[tauri::command(async)]
#[specta::specta]
pub fn clear_inventory(inv: State<Arc<Inventory>>, db: State<Db>) {
    inv.clear(&db);
}

/// The reward overlay's "I took this" for an OCR'd relic reward (English card name). It is an
/// estimate: the game's own count is only exact after the next import.
#[tauri::command(async)]
#[specta::specta]
pub fn inventory_apply_pick(
    name: String,
    inv: State<Arc<Inventory>>,
    db: State<Db>,
    items: State<ItemDbState>,
) -> bool {
    let (item_db, _) = items.snapshot();
    let Some((unique, _)) = item_db.lookup_by_name_or_slug(Some(&name), None) else {
        return false;
    };
    inv.add_deltas(
        &db,
        vec![delta::from_pick(
            unique,
            &name,
            crate::services::clock::now_ms_f64(),
        )],
    )
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    saved: bool,
    path: Option<String>,
    error: Option<ExportError>,
}

#[tauri::command(async)]
#[specta::specta]
pub fn export_inventory(app: AppHandle, inv: State<Arc<Inventory>>) -> ExportResult {
    let fail = |error| ExportResult {
        saved: false,
        path: None,
        error: Some(error),
    };
    let body = match inv.export_text() {
        Ok(b) => b,
        Err(e) => return fail(e),
    };
    let stamp = chrono::Local::now().format("%Y-%m-%d");
    let Some(path) = app
        .dialog()
        .file()
        .set_file_name(format!("companionframe-inventory-{stamp}.json"))
        .add_filter("JSON", &["json"])
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return ExportResult {
            saved: false,
            path: None,
            error: None,
        };
    };
    match write_atomic(&path, body.as_bytes()) {
        Ok(()) => ExportResult {
            saved: true,
            path: Some(path.display().to_string()),
            error: None,
        },
        Err(e) => {
            tracing::warn!("inventory export write failed: {e}");
            fail(ExportError::WriteFailed)
        }
    }
}
