//! Game lifecycle from the log alone: is the game writing, has the player logged in, and which
//! mission tag is active.

use std::sync::{Arc, Mutex};

use serde::Serialize;

use super::{Line, LineTracker};
use crate::services::eelog_parsers::lines::{
    is_login_complete_line, is_mission_end_line, parse_active_mission_tag,
};

/// The same login can arrive twice (DBWIN, then the file's lazy flush).
const LOGIN_DEDUP_MS: f64 = 30_000.0;
/// The log has no shutdown line; a game that writes nothing this long is gone. Generous because
/// the engine buffers file writes for tens of seconds when idle.
const IDLE_MS: f64 = 180_000.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
pub struct GameStatus {
    pub running: bool,
    pub logged_in: bool,
    pub mission_tag: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum GameEvent {
    Login,
    Running(bool),
}

pub type Shared = Arc<Mutex<GameStatus>>;

pub struct Lifecycle {
    status: Shared,
    emit: Box<dyn Fn(GameEvent) + Send>,
    last_line_ms: f64,
    last_login_ms: Option<f64>,
}

impl Lifecycle {
    pub fn new(emit: impl Fn(GameEvent) + Send + 'static) -> (Self, Shared) {
        let status = Shared::default();
        let me = Self {
            status: status.clone(),
            emit: Box::new(emit),
            last_line_ms: 0.0,
            last_login_ms: None,
        };
        (me, status)
    }

    fn set_running(&mut self, on: bool) {
        let mut s = self.status.lock().unwrap();
        if s.running == on {
            return;
        }
        s.running = on;
        if !on {
            *s = GameStatus::default();
        }
        drop(s);
        (self.emit)(GameEvent::Running(on));
    }
}

impl LineTracker for Lifecycle {
    fn on_line(&mut self, line: &Line) {
        self.last_line_ms = line.now_ms;
        self.set_running(true);

        if is_login_complete_line(line.text)
            && self
                .last_login_ms
                .is_none_or(|at| line.now_ms - at >= LOGIN_DEDUP_MS)
        {
            self.last_login_ms = Some(line.now_ms);
            self.status.lock().unwrap().logged_in = true;
            (self.emit)(GameEvent::Login);
        }

        if let Some(tag) = parse_active_mission_tag(line.text) {
            self.status.lock().unwrap().mission_tag = Some(tag);
        } else if is_mission_end_line(line.text) {
            self.status.lock().unwrap().mission_tag = None;
        }
    }

    fn on_reset(&mut self) {
        self.last_login_ms = None;
        self.set_running(false);
    }

    fn on_tick(&mut self, now_ms: f64) {
        if self.status.lock().unwrap().running && now_ms - self.last_line_ms >= IDLE_MS {
            self.last_login_ms = None;
            self.set_running(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::Source;

    fn setup() -> (Lifecycle, Shared, Arc<Mutex<Vec<GameEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        let (l, status) = Lifecycle::new(move |e| sink.lock().unwrap().push(e));
        (l, status, events)
    }

    fn feed(l: &mut Lifecycle, text: &str, source: Source, now_ms: f64) {
        l.on_line(&Line {
            text,
            source,
            now_ms,
            staleness_ms: 0.0,
            realtime_active: false,
        });
    }

    const LOGIN: &str = "10.000 Script Info: MainMenu::LoginDone result=true";

    #[test]
    fn login_fires_once_across_dbwin_and_its_file_echo() {
        let (mut l, status, events) = setup();
        feed(&mut l, LOGIN, Source::Dbwin, 1_000.0);
        feed(&mut l, LOGIN, Source::File, 12_000.0);
        assert_eq!(
            *events.lock().unwrap(),
            [GameEvent::Running(true), GameEvent::Login]
        );
        assert!(status.lock().unwrap().logged_in);
        // A genuine second login after the window counts again.
        feed(&mut l, LOGIN, Source::File, 60_000.0);
        assert_eq!(events.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_login_line_is_not_logged_in_without_the_success_marker() {
        let (mut l, status, _) = setup();
        feed(
            &mut l,
            "MainMenu::LoginDone result=false",
            Source::File,
            0.0,
        );
        assert!(!status.lock().unwrap().logged_in);
    }

    #[test]
    fn reset_clears_the_session() {
        let (mut l, status, events) = setup();
        feed(&mut l, LOGIN, Source::File, 0.0);
        feed(
            &mut l,
            r#"    "activeMissionTag" : "VoidT2","#,
            Source::File,
            1.0,
        );
        assert_eq!(
            status.lock().unwrap().mission_tag.as_deref(),
            Some("VoidT2")
        );
        l.on_reset();
        assert_eq!(*status.lock().unwrap(), GameStatus::default());
        assert_eq!(
            events.lock().unwrap().last(),
            Some(&GameEvent::Running(false))
        );
        // The first login after a reset is not deduped against the old one.
        feed(&mut l, LOGIN, Source::File, 5_000.0);
        assert!(status.lock().unwrap().logged_in);
    }

    #[test]
    fn mission_tag_is_cleared_when_the_mission_ends() {
        let (mut l, status, _) = setup();
        feed(
            &mut l,
            r#"    "activeMissionTag" : "VoidT6","#,
            Source::File,
            0.0,
        );
        feed(
            &mut l,
            "Sys [Info]: EOM missionLocationUnlocked=Foo",
            Source::File,
            1.0,
        );
        assert_eq!(status.lock().unwrap().mission_tag, None);
    }

    #[test]
    fn a_silent_game_stops_running_after_the_idle_timeout() {
        let (mut l, status, events) = setup();
        feed(&mut l, "1.000 Sys [Info]: hi", Source::File, 0.0);
        l.on_tick(IDLE_MS - 1.0);
        assert!(status.lock().unwrap().running);
        l.on_tick(IDLE_MS);
        assert!(!status.lock().unwrap().running);
        assert_eq!(
            events.lock().unwrap().last(),
            Some(&GameEvent::Running(false))
        );
    }
}
