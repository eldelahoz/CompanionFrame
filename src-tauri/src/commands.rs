use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::services::hotkeys::Hotkeys;
use crate::services::inventory_sync::InventorySync;
use crate::services::overlay;
use crate::store::{Settings, Store};

#[derive(Serialize, specta::Type)]
pub struct AppInfo {
    name: String,
    version: String,
    data_dir: String,
    log_dir: String,
}

#[tauri::command]
#[specta::specta]
pub fn app_info(app: AppHandle) -> Result<AppInfo, String> {
    let p = app.path();
    let dir = |r: tauri::Result<std::path::PathBuf>| {
        r.map(|d| d.display().to_string())
            .map_err(|e| e.to_string())
    };
    Ok(AppInfo {
        name: app.package_info().name.clone(),
        version: app.package_info().version.to_string(),
        data_dir: dir(p.app_data_dir())?,
        log_dir: dir(p.app_log_dir())?,
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(store: State<Store>) -> Settings {
    store.get()
}

#[tauri::command]
#[specta::specta]
pub fn set_settings(
    store: State<Store>,
    sync: State<Arc<InventorySync>>,
    hotkeys: State<Hotkeys>,
    app: AppHandle,
    mut settings: Settings,
) -> Result<(), String> {
    // Layouts are written only by the overlay editor; a stale settings form must not undo them.
    settings.overlay_layouts = store.get().overlay_layouts;
    overlay::settings::validate(&settings)?;
    if !crate::services::game_locale::is_setting(&settings.game_language) {
        return Err("unknown game language".into());
    }
    crate::services::memory_gate::validate(&settings)?;
    hotkeys.apply(&settings)?;
    store.set(settings.clone()).map_err(|e| e.to_string())?;
    if crate::services::item_db::apply_ui_language(&app, &settings.language)? {
        let _ = app.emit("game-locale-updated", ());
    }
    crate::services::memory_gate::apply(&settings);
    crate::services::scan_captures::apply(&settings);
    let _ = app.emit(overlay::settings::CHANGED_EVENT, ());
    // Starts or stops the helper right away when the data source changed.
    sync.apply("settings");
    Ok(())
}
