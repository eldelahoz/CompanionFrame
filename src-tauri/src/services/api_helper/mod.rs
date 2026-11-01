//! Inventory acquisition: the pinned external `warframe-api-helper` (download, run) and the native
//! game-memory fallback, polled in the background. Port of WFHelper's `apiHelperRunner.ts`.
//! Memory mode only (`memory_gate`). The inventory file lands in `<app data>/api-helper/inventory.json`
//! and each successful fetch is imported as the inventory snapshot (source `helper`).
//!
//! Authz (the game's own `?accountId=..&nonce=..`) lives in memory only: it is never logged,
//! persisted or put in an error string, and goes only to DE's own HTTPS endpoint (`authz`).
//!
//! shortcut: the helper is looked for in the app's helper dir only (WFHelper also scanned
//! Downloads/Desktop/Documents/home); the hash pin holds either way, so widen it if users ask.

pub mod acquire;
pub mod authz;
pub mod download;
pub mod runner;
pub mod zip;

#[cfg(test)]
mod tests;

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::db::Db;
use crate::services::eelog::{EeLog, EeLogEvent};
use crate::services::eelog_parsers::lines::is_login_complete_line;
use crate::services::inventory::Inventory;
use crate::services::inventory_sync::InventorySync;
use crate::services::memory_gate;
use crate::store::Store;
use download::{DownloadStage, HelperDownloadProgress};
use runner::Runner;

pub const HELPER_DIR: &str = "api-helper";
pub const PREFERENCES_FILE: &str = "setup-preferences.json";

/// Why the last run did not produce an inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum HelperRunReason {
    GameNotRunning,
    AccessDenied,
    NotLoggedIn,
    TokenNotFound,
    ApiFailed,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HelperStatus {
    pub exe_found: bool,
    pub running: bool,
    /// Epoch milliseconds.
    pub last_run_at: Option<f64>,
    pub last_run_ok: Option<bool>,
    pub last_run_reason: Option<HelperRunReason>,
    /// Epoch milliseconds of `inventory.json`.
    pub inventory_last_modified: Option<f64>,
    /// What the installer asked for, when it wrote a preference.
    pub installer_auto_install_helper: Option<bool>,
}

/// Sainan's helper: 1 = game not found, 2 = OpenProcess denied (game runs as admin),
/// 3 = auth string not in memory (login screen).
pub fn helper_failure_reason(exit_code: Option<i32>) -> HelperRunReason {
    match exit_code {
        Some(1) => HelperRunReason::GameNotRunning,
        Some(2) => HelperRunReason::AccessDenied,
        Some(3) => HelperRunReason::NotLoggedIn,
        _ => HelperRunReason::Error,
    }
}

/// Failures that never reached DE's API did not use the cooldown, so they may retry much sooner.
const LOCAL_FAILURE_RETRY_MS: u64 = 90_000;

/// Fast retry only for failures that resolve on their own within minutes; token-not-found is
/// persistent and a rescan hits gigabytes of game memory.
pub fn next_poll_delay_ms(reason: Option<HelperRunReason>, interval_ms: u64) -> u64 {
    use HelperRunReason::*;
    match reason {
        Some(GameNotRunning | AccessDenied | NotLoggedIn) => {
            LOCAL_FAILURE_RETRY_MS.min(interval_ms)
        }
        _ => interval_ms,
    }
}

pub fn should_retry_after_game_login(
    running: bool,
    last_ok: Option<bool>,
    reason: Option<HelperRunReason>,
) -> bool {
    use HelperRunReason::*;
    if running {
        return true;
    }
    if last_ok == Some(true) {
        return false;
    }
    matches!(
        reason,
        None | Some(GameNotRunning | NotLoggedIn | TokenNotFound | Error)
    )
}

pub fn should_use_native_fallback(helper_found: bool, reason: Option<HelperRunReason>) -> bool {
    !helper_found
        || matches!(
            reason,
            Some(HelperRunReason::TokenNotFound | HelperRunReason::Error)
        )
}

/// `autoInstallHelper` from the installer's preferences file; unreadable or absent means "no opinion".
pub fn read_auto_install_preference(path: &Path) -> Option<bool> {
    let raw = std::fs::read(path).ok()?;
    serde_json::from_slice::<serde_json::Value>(&raw)
        .ok()?
        .get("autoInstallHelper")?
        .as_bool()
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub ok: bool,
    pub error: Option<String>,
}

pub struct Helper {
    pub runner: Arc<Runner>,
    acquire: Arc<acquire::Acquire>,
    preferences: std::path::PathBuf,
}

#[tauri::command(async)]
#[specta::specta]
pub fn helper_get_status(helper: State<Helper>) -> HelperStatus {
    let mut s = helper.runner.status();
    s.installer_auto_install_helper = read_auto_install_preference(&helper.preferences);
    s
}

/// The manual "Run helper now" button: runs even when automatic sync is off.
#[tauri::command]
#[specta::specta]
pub async fn helper_run_now(helper: State<'_, Helper>) -> Result<bool, String> {
    memory_gate::require()?;
    Ok(helper.runner.run_once().await)
}

/// Downloads and verifies the pinned helper, emitting `helper-download-progress`.
#[tauri::command]
#[specta::specta]
pub async fn helper_download(
    app: AppHandle,
    helper: State<'_, Helper>,
    sync: State<'_, Arc<InventorySync>>,
) -> Result<DownloadResult, String> {
    memory_gate::require()?;
    let emit = |p: HelperDownloadProgress| {
        let _ = app.emit("helper-download-progress", p);
    };
    let res = helper.acquire.download(&emit).await;
    match &res {
        Ok(()) => sync.apply("helper download"),
        Err(e) => {
            tracing::error!(error = %e, "helper download failed");
            emit(HelperDownloadProgress::stage(DownloadStage::Error).error(e.clone()));
        }
    }
    Ok(DownloadResult {
        ok: res.is_ok(),
        error: res.err(),
    })
}

/// Builds the helper state, starts acquisition per the mode/setting, and follows EE.log logins.
pub fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data = app.path().app_data_dir()?;
    let handle = app.handle().clone();
    let acquire = Arc::new(acquire::Acquire::new(
        data.join(HELPER_DIR),
        Box::new({
            let h = handle.clone();
            move || h.state::<EeLog>().status().path.map(Into::into)
        }),
        Box::new({
            let h = handle.clone();
            move |p| {
                h.state::<Arc<Inventory>>()
                    .import_helper(&h.state::<Db>(), p)
            }
        }),
    )?);
    let runner = Runner::new(
        acquire.clone(),
        tauri::async_runtime::handle().inner().clone(),
    );
    app.manage(Helper {
        runner: runner.clone(),
        acquire,
        preferences: data.join(PREFERENCES_FILE),
    });

    let sync = Arc::new(InventorySync::new(
        runner,
        Box::new(memory_gate::allowed),
        Box::new({
            let h = handle.clone();
            move || h.state::<Store>().get().auto_inventory_sync
        }),
    ));
    app.manage(sync.clone());
    // Hashes the helper exe (disk, often AV-scanned): keep it off the setup path.
    let starter = sync.clone();
    tauri::async_runtime::spawn_blocking(move || starter.apply("startup"));

    let mut lines = handle.state::<EeLog>().subscribe();
    tauri::async_runtime::spawn(async move {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            match lines.recv().await {
                Ok(EeLogEvent::Line { text, .. }) if is_login_complete_line(&text) => {
                    sync.on_game_login()
                }
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => break,
            }
        }
    });
    Ok(())
}
