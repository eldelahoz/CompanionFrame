//! EE.log markers that the game fetched its inventory (`SyncInventoryFromDB`, login done). They
//! only tell the user their import is older than the game's own copy; they carry no item data.

use std::sync::Arc;

use tauri::{AppHandle, Manager};

use super::Inventory;
use crate::services::eelog_parsers::lines::{is_inventory_sync_line, is_login_complete_line};
use crate::services::trackers::{Line, LineTracker};

/// Older lines are the tail of an earlier session, not a sync that just happened.
const MAX_LINE_AGE_MS: f64 = 60_000.0;

pub struct SyncMarker(Box<dyn Fn(f64) + Send>);

impl LineTracker for SyncMarker {
    fn on_line(&mut self, line: &Line) {
        if line.staleness_ms <= MAX_LINE_AGE_MS
            && (is_inventory_sync_line(line.text) || is_login_complete_line(line.text))
        {
            (self.0)(line.now_ms - line.staleness_ms);
        }
    }

    fn on_reset(&mut self) {}
}

pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    let app = app.clone();
    Box::new(SyncMarker(Box::new(move |at| {
        app.state::<Arc<Inventory>>().note_game_sync(at)
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::Source;
    use std::sync::Mutex;

    fn feed(text: &str, staleness_ms: f64) -> Vec<f64> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let mut t = SyncMarker(Box::new(move |at| s.lock().unwrap().push(at)));
        t.on_line(&Line {
            text,
            source: Source::File,
            now_ms: 100_000.0,
            staleness_ms,
            realtime_active: false,
        });
        seen.lock().unwrap().clone()
    }

    #[test]
    fn marks_fresh_sync_lines_only() {
        assert_eq!(
            feed("24.5 Sys [Info]: SyncInventoryFromDB", 2_000.0),
            [98_000.0]
        );
        assert!(feed("24.5 Sys [Info]: SyncInventoryFromDB", 600_000.0).is_empty());
        assert!(feed("24.5 Sys [Info]: nothing", 0.0).is_empty());
    }
}
