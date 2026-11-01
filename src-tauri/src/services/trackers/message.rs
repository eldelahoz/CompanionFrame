//! Whisper tracker: a private chat tab in EE.log means someone wrote to the player.
use crate::services::eelog_parsers::lines::parse_whisper_username;
use crate::services::eelog_parsers::timing::EchoFilter;
use tauri::{AppHandle, Manager};

use super::{Line, LineTracker};
use crate::services::notify::{MessageOptions, Notifications, notify_in_game_message};
use crate::store::Store;

/// The file poll re-delivers DBWIN-handled lines up to ~26 s later (lazy flush).
const ECHO_WINDOW_MS: f64 = 30_000.0;

pub struct MessageTracker {
    echoes: EchoFilter,
    on_whisper: Box<dyn FnMut(&str) + Send>,
}

impl MessageTracker {
    pub fn new(on_whisper: Box<dyn FnMut(&str) + Send>) -> Self {
        Self {
            echoes: EchoFilter::new(ECHO_WINDOW_MS),
            on_whisper,
        }
    }
}

impl LineTracker for MessageTracker {
    fn on_line(&mut self, line: &Line) {
        let Some(user) = parse_whisper_username(line.text) else {
            return;
        };
        if !self.echoes.is_echo(&user, line.now_ms) {
            (self.on_whisper)(&user);
        }
    }

    fn on_reset(&mut self) {
        self.echoes.clear();
    }
}

/// The live tracker for the hub.
pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    let app = app.clone();
    Box::new(MessageTracker::new(Box::new(move |player| {
        let s = app.state::<Store>().get();
        #[cfg(windows)]
        let focused = Some(crate::platform::windows::process::foreground_is_warframe());
        #[cfg(not(windows))]
        let focused = None;
        let opts = MessageOptions {
            focused,
            while_focused: s.message_notifications_while_focused,
            native_enabled: s.message_notifications_enabled,
        };
        notify_in_game_message(&*app.state::<Notifications>(), player, &opts);
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::Source;
    use std::sync::{Arc, Mutex};

    const TAB: &str =
        "Sys [Info]: ChatRedux::AddTab: Adding tab with channel name: FTenno\u{E000} to index 3";

    fn feed(t: &mut MessageTracker, text: &str, now_ms: f64) {
        t.on_line(&Line {
            text,
            source: Source::File,
            now_ms,
            staleness_ms: 0.0,
            realtime_active: false,
        });
    }

    fn tracker_with_log() -> (MessageTracker, Arc<Mutex<Vec<String>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        let t = MessageTracker::new(Box::new(move |p| s.lock().unwrap().push(p.to_string())));
        (t, seen)
    }

    #[test]
    fn a_whisper_tab_fires_once_and_echoes_inside_30s_are_dropped() {
        let (mut t, seen) = tracker_with_log();
        feed(&mut t, TAB, 0.0);
        feed(&mut t, TAB, 20_000.0);
        assert_eq!(*seen.lock().unwrap(), ["Tenno"]);
        feed(&mut t, TAB, 100_000.0);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn other_lines_and_non_private_tabs_are_ignored_and_reset_forgets_echoes() {
        let (mut t, seen) = tracker_with_log();
        feed(
            &mut t,
            "Sys [Info]: ChatRedux::AddTab: Adding tab with channel name: Region to index 1",
            0.0,
        );
        feed(&mut t, "Sys [Info]: unrelated", 0.0);
        assert!(seen.lock().unwrap().is_empty());
        feed(&mut t, TAB, 0.0);
        t.on_reset();
        feed(&mut t, TAB, 1.0);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }
}
