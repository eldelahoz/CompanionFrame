//! Arbitration runs: the live tracker (a `LineTracker` over `eelog_parsers::arbi`), the save path
//! shared with the EE.log importer, and cross-source duplicate detection. Port of WFHelper's
//! `arbiRunTracker`, `arbiLogImporter`, `eeLogSegmentImporter` and the generic `runTracker`.
use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::segment::{
    self, Dup, RunSource, RunTracker, SegEvent, SegmentParser, duplicate_targets, run_id, tag,
};
use super::store::{self, RunRow};
use crate::services::eelog_parsers::arbi::{
    ArbiMissionType, ArbiParsedRun, ArbiParser, ArbiParserEvent, ArbiRunStats, RunEndReason,
};

pub const KIND: &str = "arbi";
/// Same floor the summary overlay uses; shorter runs aren't worth an entry.
const MIN_SAVED_ROTATIONS: u32 = 2;
const MAX_VITUS: f64 = 10_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum ArbiRunEndReason {
    MissionEnd,
    Aborted,
    NewMission,
    LogTruncated,
    AppQuit,
    Inactivity,
    Imported,
}

impl From<RunEndReason> for ArbiRunEndReason {
    fn from(r: RunEndReason) -> Self {
        match r {
            RunEndReason::MissionEnd => Self::MissionEnd,
            RunEndReason::Aborted => Self::Aborted,
            RunEndReason::NewMission => Self::NewMission,
        }
    }
}

