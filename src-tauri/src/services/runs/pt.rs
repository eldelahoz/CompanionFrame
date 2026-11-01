//! Profit Taker runs on the shared run storage. Port of WFHelper's `profitTakerTracker` and
//! `profitTakerLogImporter`; the stats live in the frontend (`lib/profitTakerStats.ts`).
use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::segment::{
    self, Dup, RunSource, RunTracker, SegEvent, SegmentParser, duplicate_targets, run_id, tag,
};
use super::store::{self, RunRow};
use crate::services::eelog_parsers::profit_taker::{
    ProfitTakerParser, PtParsedRun, PtParserEvent, PtPhase, PtRunEndReason,
};

pub const KIND: &str = "pt";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum PtRecordEndReason {
    Completed,
    Aborted,
    MissionEnd,
    NewRun,
    LogTruncated,
    AppQuit,
    Inactivity,
    Imported,
}

impl From<PtRunEndReason> for PtRecordEndReason {
    fn from(r: PtRunEndReason) -> Self {
        match r {
            PtRunEndReason::Completed => Self::Completed,
            PtRunEndReason::MissionEnd => Self::MissionEnd,
            PtRunEndReason::Aborted => Self::Aborted,
            PtRunEndReason::NewRun => Self::NewRun,
        }
    }
}

/// What the UI lists. Everything is stored in `runs.summary`; tags, notes and the duplicate link
/// live in their own columns and win on read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PtRunRecord {
    /// "YYYY-MM-DD_HH-mm-ss" local wall clock at run start.
    pub id: String,
    /// Wall clock, ms since the epoch.
    pub started_at: f64,
    pub ended_at: f64,
    pub duration_sec: f64,
    pub flight_sec: f64,
    pub shield_sec: f64,
    pub leg_sec: f64,
    pub body_sec: f64,
    pub pylon_sec: f64,
    pub phases: Vec<PtPhase>,
    /// PB groups count these recorded names.
    #[serde(default)]
    pub players: Vec<String>,
    pub complete: bool,
    pub bugged: bool,
    pub aborted: bool,
    #[serde(default)]
    pub host_migration: bool,
    /// No elevator-exit line, so flight time was measured from the job-start line.
    #[serde(default)]
    pub flight_unreliable: bool,
    /// False once the raw capture is deleted.
    #[serde(default)]
    pub has_log: bool,
    pub log_size_bytes: f64,
    pub end_reason: PtRecordEndReason,
    pub source: RunSource,
    #[serde(default)]
    pub tags: Vec<String>,
    pub notes: Option<String>,
    /// Id of the record this one duplicates.
    pub duplicate_of: Option<String>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PtRunsPayload {
    pub runs: Vec<PtRunRecord>,
    pub disk_usage_bytes: f64,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PtImportResult {
    pub imported: Vec<PtRunRecord>,
    pub skipped: u32,
}

pub type Finished = segment::Finished<PtParsedRun, PtRecordEndReason>;

fn skip_reason(p: &PtParsedRun) -> Option<&'static str> {
    if !p.confirmed {
        Some("bounty was never entered")
    } else if !p.host_telemetry {
        Some("client-side run, no Camper telemetry in the log")
    } else if p.phases.is_empty() {
        Some("no phase data")
    } else {
        None
    }
}

fn from_row(row: RunRow) -> Option<PtRunRecord> {
    let mut r: PtRunRecord = serde_json::from_str(&row.summary).ok()?;
    r.tags = row.tags;
    r.notes = row.notes;
    r.duplicate_of = row.duplicate_of;
    r.has_log = row.log_size > 0;
    r.log_size_bytes = row.log_size as f64;
    Some(r)
}

