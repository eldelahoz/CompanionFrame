use crate::db::mem_conn;
use std::sync::{Arc, Mutex};

use super::*;
use crate::services::eelog::Source;
use crate::services::runs::segment::MAX_SEGMENT_BYTES;
use crate::services::trackers::{Line, LineTracker};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/eelog/pt/");

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{DIR}{name}")).unwrap()
}

struct Rig {
    db: Arc<Mutex<Connection>>,
    saved: Arc<Mutex<Vec<PtRunRecord>>>,
    tracker: PtTracker,
}

impl Rig {
    fn new() -> Self {
        let db = Arc::new(Mutex::new(mem_conn()));
        let saved = Arc::new(Mutex::new(Vec::new()));
        let (d, s) = (db.clone(), saved.clone());
        let tracker = PtTracker::new(Box::new(move |f| {
            if let Some(r) = save(&mut d.lock().unwrap(), f, RunSource::Live).unwrap() {
                s.lock().unwrap().push(r);
            }
        }));
        Self { db, saved, tracker }
    }

    fn feed_from(&mut self, source: Source, lines: &[String]) {
        for text in lines {
            self.tracker.on_line(&Line {
                text,
                source,
                now_ms: 1_780_000_000_000.0,
                staleness_ms: 0.0,
                realtime_active: false,
            });
        }
    }

    fn feed(&mut self, lines: &[String]) {
        self.feed_from(Source::File, lines);
    }

    fn only(&self) -> PtRunRecord {
        let s = self.saved.lock().unwrap();
        assert_eq!(s.len(), 1, "expected one saved run");
        s[0].clone()
    }
}

fn fixture_lines(name: &str) -> Vec<String> {
    fixture(name).lines().map(String::from).collect()
}

fn single() -> Vec<String> {
    fixture_lines("host-single-run.log")
}

fn session(t: f64) -> String {
    format!(
        "{t:.3} Script [Info]: EidolonMP.lua: Session map string: /Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyFour"
    )
}
fn phase(t: f64, which: &str) -> String {
    format!(
        "{t:.3} Script [Info]: CamperHeistOrbFight.lua: Orb Fight - Starting {which} attack Orb phase"
    )
}

#[test]
fn saves_a_completed_host_run_with_its_capture() {
    let mut rig = Rig::new();
    rig.feed(&single());
    let run = rig.only();
    assert!(run.complete);
    assert!((run.duration_sec - 112.971).abs() < 0.01);
    assert_eq!(run.phases.len(), 4);
    assert_eq!(run.end_reason, PtRecordEndReason::Completed);
    assert_eq!(run.source, RunSource::Live);
    assert!(run.has_log && run.log_size_bytes > 0.0);

    let db = rig.db.lock().unwrap();
    let text = store::gunzip(&store::log_gz(&db, &run.id).unwrap()).unwrap();
    assert!(text.contains("Orb Fight - Starting final attack Orb phase"));
    // The record survives a reload from the table.
    assert_eq!(list(&db).unwrap(), vec![run]);
}

#[test]
fn a_run_without_player_telemetry_has_no_players() {
    let mut rig = Rig::new();
    let lines: Vec<_> = single()
        .into_iter()
        .filter(|l| !l.contains("loadout loader finished"))
        .collect();
    rig.feed(&lines);
    let run = rig.only();
    assert!(run.complete);
    assert!(run.players.is_empty());
}

#[test]
fn retains_an_observed_player() {
    let mut rig = Rig::new();
    let lines: Vec<_> = single()
        .into_iter()
        .filter(|l| !l.contains("ClientOne"))
        .collect();
    rig.feed(&lines);
    assert_eq!(rig.only().players, vec!["HostPlayer".to_string()]);
}

#[test]
fn ignores_the_dbwin_stream() {
    let mut rig = Rig::new();
    rig.feed_from(Source::Dbwin, &single());
    assert!(rig.saved.lock().unwrap().is_empty());
}

#[test]
fn does_not_save_a_client_side_run() {
    let mut rig = Rig::new();
    rig.feed(&[
        session(10.0),
        phase(20.0, "first"),
        phase(30.0, "second"),
        "40.000 Sys [Info]: EOM missionLocationUnlocked=1".into(),
    ]);
    assert!(list(&rig.db.lock().unwrap()).unwrap().is_empty());
}

#[test]
fn drops_a_capture_armed_but_never_entered() {
    let mut rig = Rig::new();
    rig.feed(&[
        "5.000 Script [Info]: ThemedSquadOverlay.lua: Active jobId set to /Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyFour_-6".into(),
        "9.000 Game [Info]: HostPlayer loadout loader finished.".into(),
        "20.000 Script [Info]: EidolonMP.lua: EIDOLONMP: TryTownTransition(1)".into(),
    ]);
    assert!(list(&rig.db.lock().unwrap()).unwrap().is_empty());
}

