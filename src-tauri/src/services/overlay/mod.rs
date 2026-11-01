//! Overlay windows (reward, riven, arbi, trade): created once, hidden, reused, shown without
//! activation over the game's monitor. See [`manager`] for the decision logic.

pub mod arbi;
#[cfg(windows)]
mod host;
pub mod layout;
pub mod manager;
pub mod placement;
pub mod reward;
pub mod riven;
pub mod riven_analyzer;
pub mod settings;
#[cfg(test)]
mod tests;
pub mod trade;

use tauri::AppHandle;

pub use manager::{OverlayKind, OverlayRect};

#[cfg(windows)]
type Overlays = std::sync::Mutex<manager::Manager<host::TauriHost>>;

#[cfg(windows)]
fn with<R>(
    app: &AppHandle,
    f: impl FnOnce(&mut manager::Manager<host::TauriHost>) -> R,
) -> Result<R, String> {
    use tauri::Manager as _;

    let state = app
        .try_state::<Overlays>()
        .ok_or("overlays are not ready")?;
    let mut m = state.lock().map_err(|_| "overlay state poisoned")?;
    Ok(f(&mut m))
}

#[cfg(not(windows))]
fn with<R>(_: &AppHandle, _: impl FnOnce(&mut manager::Manager<NoHost>) -> R) -> Result<R, String> {
    Err("windows only".into())
}

/// Placeholder so the commands type-check off Windows (F7 brings the real hosts).
#[cfg(not(windows))]
struct NoHost;

#[cfg(not(windows))]
impl manager::Host for NoHost {
    fn show(&mut self, _: OverlayKind) {}
    fn hide(&mut self, _: OverlayKind) {}
    fn set_click_through(&mut self, _: OverlayKind, _: bool) {}
    fn place(&mut self, _: OverlayKind, _: OverlayRect) {}
    fn is_topmost(&self, _: OverlayKind) -> bool {
        false
    }
    fn raise(&mut self, _: OverlayKind) {}
    fn set_content_visible(&mut self, _: OverlayKind, _: bool) {}
    fn game(&mut self) -> Option<manager::Game> {
        None
    }
    fn primary_monitor(&self) -> Option<OverlayRect> {
        None
    }
    fn cursor(&self) -> Option<(i32, i32)> {
        None
    }
    fn restore_game_focus(&mut self) {}
    fn keep_mapped(&self) -> bool {
        false
    }
}

// Commands are async so they never run on the main thread, which the host's window calls wait on.

/// Shows the overlay without taking focus from the game.
#[tauri::command]
#[specta::specta]
pub async fn overlay_show(app: AppHandle, kind: OverlayKind) -> Result<(), String> {
    with(&app, |m| m.show(kind))
}

#[tauri::command]
#[specta::specta]
pub async fn overlay_hide(app: AppHandle, kind: OverlayKind) -> Result<(), String> {
    with(&app, |m| m.hide(kind))
}

/// Makes the whole window take clicks (still without focus) or click-through again.
#[tauri::command]
#[specta::specta]
pub async fn overlay_set_interactive(
    app: AppHandle,
    kind: OverlayKind,
    interactive: bool,
) -> Result<(), String> {
    with(&app, |m| m.set_interactive(kind, interactive))
}

/// Clickable regions in physical screen pixels (CSS px x devicePixelRatio + window origin);
/// everything outside them stays click-through.
#[tauri::command]
#[specta::specta]
pub async fn overlay_set_regions(
    app: AppHandle,
    kind: OverlayKind,
    regions: Vec<OverlayRect>,
) -> Result<(), String> {
    with(&app, |m| m.set_regions(kind, regions))
}

/// Event to every overlay page: `true` while overlays are being moved.
pub const MOVE_EVENT: &str = "overlay-move-mode";

/// Move mode on: every enabled overlay shows sample content and can be dragged. Off: the pages
/// already saved their positions; the overlays lock and the sample-only ones hide.
pub fn set_move_mode(app: &AppHandle, on: bool) -> Result<(), String> {
    use tauri::Emitter;
    with(app, |m| if on { m.start_move() } else { m.end_move() })?;
    tracing::info!("overlay move mode {}", if on { "on" } else { "off" });
    app.emit(MOVE_EVENT, on).map_err(|e| e.to_string())
}

/// The Settings button and the on-screen Done button.
#[tauri::command]
#[specta::specta]
pub async fn overlay_move_mode(app: AppHandle, on: bool) -> Result<(), String> {
    set_move_mode(&app, on)
}

#[cfg(windows)]
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    use std::sync::Mutex;
    use std::time::Duration;

    use tauri::{Listener, Manager as _};

    use crate::services::hotkeys::HotkeyFired;

    reward::setup(app);
    let handle = app.handle().clone();
    // Building four webviews is slow, so it stays out of setup().
    tauri::async_runtime::spawn(async move {
        let host = match host::TauriHost::build(&handle) {
            Ok(h) => h,
            Err(e) => return tracing::error!(error = %e, "overlay windows failed"),
        };
        handle.manage::<Overlays>(Mutex::new(manager::Manager::new(host)));

        let hk = handle.clone();
        // The reward scan key scans or closes the reward card; the move key toggles move mode.
        handle.listen("hotkey", move |ev| {
            match serde_json::from_str::<HotkeyFired>(ev.payload())
                .as_ref()
                .map(|h| h.id.as_str())
            {
                Ok("reward_scan") => reward::on_hotkey(&hk),
                Ok("overlay_move") => {
                    let on = !with(&hk, |m| m.is_moving()).unwrap_or(true);
                    let _ = set_move_mode(&hk, on);
                }
                _ => {}
            }
        });

        let rk = handle.clone();
        handle.listen("hotkey", move |ev| {
            if serde_json::from_str::<HotkeyFired>(ev.payload())
                .is_ok_and(|h| h.id == "riven_rescan")
            {
                riven::on_hotkey(&rk);
            }
        });

        // 250 ms foreground watch; the cursor is sampled every 50 ms only while regions exist.
        let mut n = 0u32;
        loop {
            let fast = with(&handle, |m| {
                if n.is_multiple_of(5) {
                    m.tick();
                }
                m.poll_cursor();
                m.needs_fast_poll()
            })
            .unwrap_or(false);
            n = n.wrapping_add(1);
            tokio::time::sleep(Duration::from_millis(if fast { 50 } else { 250 })).await;
            if !fast {
                n = 0;
            }
        }
    });
    Ok(())
}