/// What the UI lists. Everything is stored in `runs.summary`; tags, notes and the duplicate link
/// live in their own columns and win on read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiRunRecord {
    /// "YYYY-MM-DD_HH-mm-ss" local wall clock at run start.
    pub id: String,
    /// Wall clock, ms since the epoch.
    pub started_at: f64,
    pub ended_at: f64,
    pub mission_name: String,
    pub node: String,
    pub mission_type: ArbiMissionType,
    pub mission_type_raw: Option<String>,
    pub sol_node: Option<String>,
    pub duration_sec: f64,
    pub rotations: u32,
    pub drones: u32,
    pub total_enemies: u32,
    pub vitus_actual: Option<f64>,
    /// False once the raw capture is deleted.
    #[serde(default)]
    pub has_log: bool,
    pub log_size_bytes: f64,
    pub end_reason: ArbiRunEndReason,
    pub source: RunSource,
    pub stats: Option<ArbiRunStats>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub players: Vec<String>,
    pub notes: Option<String>,
    /// Id of the richer record this one duplicates.
    pub duplicate_of: Option<String>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiRunsPayload {
    pub runs: Vec<ArbiRunRecord>,
    pub disk_usage_bytes: f64,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArbiImportResult {
    pub imported: Vec<ArbiRunRecord>,
    pub skipped: u32,
}

pub type Finished = segment::Finished<ArbiParsedRun, ArbiRunEndReason>;

fn skip_reason(p: &ArbiParsedRun) -> Option<String> {
    if !p.host_telemetry {
        Some("client-side run, no AI data in the log".into())
    } else if p.rotations < MIN_SAVED_ROTATIONS {
        Some(format!("{} rotation(s)", p.rotations))
    } else {
        None
    }
}

fn from_row(row: RunRow) -> Option<ArbiRunRecord> {
    let mut r: ArbiRunRecord = serde_json::from_str(&row.summary).ok()?;
    r.tags = row.tags;
    r.notes = row.notes;
    r.duplicate_of = row.duplicate_of;
    r.has_log = row.log_size > 0;
    r.log_size_bytes = row.log_size as f64;
    Some(r)
}

pub fn get(conn: &Connection, id: &str) -> Option<ArbiRunRecord> {
    store::get(conn, id)
        .ok()
        .flatten()
        .filter(|r| r.kind == KIND)
        .and_then(from_row)
}

pub fn list(conn: &Connection) -> rusqlite::Result<Vec<ArbiRunRecord>> {
    Ok(store::list(conn, KIND)?
        .into_iter()
        .filter_map(from_row)
        .collect())
}

pub fn payload(conn: &Connection) -> rusqlite::Result<ArbiRunsPayload> {
    Ok(ArbiRunsPayload {
        runs: list(conn)?,
        disk_usage_bytes: store::disk_usage(conn, KIND)? as f64,
    })
}

/// Persists a closed run. None when it is not worth keeping or, for an import, already stored.
pub fn save(
    conn: &mut Connection,
    f: Finished,
    source: RunSource,
) -> rusqlite::Result<Option<ArbiRunRecord>> {
    if let Some(why) = skip_reason(&f.parsed) {
        tracing::info!("[Arbi] run not saved ({why})");
        return Ok(None);
    }
    let Some(id) = run_id(conn, f.started_at_ms, source) else {
        return Ok(None);
    };
    let blob = store::gzip(&f.lines);
    let p = f.parsed;
    let end_sec = p.run_end_sec.unwrap_or(p.last_activity_sec);
    let ended_at = f.started_at_ms + ((end_sec - p.run_start_sec) * 1000.0).max(0.0);
    let record = ArbiRunRecord {
        id: id.clone(),
        started_at: f.started_at_ms,
        ended_at,
        mission_name: p.mission_name,
        node: p.node,
        mission_type: p.mission_type,
        mission_type_raw: p.mission_type_raw,
        sol_node: p.sol_node,
        duration_sec: p.duration_sec,
        rotations: p.rotations,
        drones: p.drones,
        total_enemies: p.total_enemies,
        vitus_actual: None,
        has_log: true,
        log_size_bytes: blob.len() as f64,
        end_reason: f.reason,
        source,
        stats: p.stats,
        tags: vec![],
        players: p.players,
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
        "[Arbi] run saved: {} ({:?}), {} rotations, {} drones, end={}",
        record.node,
        record.mission_type,
        record.rotations,
        record.drones,
        row.end_reason
    );
    refresh_duplicates(conn)?;
    Ok(get(conn, &id))
}

/// Recomputes every `duplicate_of` link from scratch; true when anything moved. A demoted
/// record's annotations move onto its winner unless the winner has its own.
pub fn mark_duplicates(recs: &mut [ArbiRunRecord]) -> bool {
    let dups: Vec<Dup> = recs
        .iter()
        .map(|r| Dup {
            started_at: r.started_at,
            duration_sec: r.duration_sec,
            live: r.source == RunSource::Live,
            log_size: r.log_size_bytes,
            // Full stats beat none.
            richness: u8::from(r.stats.is_some()),
            target: r.node.clone(),
        })
        .collect();
    let mut changed = false;
    for (k, target) in duplicate_targets(&dups).into_iter().enumerate() {
        let link = target.map(|t| recs[t].id.clone());
        if link == recs[k].duplicate_of {
            continue;
        }
        recs[k].duplicate_of = link;
        if let Some(t) = target {
            let (notes, tags, vitus) = (
                recs[k].notes.clone(),
                recs[k].tags.clone(),
                recs[k].vitus_actual,
            );
            let w = &mut recs[t];
            if w.notes.is_none() {
                w.notes = notes;
            }
            if w.tags.is_empty() {
                w.tags = tags;
            }
            if w.vitus_actual.is_none() {
                w.vitus_actual = vitus;
            }
        }
        changed = true;
    }
    changed
}

fn write_back(conn: &Connection, rec: &ArbiRunRecord) -> rusqlite::Result<()> {
    let Some(mut row) = store::get(conn, &rec.id)? else {
        return Ok(());
    };
    row.summary = serde_json::to_string(rec).unwrap_or_default();
    row.notes = rec.notes.clone();
    row.tags = rec.tags.clone();
    row.duplicate_of = rec.duplicate_of.clone();
    store::update(conn, &row)
}

pub fn refresh_duplicates(conn: &Connection) -> rusqlite::Result<()> {
    let before = list(conn)?;
    let mut after = before.clone();
    if mark_duplicates(&mut after) {
        for (b, a) in before.iter().zip(&after) {
            if b != a {
                write_back(conn, a)?;
            }
        }
    }
    Ok(())
}

/// Measured vitus for a run; None for an unknown id or an out-of-range value.
pub fn set_vitus(conn: &Connection, id: &str, vitus: Option<f64>) -> Option<ArbiRunRecord> {
    if vitus.is_some_and(|v| !v.is_finite() || !(0.0..=MAX_VITUS).contains(&v)) {
        return None;
    }
    let mut rec = get(conn, id)?;
    rec.vitus_actual = vitus.map(f64::round);
    write_back(conn, &rec).ok()?;
    Some(rec)
}

/// Adapts the arbitration parser to the shared tracker and importer.
#[derive(Default)]
pub struct ArbiSeg(ArbiParser);

impl SegmentParser for ArbiSeg {
    type Run = ArbiParsedRun;
    type Reason = ArbiRunEndReason;

    fn feed(&mut self, line: &str) -> Option<SegEvent<ArbiRunEndReason>> {
        Some(match self.0.feed_line(line)? {
            ArbiParserEvent::RunStart { game_time_sec, .. } => SegEvent::Start(game_time_sec),
            ArbiParserEvent::RunEnd { reason } => SegEvent::End(reason.into()),
        })
    }
    fn is_active(&self) -> bool {
        self.0.is_run_active()
    }
    fn activity_count(&self) -> u32 {
        self.0.activity_count()
    }
    fn finalize(&mut self) -> Option<ArbiParsedRun> {
        self.0.finalize()
    }
    fn reset(&mut self) {
        self.0.reset();
    }
    fn run_start_sec(run: &ArbiParsedRun) -> f64 {
        run.run_start_sec
    }
    fn truncated() -> ArbiRunEndReason {
        ArbiRunEndReason::LogTruncated
    }
    fn inactive() -> ArbiRunEndReason {
        ArbiRunEndReason::Inactivity
    }
    fn imported() -> ArbiRunEndReason {
        ArbiRunEndReason::Imported
    }
}

pub type ArbiTracker = RunTracker<ArbiSeg>;

/// Splits an external EE.log into runs and saves each one. Already-stored runs and runs below the
/// save floor count as skipped.
pub fn import_log(
    path: &Path,
    db: &Mutex<Connection>,
    max_segment_bytes: usize,
) -> Result<ArbiImportResult, String> {
    let (imported, skipped) =
        segment::import_segments::<ArbiSeg, _>(path, max_segment_bytes, |f| {
            save(&mut db.lock().unwrap(), f, RunSource::Imported).map_err(|e| e.to_string())
        })?;
    Ok(ArbiImportResult { imported, skipped })
}

#[cfg(test)]
mod tests;