pub fn get(conn: &Connection, id: &str) -> Option<PtRunRecord> {
    store::get(conn, id)
        .ok()
        .flatten()
        .filter(|r| r.kind == KIND)
        .and_then(from_row)
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<PtRunRecord>> {
    Ok(store::list(conn, KIND)?
        .into_iter()
        .filter_map(from_row)
        .collect())
}

pub fn payload(conn: &Connection) -> rusqlite::Result<PtRunsPayload> {
    Ok(PtRunsPayload {
        runs: list(conn)?,
        disk_usage_bytes: store::disk_usage(conn, KIND)? as f64,
    })
}

/// Persists a closed run. None when it is not worth keeping or, for an import, already stored.
pub fn save(
    conn: &mut Connection,
    f: Finished,
    source: RunSource,
) -> rusqlite::Result<Option<PtRunRecord>> {
    if let Some(why) = skip_reason(&f.parsed) {
        tracing::info!("[PT] run not saved ({why})");
        return Ok(None);
    }
    let Some(id) = run_id(conn, f.started_at_ms, source) else {
        return Ok(None);
    };
    let blob = store::gzip(&f.lines);
    let p = f.parsed;
    let end_sec = p.run_end_sec.unwrap_or(p.last_activity_sec);
    let ended_at = f.started_at_ms + ((end_sec - p.run_start_sec) * 1000.0).max(0.0);
    let record = PtRunRecord {
        id: id.clone(),
        started_at: f.started_at_ms,
        ended_at,
        duration_sec: p.duration_sec,
        flight_sec: p.flight_sec,
        shield_sec: p.shield_sec,
        leg_sec: p.leg_sec,
        body_sec: p.body_sec,
        pylon_sec: p.pylon_sec,
        phases: p.phases,
        players: p.players,
        complete: p.complete,
        bugged: p.bugged,
        aborted: p.aborted,
        host_migration: p.host_migration,
        flight_unreliable: p.flight_unreliable,
        has_log: true,
        log_size_bytes: blob.len() as f64,
        end_reason: f.reason,
        source,
        tags: vec![],
        notes: None,
        duplicate_of: None,
    };
    let row = RunRow {
        id: id.clone(),
        kind: KIND.into(),
        started_at: f.started_at_ms as i64,
        ended_at: ended_at as i64,
        end_reason: tag(&record.end_reason),
        source: tag(&source),
        summary: serde_json::to_string(&record).unwrap_or_default(),
        notes: None,
        tags: vec![],
        duplicate_of: None,
        log_size: blob.len() as i64,
    };
    if !store::insert(conn, &row, Some(&blob))? {
        return Ok(None);
    }
    tracing::info!(
        "[PT] run saved: {:.3}s, {} phase(s), complete={}, bugged={}, end={}",
        record.duration_sec,
        record.phases.len(),
        record.complete,
        record.bugged,
        row.end_reason
    );
    refresh_duplicates(conn)?;
    Ok(get(conn, &id))
}

/// Recomputes every `duplicate_of` link from scratch. There is one orb, so start time and
/// duration alone identify a run; a demoted copy hands its notes and tags to the winner.
pub fn refresh_duplicates(conn: &Connection) -> rusqlite::Result<()> {
    let recs = list(conn)?;
    let dups: Vec<Dup> = recs
        .iter()
        .map(|r| Dup {
            started_at: r.started_at,
            duration_sec: r.duration_sec,
            live: r.source == RunSource::Live,
            log_size: r.log_size_bytes,
            richness: 0,
            target: String::new(),
        })
        .collect();
    let mut after = recs.clone();
    for (k, target) in duplicate_targets(&dups).into_iter().enumerate() {
        after[k].duplicate_of = target.map(|t| recs[t].id.clone());
        if let Some(t) = target {
            let (notes, tags) = (recs[k].notes.clone(), recs[k].tags.clone());
            let w = &mut after[t];
            if w.notes.is_none() {
                w.notes = notes;
            }
            if w.tags.is_empty() {
                w.tags = tags;
            }
        }
    }
    for (b, a) in recs.iter().zip(&after) {
        if b == a {
            continue;
        }
        if let Some(mut row) = store::get(conn, &a.id)? {
            row.summary = serde_json::to_string(a).unwrap_or_default();
            row.notes = a.notes.clone();
            row.tags = a.tags.clone();
            row.duplicate_of = a.duplicate_of.clone();
            store::update(conn, &row)?;
        }
    }
    Ok(())
}

/// Adapts the Profit Taker parser to the shared tracker and importer.
#[derive(Default)]
pub struct PtSeg(ProfitTakerParser);

impl SegmentParser for PtSeg {
    type Run = PtParsedRun;
    type Reason = PtRecordEndReason;

    fn feed(&mut self, line: &str) -> Option<SegEvent<PtRecordEndReason>> {
        Some(match self.0.feed_line(line)? {
            PtParserEvent::RunStart { game_time_sec } => SegEvent::Start(game_time_sec),
            PtParserEvent::RunEnd { reason } => SegEvent::End(reason.into()),
        })
    }
    fn is_active(&self) -> bool {
        self.0.is_run_active()
    }
    fn activity_count(&self) -> u32 {
        self.0.activity_count()
    }
    fn finalize(&mut self) -> Option<PtParsedRun> {
        self.0.finalize()
    }
    fn reset(&mut self) {
        self.0.reset();
    }
    fn run_start_sec(run: &PtParsedRun) -> f64 {
        run.run_start_sec
    }
    fn truncated() -> PtRecordEndReason {
        PtRecordEndReason::LogTruncated
    }
    fn inactive() -> PtRecordEndReason {
        PtRecordEndReason::Inactivity
    }
    fn imported() -> PtRecordEndReason {
        PtRecordEndReason::Imported
    }
}

pub type PtTracker = RunTracker<PtSeg>;

/// Splits an external EE.log into runs and saves each one. Already-stored runs and runs that fail
/// the save gate count as skipped.
pub fn import_log(
    path: &Path,
    db: &Mutex<Connection>,
    max_segment_bytes: usize,
) -> Result<PtImportResult, String> {
    let (imported, skipped) = segment::import_segments::<PtSeg, _>(path, max_segment_bytes, |f| {
        save(&mut db.lock().unwrap(), f, RunSource::Imported).map_err(|e| e.to_string())
    })?;
    Ok(PtImportResult { imported, skipped })
}

#[cfg(test)]
mod tests;
