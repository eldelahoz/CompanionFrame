//! Configurable global hotkeys. On Windows a low-level keyboard hook fires (and swallows) a
//! binding only while Warframe is in front; if the hook cannot be installed the bindings move
//! to `tauri-plugin-global-shortcut`, which fires everywhere and consumes the key system-wide.

pub mod accelerator;
pub mod matcher;
pub mod registry;

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::store::{Settings, Store};
use registry::Registry;

/// Payload of the `hotkey` event. `id` is an owned binding (`reward_scan`, `overlay_move`,
/// `riven_rescan`).
#[derive(Clone, Serialize, Deserialize)]
pub struct HotkeyFired {
    pub id: String,
    pub ts_ms: u64,
}

pub struct Hotkeys {
    app: AppHandle,
    inner: Mutex<Inner>,
}

struct Inner {
    registry: Registry,
    /// Windows only: without the keyboard hook everything goes through global-shortcut.
    #[cfg(windows)]
    fallback: bool,
}

fn owned(s: &Settings) -> [(&'static str, &str); 3] {
    [
        ("reward_scan", &s.reward_scan_hotkey),
        ("overlay_move", &s.overlay_move_hotkey),
        ("riven_rescan", &s.riven_rescan_hotkey),
    ]
}

fn emit(app: &AppHandle, id: String) {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let _ = app.emit("hotkey", HotkeyFired { id, ts_ms });
}

impl Hotkeys {
    /// Validates every binding first; on error nothing changes (so a bad setting is never saved).
    pub fn apply(&self, s: &Settings) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        inner.registry.set_owned(&owned(s))?;
        self.sync(&inner);
        Ok(())
    }

    #[cfg(windows)]
    fn fall_back(&self, reason: &str) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.fallback {
            tracing::warn!(
                reason,
                "keyboard hook unavailable, falling back to global-shortcut"
            );
            inner.fallback = true;
            self.sync(&inner);
        }
    }

    fn sync(&self, inner: &Inner) {
        let watch = inner.registry.watch();
        #[cfg(windows)]
        if !inner.fallback {
            crate::platform::windows::hotkey::set_watch(watch);
            return;
        }
        let gs = self.app.global_shortcut();
        let _ = gs.unregister_all();
        for (id, accel) in watch {
            let app = self.app.clone();
            let bound = gs.on_shortcut(accel.to_string().as_str(), {
                let id = id.clone();
                move |_, _, ev| {
                    if ev.state() == ShortcutState::Pressed {
                        emit(&app, id.clone());
                    }
                }
            });
            if let Err(e) = bound {
                tracing::warn!(%id, %accel, error = %e, "global-shortcut register failed");
            }
        }
    }
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let hotkeys = Hotkeys {
        app: app.handle().clone(),
        inner: Mutex::new(Inner {
            registry: Registry::default(),
            #[cfg(windows)]
            fallback: false,
        }),
    };
    let wanted = app.state::<Store>().get();
    if let Err(e) = hotkeys.apply(&wanted) {
        tracing::warn!(error = %e, "stored hotkeys rejected, using defaults");
        let _ = hotkeys.apply(&Settings::default());
    }
    app.manage(hotkeys);

    #[cfg(windows)]
    {
        use crate::platform::windows::hotkey::{self, HookEvent};
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = app.handle().clone();
        // Plain threads: `setup` has no tokio runtime context, and the hook thread must not block.
        std::thread::spawn(move || {
            for ev in rx {
                match ev {
                    HookEvent::Installed => tracing::info!("low-level keyboard hook installed"),
                    HookEvent::Failed(why) => handle.state::<Hotkeys>().fall_back(&why),
                    HookEvent::Fired(id) => emit(&handle, id),
                }
            }
        });
        hotkey::start(tx);
    }
    Ok(())
}

/// Waits up to 10 s for the next key combo (in any app) and returns it as an accelerator string,
/// for a "press the new shortcut" field. Esc cancels. Needs the keyboard hook.
#[tauri::command]
#[specta::specta]
pub async fn hotkey_capture() -> Result<String, String> {
    #[cfg(windows)]
    {
        use crate::platform::windows::hotkey;
        use std::time::Duration;
        let (tx, rx) = tokio::sync::oneshot::channel();
        hotkey::arm_capture(tx);
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(Some(accel))) => Ok(accel.to_string()),
            Ok(Ok(None)) => Err("cancelled".into()),
            _ => {
                hotkey::cancel_capture();
                Err("timed out".into())
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("hotkey capture needs the Windows keyboard hook".into())
    }
}