#[test]
fn finalizes_a_cut_run_on_reset() {
    let mut rig = Rig::new();
    let lines = single();
    rig.feed(&lines[..lines.len() - 12]);
    rig.tracker.on_reset();
    let s = rig.saved.lock().unwrap();
    assert!(s.len() <= 1);
    if let Some(r) = s.first() {
        assert_eq!(r.end_reason, PtRecordEndReason::LogTruncated);
    }
}

#[test]
fn annotations_log_delete_and_record_delete() {
    let mut rig = Rig::new();
    rig.feed(&single());
    let id = rig.only().id;
    let mut db = rig.db.lock().unwrap();
    store::set_tags(&db, &id, &["  Sub 2  ".into(), "sub 2".into()]).unwrap();
    store::set_notes(&db, &id, "  bad pylons  ").unwrap();
    let r = get(&db, &id).unwrap();
    assert_eq!(
        (r.tags, r.notes.as_deref()),
        (vec!["Sub 2".into()], Some("bad pylons"))
    );

    assert!(store::delete_log(&db, &id).unwrap());
    let r = get(&db, &id).unwrap();
    assert!(!r.has_log);
    assert_eq!(payload(&db).unwrap().disk_usage_bytes, 0.0);
    assert!(store::delete(&mut db, &id).unwrap());
    assert!(get(&db, &id).is_none());
}

#[test]
fn links_an_imported_copy_of_a_live_run_as_a_duplicate() {
    let mut rig = Rig::new();
    rig.feed(&single());
    let live = rig.only();

    let mut p = ProfitTakerParser::new();
    for l in single() {
        p.feed_line(&l);
    }
    let parsed = p.finalize().unwrap();
    // One minute apart, same duration: inside the live-vs-imported window.
    let copy = save(
        &mut rig.db.lock().unwrap(),
        Finished {
            parsed,
            started_at_ms: live.started_at + 60_000.0,
            reason: PtRecordEndReason::Imported,
            lines: single(),
        },
        RunSource::Imported,
    )
    .unwrap()
    .unwrap();
    let runs = list(&rig.db.lock().unwrap()).unwrap();
    assert_eq!(runs.len(), 2);
    let dups: Vec<_> = runs.iter().filter(|r| r.duplicate_of.is_some()).collect();
    assert_eq!(dups.len(), 1);
    assert_ne!(dups[0].duplicate_of.as_deref(), Some(dups[0].id.as_str()));
    assert!(runs.iter().any(|r| r.id == copy.id));
}

fn tmp(name: &str, content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cf-pt-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

fn import(db: &Mutex<Connection>, name: &str) -> PtImportResult {
    import_log(&Path::new(DIR).join(name), db, MAX_SEGMENT_BYTES).unwrap()
}

#[test]
fn imports_every_run_out_of_a_shared_log() {
    let db = Mutex::new(mem_conn());
    let r = import(&db, "host-two-runs.log");
    assert_eq!((r.imported.len(), r.skipped), (2, 0));
    assert!(r.imported.iter().all(|x| x.source == RunSource::Imported));
    assert!(
        r.imported
            .iter()
            .all(|x| x.end_reason == PtRecordEndReason::Completed)
    );
    assert!((r.imported[0].duration_sec - 112.971).abs() < 0.01);
    assert!((r.imported[1].duration_sec - 100.618).abs() < 0.01);
    assert_eq!(r.imported[1].players, vec!["HostPlayer".to_string()]);
    assert_eq!(list(&db.lock().unwrap()).unwrap().len(), 2);
}

#[test]
fn anchors_the_wall_clock_on_the_log_header() {
    let db = Mutex::new(mem_conn());
    let r = import(&db, "host-single-run.log");
    assert_eq!(r.imported.len(), 1);
    // Header: game time 0.5 = 2026-03-01 11:00:00 UTC, so the run starts 5316.394s in.
    let expected = 1_772_362_800_000.0 - 500.0 + 5316.394 * 1000.0;
    let run = &r.imported[0];
    assert!((run.started_at - expected).abs() < 10.0);
    assert!((run.ended_at - run.started_at - 112_971.0).abs() < 10.0);
}

#[test]
fn skips_a_run_it_already_imported() {
    let db = Mutex::new(mem_conn());
    assert_eq!(import(&db, "host-single-run.log").imported.len(), 1);
    let again = import(&db, "host-single-run.log");
    assert_eq!((again.imported.len(), again.skipped), (0, 1));
    assert_eq!(list(&db.lock().unwrap()).unwrap().len(), 1);
}

#[test]
fn imports_nothing_from_a_log_without_runs_and_errors_on_a_missing_file() {
    let db = Mutex::new(mem_conn());
    let p = tmp(
        "other.log",
        "1.000 Sys [Info]: nothing here\n2.000 Sys [Info]: still nothing",
    );
    let r = import_log(&p, &db, MAX_SEGMENT_BYTES).unwrap();
    assert_eq!((r.imported.len(), r.skipped), (0, 0));
    assert!(import_log(&p.with_file_name("missing.log"), &db, MAX_SEGMENT_BYTES).is_err());
}
