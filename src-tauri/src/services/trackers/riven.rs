//! Riven tracker: feeds the rolling-screen state machine and hands each finished session on.
use crate::services::eelog::Source;
use crate::services::eelog_parsers::riven::{LineSource, RivenEvent, RivenStateMachine};
use crate::services::rivens::sessions::{Recorder, RivenRollSession};

use super::{Line, LineTracker};

pub struct RivenTracker {
    machine: RivenStateMachine,
    recorder: Recorder,
    on_session: Box<dyn FnMut(RivenRollSession) + Send>,
    /// Every cycle-screen event, for the riven overlay.
    on_event: Box<dyn FnMut(&RivenEvent) + Send>,
}

impl RivenTracker {
    pub fn new(
        on_session: Box<dyn FnMut(RivenRollSession) + Send>,
        on_event: Box<dyn FnMut(&RivenEvent) + Send>,
    ) -> Self {
        Self {
            machine: RivenStateMachine::new(),
            recorder: Recorder::default(),
            on_session,
            on_event,
        }
    }

    fn feed(&mut self, e: &RivenEvent, now_ms: f64) {
        (self.on_event)(e);
        if let Some(s) = self.recorder.feed(e, now_ms) {
            (self.on_session)(s);
        }
    }
}

impl LineTracker for RivenTracker {
    fn on_line(&mut self, line: &Line) {
        let source = match line.source {
            Source::Dbwin => LineSource::Dbwin,
            Source::File => LineSource::File,
        };
        let out = self
            .machine
            .process(line.text, source, line.realtime_active, line.now_ms);
        for e in &out.events {
            self.feed(e, line.now_ms);
        }
    }

    fn on_reset(&mut self) {
        self.machine.reset();
        self.recorder.reset();
    }

    fn on_tick(&mut self, now_ms: f64) {
        for e in self.machine.tick(now_ms) {
            self.feed(&e, now_ms);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const OPEN: &str = "Sys [Info]: Created /Lotus/Interface/OmegaRerollSelection.swf";
    const DIORAMA: &str = "Script [Info]: OmegaRerollSelection.lua: Diorama setup complete";
    const CYCLE: &str = "Dialog.lua: Dialog::CreateOkCancel(description=Are you sure you want to cycle Braton Prime for 1,200?, title= leftItem=/x)";
    const SEND_OK: &str = "Dialog.lua: Dialog::SendResult(4)";
    const CLOSE: &str = "Sys [Info]: NpcManager::ClearAgents() ReadyToCreateAgents = false";

    fn feed(t: &mut RivenTracker, text: &str, now_ms: f64) {
        t.on_line(&Line {
            text,
            source: Source::File,
            now_ms,
            staleness_ms: 0.0,
            realtime_active: false,
        });
    }

    type Events = Arc<Mutex<Vec<RivenEvent>>>;

    fn tracker_with_events() -> (RivenTracker, Arc<Mutex<Vec<RivenRollSession>>>, Events) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let events: Events = Arc::default();
        let (sink, esink) = (seen.clone(), events.clone());
        (
            RivenTracker::new(
                Box::new(move |s| sink.lock().unwrap().push(s)),
                Box::new(move |e| esink.lock().unwrap().push(e.clone())),
            ),
            seen,
            events,
        )
    }

    fn tracker() -> (RivenTracker, Arc<Mutex<Vec<RivenRollSession>>>) {
        let (t, seen, _) = tracker_with_events();
        (t, seen)
    }

    #[test]
    fn the_overlay_hears_the_screen_open_roll_and_close() {
        let (mut t, _, events) = tracker_with_events();
        for (i, line) in [OPEN, DIORAMA, CYCLE, SEND_OK, CLOSE].iter().enumerate() {
            feed(&mut t, line, 10_000.0 + i as f64 * 1_000.0);
        }
        let events = events.lock().unwrap();
        assert_eq!(events.first(), Some(&RivenEvent::SessionOpen));
        assert!(events.contains(&RivenEvent::RollConfirmed));
        assert_eq!(events.last(), Some(&RivenEvent::SessionClose));
    }

    #[test]
    fn a_rolled_session_is_reported_with_weapon_rolls_and_kuva() {
        let (mut t, seen) = tracker();
        for (i, line) in [OPEN, DIORAMA, CYCLE, SEND_OK, CYCLE, SEND_OK, CLOSE]
            .iter()
            .enumerate()
        {
            feed(&mut t, line, 10_000.0 + i as f64 * 1_000.0);
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let s = &seen[0];
        assert_eq!(
            (s.weapon.as_str(), s.rolls, s.kuva_spent),
            ("Braton Prime", 2, 2400)
        );
        assert_eq!((s.started_at, s.ended_at), (10_000.0, 16_000.0));
    }

    #[test]
    fn a_session_without_rolls_or_cut_by_reset_is_dropped() {
        let (mut t, seen) = tracker();
        for (i, line) in [OPEN, DIORAMA, CLOSE].iter().enumerate() {
            feed(&mut t, line, 10_000.0 + i as f64 * 1_000.0);
        }
        feed(&mut t, OPEN, 100_000.0);
        feed(&mut t, CYCLE, 101_000.0);
        feed(&mut t, SEND_OK, 102_000.0);
        t.on_reset();
        feed(&mut t, CLOSE, 103_000.0);
        assert!(seen.lock().unwrap().is_empty());
    }
}
