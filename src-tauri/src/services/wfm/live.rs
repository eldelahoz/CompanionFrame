//! Wires the socket, presence and the session together and exposes them to the frontend.
//!
//! Events: `wfm-ws-status` (`WsStatus`), `wfm-ws-event` (`{route, payload}`), `wfm-presence`
//! (`PresenceState`). One task polls the game and drives presence; it ends with the app.

use crate::services::clock::now_ms;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tauri::{Emitter, Manager, State};

use super::errors::WfmError;
use super::presence::{Presence, PresenceOptions, PresenceState, Pusher, Status};
use super::session::Session;
use super::ws::{Listener, Out, Timing, URL, WsStatus};

const GAME_POLL: Duration = Duration::from_secs(5);

struct SocketPusher {
    listener: Arc<Listener>,
    ready: Arc<AtomicBool>,
}

impl Pusher for SocketPusher {
    fn ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
    async fn push(&self, status: Status, duration: Option<u64>) -> Result<Option<String>, String> {
        self.listener.set_status(status.as_str(), duration).await
    }
}

pub struct Live {
    presence: Arc<Presence<SocketPusher>>,
    ws_status: Mutex<WsStatus>,
}

#[cfg(windows)]
fn game_running() -> bool {
    crate::platform::windows::process::warframe_running()
}

#[cfg(not(windows))]
fn game_running() -> bool {
    false
}

#[cfg(windows)]
fn idle_seconds() -> Option<f64> {
    crate::platform::windows::process::idle_seconds()
}

#[cfg(not(windows))]
fn idle_seconds() -> Option<f64> {
    None
}

pub fn setup(app: &tauri::App) {
    let handle = app.handle().clone();
    let session = app.state::<Session>();
    let ready = Arc::new(AtomicBool::new(false));
    let slot: Arc<OnceLock<Arc<Presence<SocketPusher>>>> = Arc::default();

    let (h, r, s) = (handle.clone(), ready.clone(), slot.clone());
    let listener = Arc::new(Listener::start(
        URL,
        None,
        session.client().token(),
        Timing::default(),
        move |out| match out {
            Out::Status(st) => {
                r.store(st == WsStatus::Connected, Ordering::SeqCst);
                *h.state::<Live>().ws_status.lock().unwrap() = st;
                let _ = h.emit("wfm-ws-status", st);
                if st == WsStatus::Connected
                    && let Some(p) = s.get().cloned()
                {
                    tauri::async_runtime::spawn(async move { p.resync().await });
                }
            }
            Out::Event(e) => {
                if e.route.contains("status/set")
                    && let Some(p) = s.get().cloned()
                {
                    let payload = e.payload.clone();
                    tauri::async_runtime::spawn(
                        async move { p.apply_server_status(&payload).await },
                    );
                }
                let _ = h.emit("wfm-ws-event", e);
            }
        },
    ));

    let h = handle.clone();
    let presence = Arc::new(Presence::new(
        SocketPusher {
            listener: listener.clone(),
            ready,
        },
        move |st| {
            let _ = h.emit("wfm-presence", st);
        },
    ));
    let _ = slot.set(presence.clone());
    app.manage(Live {
        presence: presence.clone(),
        ws_status: Mutex::new(WsStatus::Stopped),
    });

    // Rotated tokens reach the next reconnect; sign-out/expiry also forgets the account state.
    let (l, p) = (listener, presence.clone());
    session.on_token(Box::new(move |t| {
        l.set_token(t.map(str::to_owned));
        if t.is_none() {
            let p = p.clone();
            tauri::async_runtime::spawn(async move { p.reset().await });
        }
    }));

    tauri::async_runtime::spawn(async move {
        loop {
            presence.sync_game(game_running()).await;
            // No idle source keeps the last reading.
            if let Some(s) = idle_seconds() {
                presence.sync_idle(s).await;
            }
            presence.tick(now_ms()).await;
            tokio::time::sleep(GAME_POLL).await;
        }
    });
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_presence_get(live: State<'_, Live>) -> Result<PresenceState, WfmError> {
    Ok(live.presence.state().await)
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_presence_set(
    status: Status,
    live: State<'_, Live>,
) -> Result<PresenceState, WfmError> {
    live.presence
        .set_manual(status)
        .await
        .map_err(WfmError::invalid)?;
    Ok(live.presence.state().await)
}

/// Frontend pushes the persisted presence settings on boot and on every save.
#[tauri::command]
#[specta::specta]
pub async fn wfm_presence_set_options(
    options: PresenceOptions,
    live: State<'_, Live>,
) -> Result<(), WfmError> {
    live.presence.set_options(options).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn wfm_ws_status(live: State<'_, Live>) -> WsStatus {
    *live.ws_status.lock().unwrap()
}
