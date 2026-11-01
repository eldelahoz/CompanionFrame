use crate::services::clock::now_ms_f64;
use std::time::Duration;

use tokio::sync::broadcast::{self, error::RecvError};

use super::{Line, LineTracker};
use crate::services::eelog::{EeLogEvent, Source};
use crate::services::eelog_parsers::timing::{DebugLineGate, EeUptimeTracker};

const TICK: Duration = Duration::from_millis(500);
/// DBWIN counts as live while it keeps delivering; it goes quiet between missions.
const REALTIME_WINDOW_MS: f64 = 60_000.0;

pub struct Hub {
    trackers: Vec<Box<dyn LineTracker>>,
    uptime: EeUptimeTracker,
    gate: DebugLineGate,
    last_dbwin_ms: Option<f64>,
}

impl Hub {
    pub fn new(trackers: Vec<Box<dyn LineTracker>>) -> Self {
        Self {
            trackers,
            uptime: EeUptimeTracker::new(),
            gate: DebugLineGate::new(),
            last_dbwin_ms: None,
        }
    }

    pub fn handle(&mut self, event: EeLogEvent, now_ms: f64) {
        let (text, source) = match event {
            EeLogEvent::Reset => return self.reset(),
            EeLogEvent::Line { text, .. } if text.is_empty() => return,
            EeLogEvent::Line { text, source } => (text, source),
        };
        let staleness_ms = match source {
            Source::File => self.uptime.observe(&text, now_ms),
            Source::Dbwin => {
                self.last_dbwin_ms = Some(now_ms);
                // The live stream is a firehose; only lines a parser wants pass.
                if !self.gate.wants(&text, now_ms) {
                    return;
                }
                0.0
            }
        };
        let line = Line {
            text: &text,
            source,
            now_ms,
            staleness_ms,
            realtime_active: self
                .last_dbwin_ms
                .is_some_and(|at| now_ms - at < REALTIME_WINDOW_MS),
        };
        for t in &mut self.trackers {
            t.on_line(&line);
        }
    }

    pub fn tick(&mut self, now_ms: f64) {
        for t in &mut self.trackers {
            t.on_tick(now_ms);
        }
    }

    fn reset(&mut self) {
        self.uptime.reset();
        self.gate = DebugLineGate::new();
        self.last_dbwin_ms = None;
        for t in &mut self.trackers {
            t.on_reset();
        }
    }
}

pub async fn run(mut hub: Hub, mut rx: broadcast::Receiver<EeLogEvent>) {
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) => hub.handle(ev, now_ms_f64()),
                // Lines were lost mid-run, so any half-built state is suspect.
                Err(RecvError::Lagged(n)) => {
                    tracing::warn!(n, "tracker hub lagged; resetting trackers");
                    hub.handle(EeLogEvent::Reset, now_ms_f64());
                }
                Err(RecvError::Closed) => return,
            },
            _ = tick.tick() => hub.tick(now_ms_f64()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::eelog::tail::{Out, Tail};
    use crate::services::eelog::tests::tmp_dir;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(String, Source, f64, bool)>>>;

    #[derive(Default)]
    struct Probe {
        seen: Seen,
        resets: Arc<Mutex<u32>>,
    }

    impl LineTracker for Probe {
        fn on_line(&mut self, l: &Line) {
            self.seen.lock().unwrap().push((
                l.text.to_string(),
                l.source,
                l.staleness_ms,
                l.realtime_active,
            ));
        }
        fn on_reset(&mut self) {
            *self.resets.lock().unwrap() += 1;
        }
    }

    fn hub() -> (Hub, Seen, Arc<Mutex<u32>>) {
        let p = Probe::default();
        let (seen, resets) = (p.seen.clone(), p.resets.clone());
        (Hub::new(vec![Box::new(p)]), seen, resets)
    }

    fn file(s: &str) -> EeLogEvent {
        EeLogEvent::Line {
            text: s.into(),
            source: Source::File,
        }
    }

    #[test]
    fn a_fixture_replayed_by_the_tail_reaches_a_tracker_in_order() {
        let path = tmp_dir("hub").join("EE.log");
        std::fs::write(
            &path,
            "1.000 Sys [Info]: a\n2.000 Sys [Info]: b\n3.000 Sys [Info]: c\n",
        )
        .unwrap();
        let mut tail = Tail::new(path, false);
        let (mut hub, seen, _) = hub();
        tail.poll(|o| {
            if let Out::Line(text) = o {
                hub.handle(file(&text), 1_000_000.0);
            }
        });
        let texts: Vec<_> = seen.lock().unwrap().iter().map(|s| s.0.clone()).collect();
        assert_eq!(
            texts,
            [
                "1.000 Sys [Info]: a",
                "2.000 Sys [Info]: b",
                "3.000 Sys [Info]: c"
            ]
        );
    }

    #[test]
    fn reset_reaches_trackers_and_restarts_the_uptime_baseline() {
        let (mut hub, seen, resets) = hub();
        hub.handle(file("100.000 Sys [Info]: a"), 1_000_100_000.0);
        // Five seconds late against the baseline.
        hub.handle(file("101.000 Sys [Info]: b"), 1_000_106_000.0);
        assert_eq!(seen.lock().unwrap()[1].2, 5_000.0);
        hub.handle(EeLogEvent::Reset, 0.0);
        assert_eq!(*resets.lock().unwrap(), 1);
        hub.handle(file("102.000 Sys [Info]: c"), 2_000_000_000.0);
        assert_eq!(seen.lock().unwrap()[2].2, 0.0, "baseline forgotten");
    }

    #[test]
    fn dbwin_lines_pass_the_gate_and_mark_the_realtime_stream_live() {
        let (mut hub, seen, _) = hub();
        let dbwin = |s: &str| EeLogEvent::Line {
            text: s.into(),
            source: Source::Dbwin,
        };
        hub.handle(dbwin("noise nobody parses"), 1_000.0);
        hub.handle(
            dbwin("Script Info: MainMenu::LoginDone result=true"),
            2_000.0,
        );
        hub.handle(file("5.000 Sys [Info]: echo"), 3_000.0);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "gated line dropped");
        assert_eq!((seen[0].1, seen[0].3), (Source::Dbwin, true));
        assert_eq!((seen[1].1, seen[1].3), (Source::File, true));
    }
}
