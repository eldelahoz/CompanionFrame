//! Live arbitration counter: its own `ArbiParser` over the EE.log lines, a snapshot pushed to the
//! arbi overlay every tick while a run is open, and the window shown/hidden with the run.

use tauri::{AppHandle, Emitter};

use super::{OverlayKind, with};
use crate::services::eelog::Source;
use crate::services::eelog_parsers::arbi::{ArbiLive, ArbiParser, ArbiParserEvent};
use crate::services::trackers::{Line, LineTracker};

/// `arbi-live` carries `Option<ArbiLive>`: a snapshot while a run is open, null once it closes.
pub const EVENT: &str = "arbi-live";

#[derive(Default)]
struct Live {
    parser: ArbiParser,
    /// The overlay was told a run is open.
    open: bool,
}

impl Live {
    fn feed(&mut self, text: &str) {
        let ended = matches!(
            self.parser.feed_line(text),
            Some(ArbiParserEvent::RunEnd { .. })
        );
        if ended {
            self.parser.finalize();
            // Back-to-back runs: the ending line may itself open the next one.
            self.parser.feed_line(text);
        }
    }

    /// `Some(state)` when the overlay needs telling: a snapshot, or None once the run closed.
    fn poll(&mut self) -> Option<Option<ArbiLive>> {
        match self.parser.snapshot() {
            Some(s) => {
                self.open = true;
                Some(Some(s))
            }
            None => std::mem::take(&mut self.open).then_some(None),
        }
    }

    fn reset(&mut self) {
        self.parser.reset();
    }
}

pub struct ArbiLiveTracker {
    app: AppHandle,
    live: Live,
}

pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    Box::new(ArbiLiveTracker {
        app: app.clone(),
        live: Live::default(),
    })
}

impl ArbiLiveTracker {
    fn push(&mut self) {
        let Some(state) = self.live.poll() else {
            return;
        };
        let on = state.is_some();
        let _ = with(&self.app, |m| match (on, m.is_shown(OverlayKind::Arbi)) {
            (true, false) => m.show(OverlayKind::Arbi),
            (false, true) => m.hide(OverlayKind::Arbi),
            _ => {}
        });
        let _ = self.app.emit(EVENT, state);
    }
}

impl LineTracker for ArbiLiveTracker {
    fn on_line(&mut self, line: &Line) {
        if line.source == Source::File {
            self.live.feed(line.text);
        }
    }

    fn on_reset(&mut self) {
        self.live.reset();
        self.push();
    }

    fn on_tick(&mut self, _: f64) {
        self.push();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/eelog/arbi/");

    fn lines(name: &str) -> Vec<String> {
        std::fs::read_to_string(format!("{FIXTURES}{name}"))
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }

    /// Feeds a fixture, polling after every line like the 500 ms tick would, and returns what
    /// the overlay was told.
    fn replay(name: &str) -> Vec<Option<ArbiLive>> {
        let mut live = Live::default();
        let mut out = Vec::new();
        for l in lines(name) {
            live.feed(&l);
            out.extend(live.poll());
        }
        out
    }

    #[test]
    fn counter_follows_a_defense_run_then_clears() {
        let told = replay("stoefler-defense-ee.log");
        assert!(told.last().unwrap().is_none(), "cleared at the end marker");
        let seen: Vec<&ArbiLive> = told.iter().flatten().collect();
        let last = seen.last().unwrap();
        assert!(!last.node.is_empty());
        assert_eq!((last.rotations, last.drones), (2, 3));
        assert!(last.enemies >= last.drones);
        assert!(last.elapsed_sec > 100.0);
        assert!(last.dpm > 0.0);
        // Totals only grow while the run is open.
        assert!(seen.windows(2).all(|w| w[0].drones <= w[1].drones));
        // Elapsed may drop once, when the start anchor moves from mission load to the first combat.
    }

    #[test]
    fn aborted_run_clears_too() {
        let told = replay("oestrus-abort-ee.log");
        assert!(told.iter().any(Option::is_some));
        assert!(told.last().unwrap().is_none());
    }

    #[test]
    fn nothing_is_told_while_idle() {
        let mut live = Live::default();
        live.feed("1.000 Sys [Info]: nothing to see");
        assert_eq!(live.poll(), None);
    }

    #[test]
    fn log_restart_mid_run_clears() {
        let mut live = Live::default();
        for l in lines("stoefler-defense-ee.log").iter().take(12) {
            live.feed(l);
        }
        assert!(matches!(live.poll(), Some(Some(_))));
        live.reset();
        assert_eq!(live.poll(), Some(None));
    }
}
