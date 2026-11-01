//! What every EE.log run feature shares: the live capture/finalize machinery, the importer that
//! splits an external log into runs, run ids and cross-source duplicate detection. A feature
//! supplies a `SegmentParser`; port of WFHelper's generic `runTracker` and `eeLogSegmentImporter`.
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use chrono::{Local, NaiveDateTime, TimeZone};
use regex::Regex;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::store;
use crate::services::eelog::Source;
use crate::services::eelog_parsers::line_ts;
use crate::services::trackers::{Line, LineTracker};

/// Finalize a run when no combat activity arrives for this long (crash to desktop, connection loss).
const INACTIVITY_TIMEOUT_MS: f64 = 10.0 * 60_000.0;
pub const MAX_SEGMENT_BYTES: usize = 256 * 1024 * 1024;
/// A live capture and an imported log describe one run when they start close together and last
/// the same; the wall-clock start can drift by the import's header anchor, the duration cannot.
const DUPLICATE_START_TOLERANCE_MS: f64 = 120_000.0;
const DUPLICATE_DURATION_TOLERANCE_SEC: f64 = 5.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RunSource {
    Live,
    Imported,
}

pub fn tag<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

pub fn format_id(ms: f64) -> String {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .unwrap_or_default()
        .format("%Y-%m-%d_%H-%M-%S")
        .to_string()
}

/// A live run takes a free `-n` suffix (back-to-back runs share a second); an import is None when
/// its id is already stored, which is what makes re-importing a log idempotent.
pub fn run_id(conn: &Connection, started_at_ms: f64, source: RunSource) -> Option<String> {
    let base = format_id(started_at_ms);
    match source {
        RunSource::Imported => (!store::exists(conn, &base)).then_some(base),
        RunSource::Live => {
            let mut id = base.clone();
            let mut n = 2;
            while store::exists(conn, &id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            Some(id)
        }
    }
}

pub enum SegEvent<R> {
    /// A run opened; the game-clock second it starts at.
    Start(f64),
    End(R),
}

/// An EE.log run parser as the tracker and importer see it.
pub trait SegmentParser: Default + Send {
    type Run;
    type Reason: Copy;
    fn feed(&mut self, line: &str) -> Option<SegEvent<Self::Reason>>;
    fn is_active(&self) -> bool;
    /// Monotonic combat activity count; lines that do not bump it cannot keep a run alive.
    fn activity_count(&self) -> u32;
    fn finalize(&mut self) -> Option<Self::Run>;
    fn reset(&mut self);
    fn run_start_sec(run: &Self::Run) -> f64;
    fn truncated() -> Self::Reason;
    fn inactive() -> Self::Reason;
    fn imported() -> Self::Reason;
}

/// A closed run waiting to be saved.
pub struct Finished<R, E> {
    pub parsed: R,
    /// Wall clock of the run start, ms since the epoch.
    pub started_at_ms: f64,
    pub reason: E,
    pub lines: Vec<String>,
}

pub struct Capture {
    pub lines: Vec<String>,
    bytes: usize,
    cap: usize,
    truncated: bool,
}

impl Capture {
    pub fn new(first: &str, cap: usize) -> Self {
        let mut c = Self {
            lines: Vec::new(),
            bytes: 0,
            cap,
            truncated: false,
        };
        c.push(first);
        c
    }

    pub fn push(&mut self, line: &str) {
        if self.truncated {
            return;
        }
        let n = line.len() + 1;
        if self.bytes + n > self.cap {
            tracing::warn!("run capture exceeds cap, raw log truncated");
            self.truncated = true;
            return;
        }
        self.bytes += n;
        self.lines.push(line.to_string());
    }
}

struct Active {
    /// Wall clock of the line that opened the capture.
    wall_ms: f64,
    /// Game time of that line; anchors the record's wall clock.
    captured_at_sec: f64,
    cap: Capture,
    /// Wall clock of the last combat event (orbiter chatter after a mission must not keep it alive).
    last_combat_ms: f64,
    last_count: u32,
}

type Sink<P> =
    Box<dyn FnMut(Finished<<P as SegmentParser>::Run, <P as SegmentParser>::Reason>) + Send>;

/// The live tracker: feeds file lines to the parser and hands each closed run to the sink.
pub struct RunTracker<P: SegmentParser> {
    parser: P,
    active: Option<Active>,
    sink: Sink<P>,
}

impl<P: SegmentParser> RunTracker<P> {
    pub fn new(sink: Sink<P>) -> Self {
        Self {
            parser: P::default(),
            active: None,
            sink,
        }
    }

    fn start(&mut self, game_time_sec: f64, line: &Line) {
        self.active = Some(Active {
            wall_ms: line.now_ms - line.staleness_ms,
            captured_at_sec: game_time_sec,
            cap: Capture::new(line.text, MAX_SEGMENT_BYTES),
            last_combat_ms: line.now_ms,
            last_count: 0,
        });
    }

    fn finalize(&mut self, reason: P::Reason) {
        let Some(a) = self.active.take() else { return };
        let Some(parsed) = self.parser.finalize() else {
            return;
        };
        let started_at_ms = a.wall_ms + (P::run_start_sec(&parsed) - a.captured_at_sec) * 1000.0;
        (self.sink)(Finished {
            parsed,
            started_at_ms,
            reason,
            lines: a.cap.lines,
        });
    }
}

impl<P: SegmentParser> LineTracker for RunTracker<P> {
    fn on_line(&mut self, line: &Line) {
        // File lines are complete, ordered and deduped; the DBWIN stream duplicates them.
        if line.source != Source::File {
            return;
        }
        match self.parser.feed(line.text) {
            Some(SegEvent::Start(sec)) => self.start(sec, line),
            Some(SegEvent::End(reason)) => {
                self.finalize(reason);
                // Back-to-back runs: the ending line may itself start the next one.
                if let Some(SegEvent::Start(sec)) = self.parser.feed(line.text) {
                    self.start(sec, line);
                }
            }
            None => {
                let count = self.parser.activity_count();
                if let Some(a) = &mut self.active {
                    a.cap.push(line.text);
                    if count != a.last_count {
                        a.last_count = count;
                        a.last_combat_ms = line.now_ms;
                    }
                }
            }
        }
    }

    fn on_reset(&mut self) {
        self.finalize(P::truncated());
        self.parser.reset();
    }

    fn on_tick(&mut self, now_ms: f64) {
        if self
            .active
            .as_ref()
            .is_some_and(|a| now_ms - a.last_combat_ms > INACTIVITY_TIMEOUT_MS)
        {
            tracing::info!("no activity events for 10min, finalizing the run");
            self.finalize(P::inactive());
        }
    }
}

/// EE.log header, e.g. "Sys [Diag]: Current time: Fri Jul 04 12:34:56 2026 [UTC: Fri Jul 04 10:34:56 2026]".
static CURRENT_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Sys \[Diag\]: Current time: (.+?)(?: \[UTC: (.+?)\])?\s*$").unwrap()
});

