//! Tracker hub: the one subscriber to the EE.log broadcast. It stamps each line with timing and
//! source facts and fans it out to the trackers; trackers never touch the broadcast themselves.

mod hub;
mod lifecycle;
mod message;
mod riven;
mod trade;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::services::eelog::{EeLog, Source};
use hub::Hub;
use lifecycle::{GameEvent, GameStatus, Lifecycle};
pub use riven::RivenTracker;
pub use trade::{EVENT as TRADE_EVENT, TradeDetected};

/// One log line plus what the hub learned about it.
pub struct Line<'a> {
    pub text: &'a str,
    pub source: Source,
    /// Wall clock, ms since the epoch.
    pub now_ms: f64,
    /// How stale a file line is (engine flush lag); always 0 for the live DBWIN stream.
    pub staleness_ms: f64,
    /// The live debug stream is delivering, so file lines are probably echoes of it.
    pub realtime_active: bool,
}

pub trait LineTracker: Send {
    fn on_line(&mut self, line: &Line);
    /// The log restarted or switched file: drop per-session state.
    fn on_reset(&mut self);
    /// Called a couple of times a second even when no line arrives, for timeouts.
    fn on_tick(&mut self, _now_ms: f64) {}
}

#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct TrackersStatus {
    pub game: GameStatus,
}

pub struct Trackers {
    game: lifecycle::Shared,
}

#[tauri::command]
#[specta::specta]
pub fn trackers_status(trackers: State<Trackers>) -> TrackersStatus {
    TrackersStatus {
        game: trackers.game.lock().unwrap().clone(),
    }
}

/// A log the game starts while the app runs: its first line names the game language.
struct GameLanguage(AppHandle);

impl LineTracker for GameLanguage {
    fn on_line(&mut self, line: &Line) {
        if let Some(code) = crate::services::eelog_parsers::lines::command_line_language(line.text)
        {
            crate::services::item_db::set_log_language(&self.0, &code);
        }
    }

    fn on_reset(&mut self) {}
}

/// Manages `Trackers` and spawns the hub task. Must run after `eelog::start`.
pub fn setup(app: &AppHandle) {
    let rx = app.state::<EeLog>().subscribe();
    let handle = app.clone();
    let (lifecycle, game) = Lifecycle::new(move |e| {
        let _ = match e {
            GameEvent::Login => handle.emit("game-login", ()),
            GameEvent::Running(on) => handle.emit("game-running", on),
        };
    });
    app.manage(Trackers { game });
    let hub = Hub::new(vec![
        Box::new(lifecycle),
        crate::services::runs::arbi_tracker(app),
        crate::services::runs::pt_tracker(app),
        trade::tracker(app),
        crate::services::inventory::sync_marker::tracker(app),
        message::tracker(app),
        crate::services::rivens::tracker(app),
        crate::services::mission_rewards::tracker(app),
        crate::services::overlay::arbi::tracker(app),
        crate::services::overlay::reward::tracker(app),
        crate::services::relic_planner::tracker(app),
        Box::new(GameLanguage(app.clone())),
    ]);
    tauri::async_runtime::spawn(hub::run(hub, rx));
}