fn parse_stamp(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim(), "%a %b %d %H:%M:%S %Y").ok()
}

/// Wall clock of a header line; the UTC stamp wins, since the local one is the log author's.
fn header_wall_ms(line: &str) -> Option<f64> {
    let c = CURRENT_TIME.captures(line)?;
    let ms = match c.get(2).and_then(|u| parse_stamp(u.as_str())) {
        Some(utc) => utc.and_utc().timestamp_millis(),
        None => parse_stamp(&c[1])?
            .and_local_timezone(Local)
            .single()?
            .timestamp_millis(),
    };
    Some(ms as f64)
}

/// Splits an external EE.log into runs and hands each to `save`; None counts as skipped.
pub fn import_segments<P: SegmentParser, T>(
    path: &Path,
    max_segment_bytes: usize,
    mut save: impl FnMut(Finished<P::Run, P::Reason>) -> Result<Option<T>, String>,
) -> Result<(Vec<T>, u32), String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mtime_ms = file
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_millis() as f64);
    let mut reader = BufReader::new(file);

    let mut parser = P::default();
    let mut zero_ms: Option<f64> = None;
    let mut last_ts = 0.0f64;
    let mut capture: Option<Capture> = None;
    let mut imported = Vec::new();
    let mut skipped = 0u32;

    let mut buf = Vec::new();
    loop {
        buf.clear();
        if reader
            .read_until(b'\n', &mut buf)
            .map_err(|e| e.to_string())?
            == 0
        {
            break;
        }
        let text = String::from_utf8_lossy(&buf);
        let line = text.trim_end_matches(['\n', '\r']);

        if zero_ms.is_none()
            && let Some(wall) = header_wall_ms(line)
        {
            zero_ms = Some(wall - line_ts(line) * 1000.0);
        }
        last_ts = last_ts.max(line_ts(line));

        let mut event = parser.feed(line);
        if let Some(SegEvent::End(reason)) = event {
            finish(
                &mut parser,
                &mut capture,
                zero_ms,
                last_ts,
                mtime_ms,
                reason,
                &mut save,
                &mut imported,
                &mut skipped,
            )?;
            // The ending line may itself start the next run.
            event = parser
                .feed(line)
                .filter(|e| matches!(e, SegEvent::Start(_)));
        }
        if let Some(SegEvent::Start(_)) = event {
            capture = Some(Capture::new(line, max_segment_bytes));
        } else if parser.is_active()
            && let Some(c) = &mut capture
        {
            c.push(line);
        }
    }
    if parser.is_active() {
        finish(
            &mut parser,
            &mut capture,
            zero_ms,
            last_ts,
            mtime_ms,
            P::imported(),
            &mut save,
            &mut imported,
            &mut skipped,
        )?;
    }
    tracing::info!(
        "import done: {} run(s) imported, {skipped} skipped",
        imported.len()
    );
    Ok((imported, skipped))
}

#[allow(clippy::too_many_arguments)]
fn finish<P: SegmentParser, T>(
    parser: &mut P,
    capture: &mut Option<Capture>,
    zero_ms: Option<f64>,
    last_ts: f64,
    mtime_ms: f64,
    reason: P::Reason,
    save: &mut impl FnMut(Finished<P::Run, P::Reason>) -> Result<Option<T>, String>,
    imported: &mut Vec<T>,
    skipped: &mut u32,
) -> Result<(), String> {
    let (Some(parsed), Some(cap)) = (parser.finalize(), capture.take()) else {
        return Ok(());
    };
    let start = P::run_start_sec(&parsed);
    let started_at_ms = match zero_ms {
        Some(z) => z + start * 1000.0,
        // No header timestamp: anchor the file's last line to its mtime.
        None => mtime_ms - (last_ts - start).max(0.0) * 1000.0,
    };
    match save(Finished {
        parsed,
        started_at_ms,
        reason,
        lines: cap.lines,
    })? {
        Some(r) => imported.push(r),
        None => *skipped += 1,
    }
    Ok(())
}

/// What duplicate detection needs to know about a stored run.
pub struct Dup {
    pub started_at: f64,
    pub duration_sec: f64,
    pub live: bool,
    pub log_size: f64,
    /// Higher wins before log size and source are compared.
    pub richness: u8,
    /// Extra condition for two runs to be the same (the node, for arbitration).
    pub target: String,
}

fn richer(d: &[Dup], i: usize, j: usize) -> (usize, usize) {
    let (a, b) = (&d[i], &d[j]);
    if a.richness != b.richness {
        return if a.richness > b.richness {
            (i, j)
        } else {
            (j, i)
        };
    }
    if a.log_size != b.log_size {
        return if a.log_size > b.log_size {
            (i, j)
        } else {
            (j, i)
        };
    }
    if a.live { (i, j) } else { (j, i) }
}

/// For each run the index of the richer record it duplicates, or None when it is unique. Only
/// cross-source pairs qualify, so two genuine back-to-back runs stay separate.
pub fn duplicate_targets(d: &[Dup]) -> Vec<Option<usize>> {
    let mut winners = std::collections::HashMap::new();
    for i in 0..d.len() {
        for j in i + 1..d.len() {
            let (a, b) = (&d[i], &d[j]);
            let same = (a.started_at - b.started_at).abs() <= DUPLICATE_START_TOLERANCE_MS
                && (a.duration_sec - b.duration_sec).abs() <= DUPLICATE_DURATION_TOLERANCE_SEC
                && a.target == b.target;
            if a.live == b.live || !same {
                continue;
            }
            let (w, l) = richer(d, i, j);
            winners.entry(l).or_insert(w);
        }
    }
    (0..d.len())
        .map(|k| {
            let mut at = *winners.get(&k)?;
            for _ in 0..3 {
                match winners.get(&at) {
                    Some(&next) => at = next,
                    None => break,
                }
            }
            (at != k).then_some(at)
        })
        .collect()
}
