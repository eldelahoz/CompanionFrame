use crate::db::mem_conn;
use std::sync::{Arc, Mutex};

use super::*;
use crate::db;
use crate::services::eelog::Source;
use crate::services::runs::segment::{MAX_SEGMENT_BYTES, format_id};
use crate::services::trackers::{Line, LineTracker};
use std::time::UNIX_EPOCH;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/eelog/arbi/");
const CASTA: &str = "Arbitration: Casta Defense (Ceres)";
const BEREHYNIA: &str = "Arbitration: Berehynia Interception (Sedna)";

fn mission(ts: f64, name: &str) -> String {
    format!("{ts:.3} Script [Info]: ThemedSquadOverlay.lua: Mission name: {name}")
}
fn drone(ts: f64) -> String {
    format!("{ts:.3} Sys [Info]: OnAgentCreated /Npc/CorpusEliteShieldDroneAgent7")
}
fn reward(ts: f64) -> String {
    format!("{ts:.3} Sys [Info]: Created /Lotus/Interface/DefenseReward.swf")
}
fn eom(ts: f64) -> String {
    format!("{ts:.3} Sys [Info]: EOM missionLocationUnlocked=1")
}

/// A tracker whose finished runs land in an in-memory database, like the app's sink.
struct Rig {
    db: Arc<Mutex<Connection>>,
    saved: Arc<Mutex<Vec<ArbiRunRecord>>>,
    tracker: ArbiTracker,
    now_ms: f64,
}

impl Rig {
    fn new() -> Self {
        let db = Arc::new(Mutex::new(mem_conn()));
        let saved = Arc::new(Mutex::new(Vec::new()));
        let (d, s) = (db.clone(), saved.clone());
        let tracker = ArbiTracker::new(Box::new(move |f| {
            let rec = save(&mut d.lock().unwrap(), f, RunSource::Live).unwrap();
            if let Some(r) = rec {
                s.lock().unwrap().push(r);
            }
        }));
        Self {
            db,
            saved,
            tracker,
            now_ms: 1_780_000_000_000.0,
        }
    }

    fn feed_from(&mut self, source: Source, text: &str) {
        self.tracker.on_line(&Line {
            text,
            source,
            now_ms: self.now_ms,
            staleness_ms: 0.0,
            realtime_active: false,
        });
    }

    fn feed(&mut self, text: &str) {
        self.feed_from(Source::File, text);
    }

    fn feed_all(&mut self, lines: &[String]) {
        for l in lines {
            self.feed(l);
        }
    }

    /// Two rotations plus host AI lines, so the run clears the save gate.
    fn feed_run(&mut self) {
        self.feed_all(&[
            mission(100.0, CASTA),
            drone(150.0),
            drone(200.0),
            reward(400.0),
            reward(500.0),
        ]);
    }

    fn runs(&self) -> Vec<ArbiRunRecord> {
        list(&self.db.lock().unwrap()).unwrap()
    }

    fn only(&self) -> ArbiRunRecord {
        let s = self.saved.lock().unwrap();
        assert_eq!(s.len(), 1, "expected one saved run");
        s[0].clone()
    }
}

#[test]
fn records_a_run_and_its_log_on_new_mission() {
    let mut rig = Rig::new();
    rig.feed_run();
    rig.feed(&mission(900.0, "Cetus (Earth)"));
    let run = rig.only();

    assert_eq!(run.node, "Casta Defense (Ceres)");
    assert_eq!(run.mission_type, ArbiMissionType::Defense);
    assert_eq!((run.drones, run.rotations), (2, 2));
    assert_eq!(run.end_reason, ArbiRunEndReason::NewMission);
    assert_eq!(run.source, RunSource::Live);
    assert!(run.has_log && run.log_size_bytes > 0.0);

    let db = rig.db.lock().unwrap();
    let text = store::gunzip(&store::log_gz(&db, &run.id).unwrap()).unwrap();
    assert!(text.contains("Mission name: Arbitration: Casta Defense (Ceres)"));
    assert!(text.contains("DefenseReward.swf"));
    assert!(!text.contains("Cetus (Earth)"));
    assert_eq!(
        payload(&db).unwrap().disk_usage_bytes,
        run.log_size_bytes,
        "disk usage is the stored capture size"
    );
}

#[test]
fn ignores_the_dbwin_stream() {
    let mut rig = Rig::new();
    rig.feed_from(Source::Dbwin, &mission(100.0, CASTA));
    rig.feed_from(Source::Dbwin, &drone(150.0));
    rig.tracker.on_reset();
    assert!(rig.runs().is_empty());
}

#[test]
fn finalizes_as_aborted_on_a_confirmed_abort_line() {
    let mut rig = Rig::new();
    rig.feed_run();
    rig.feed("500.000 Script [Info]: TopMenu.lua: Abort: host/no session");
    let run = rig.only();
    assert_eq!(run.end_reason, ArbiRunEndReason::Aborted);
    assert_eq!(run.rotations, 2);
}

#[test]
fn finalizes_with_log_truncated_on_reset() {
    let mut rig = Rig::new();
    rig.feed_run();
    rig.tracker.on_reset();
    assert_eq!(rig.only().end_reason, ArbiRunEndReason::LogTruncated);
}

#[test]
fn saves_neither_a_client_side_run_nor_one_under_two_rotations() {
    let mut rig = Rig::new();
    // Client-side: no OnAgentCreated lines at all, however many rotations.
    rig.feed_all(&[
        mission(100.0, CASTA),
        reward(400.0),
        reward(500.0),
        reward(600.0),
        eom(700.0),
    ]);
    // Host-side but only one rotation.
    rig.feed_all(&[
        mission(1000.0, CASTA),
        drone(1050.0),
        reward(1400.0),
        eom(1500.0),
    ]);
    assert!(rig.runs().is_empty());
    assert!(rig.saved.lock().unwrap().is_empty());
}

#[test]
fn starts_a_back_to_back_run_from_the_ending_mission_line() {
    let mut rig = Rig::new();
    rig.feed_run();
    rig.feed(&mission(900.0, BEREHYNIA));
    let first = rig.only();
    assert_eq!(first.node, "Casta Defense (Ceres)");

    rig.feed_all(&[drone(950.0), reward(1000.0), reward(1100.0)]);
    rig.tracker.on_reset();
    let runs = rig.saved.lock().unwrap().clone();
    let second = &runs[1];
    assert_eq!(second.node, "Berehynia Interception (Sedna)");
    assert_eq!(second.mission_type, ArbiMissionType::Interception);
    assert_eq!(second.drones, 1);
    // Same wall-clock second must not collide.
    assert_ne!(second.id, first.id);
}

#[test]
fn records_two_runs_separated_by_a_mission_end() {
    let mut rig = Rig::new();
    rig.feed_run();
    rig.feed(&eom(500.0));
    assert_eq!(rig.only().end_reason, ArbiRunEndReason::MissionEnd);

    rig.feed_all(&[
        mission(900.0, BEREHYNIA),
        drone(950.0),
        reward(1100.0),
        reward(1150.0),
        eom(1200.0),
    ]);
    let runs = rig.runs();
    assert_eq!(runs.len(), 2);
    let casta = runs.iter().find(|r| r.node.starts_with("Casta")).unwrap();
    let db = rig.db.lock().unwrap();
    let text = store::gunzip(&store::log_gz(&db, &casta.id).unwrap()).unwrap();
    assert!(text.contains("Casta Defense") && !text.contains("Berehynia"));
}

#[test]
fn finalizes_on_inactivity_when_only_non_combat_lines_arrive() {
    let mut rig = Rig::new();
    rig.feed_run();
    for i in 0..11 {
        rig.now_ms += 60_000.0;
        rig.feed(&format!("{}.000 Sys [Info]: orbiter chatter", 900 + i));
        rig.tracker.on_tick(rig.now_ms);
    }
    assert_eq!(rig.only().end_reason, ArbiRunEndReason::Inactivity);
}

#[test]
fn stays_alive_while_combat_events_keep_arriving() {
    let mut rig = Rig::new();
    rig.feed_run();
    for i in 0..15 {
        rig.now_ms += 60_000.0;
        rig.feed(&drone(900.0 + f64::from(i) * 60.0));
        rig.tracker.on_tick(rig.now_ms);
    }
    assert!(rig.saved.lock().unwrap().is_empty());
}

#[test]
fn a_saved_run_survives_reopening_the_database() {
    let dir = std::env::temp_dir().join(format!("cf-runs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("runs.db");
    let _ = std::fs::remove_file(&path);

    let id = {
        let d = db::open(&path).unwrap();
        let run = save(
            &mut d.0.lock().unwrap(),
            Finished {
                parsed: parse_lines(&[
                    mission(100.0, CASTA),
                    drone(150.0),
                    drone(200.0),
                    reward(400.0),
                    reward(500.0),
                ]),
                started_at_ms: 1_780_000_000_000.0,
                reason: ArbiRunEndReason::MissionEnd,
                lines: vec!["x".into()],
            },
            RunSource::Live,
        )
        .unwrap()
        .unwrap();
        run.id
    };
    let reopened = db::open(&path).unwrap();
    let got = get(&reopened.0.lock().unwrap(), &id).unwrap();
    assert_eq!((got.rotations, got.drones), (2, 2));
    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

fn parse_lines(lines: &[String]) -> ArbiParsedRun {
    let mut p = ArbiParser::new();
    for l in lines {
        p.feed_line(l);
    }
    p.finalize().unwrap()
}

fn stub(id: &str, source: RunSource, start_offset_ms: f64) -> ArbiRunRecord {
    let started_at = 1_780_000_000_000.0 + start_offset_ms;
    ArbiRunRecord {
        id: id.into(),
        started_at,
        ended_at: started_at + 600_000.0,
        mission_name: CASTA.into(),
        node: "Casta Defense (Ceres)".into(),
        mission_type: ArbiMissionType::Defense,
        mission_type_raw: None,
        sol_node: None,
        duration_sec: 600.0,
        rotations: 3,
        drones: 12,
        total_enemies: 240,
        vitus_actual: None,
        has_log: false,
        log_size_bytes: 0.0,
        end_reason: ArbiRunEndReason::MissionEnd,
        source,
        stats: None,
        tags: vec![],
        players: vec![],
        notes: None,
        duplicate_of: None,
    }
}

fn put(conn: &mut Connection, rec: &ArbiRunRecord) {
    let row = RunRow {
        id: rec.id.clone(),
        kind: KIND.into(),
        started_at: rec.started_at as i64,
        ended_at: rec.ended_at as i64,
        end_reason: tag(&rec.end_reason),
        source: tag(&rec.source),
        summary: serde_json::to_string(rec).unwrap(),
        notes: rec.notes.clone(),
        tags: rec.tags.clone(),
        duplicate_of: None,
        log_size: rec.log_size_bytes as i64,
    };
    store::insert(conn, &row, None).unwrap();
}

fn stats() -> ArbiRunStats {
    ArbiRunStats {
        kills_per_drone: 20.0,
        avg_drone_interval_sec: Some(30.0),
        expected_vitus_mean: 10.0,
        expected_vitus_std: 3.0,
        vitus_per_min: 1.0,
        waves_per_rotation: 3,
        drone_timestamps: vec![],
        reward_timestamps: vec![],
        precise_start_sec: Some(0.0),
        last_activity_sec: 600.0,
        saturation_buckets: vec![],
        waves: None,
        pause_intervals: vec![],
        idle_intervals: vec![],
        rotation_saturation_pct: vec![],
        spawn_points: vec![],
        spawn_data_version: 2,
    }
}

#[test]
fn marks_the_poorer_of_a_live_imported_pair_and_frees_it_on_delete() {
    let mut c = mem_conn();
    let mut live = stub("2026-07-08_00-15-00", RunSource::Live, 0.0);
    live.stats = Some(stats());
    let mut imported = stub("2026-07-08_00-16-00", RunSource::Imported, 60_000.0);
    imported.duration_sec = 602.0;
    // Same node but three hours later: a separate run, not a duplicate.
    let later = stub(
        "2026-07-08_03-15-00",
        RunSource::Imported,
        3.0 * 3_600_000.0,
    );
    for r in [&live, &imported, &later] {
        put(&mut c, r);
    }

    refresh_duplicates(&c).unwrap();
    assert_eq!(get(&c, &live.id).unwrap().duplicate_of, None);
    assert_eq!(
        get(&c, &imported.id).unwrap().duplicate_of.as_deref(),
        Some(live.id.as_str())
    );
    assert_eq!(get(&c, &later.id).unwrap().duplicate_of, None);

    store::delete(&mut c, &live.id).unwrap();
    refresh_duplicates(&c).unwrap();
    assert_eq!(get(&c, &imported.id).unwrap().duplicate_of, None);
}

#[test]
fn a_demoted_copy_hands_its_annotations_to_the_winner() {
    let mut c = mem_conn();
    // Neither has stats, so the longer raw log wins and the annotated live copy is demoted.
    let mut a = stub("2026-07-08_00-15-00", RunSource::Live, 0.0);
    (a.notes, a.tags, a.vitus_actual) =
        (Some("boar build".into()), vec!["boar".into()], Some(1044.0));
    let mut b = stub("2026-07-08_00-16-00", RunSource::Imported, 60_000.0);
    b.log_size_bytes = 500.0;
    // A winner with its own note keeps it and only fills what it lacks.
    let mut c1 = stub("2026-07-08_03-15-00", RunSource::Live, 3.0 * 3_600_000.0);
    (c1.notes, c1.vitus_actual) = (Some("mine".into()), Some(10.0));
    let mut c2 = stub(
        "2026-07-08_03-16-00",
        RunSource::Imported,
        3.0 * 3_600_000.0 + 60_000.0,
    );
    (c2.log_size_bytes, c2.notes) = (500.0, Some("theirs".into()));
    for r in [&a, &b, &c1, &c2] {
        put(&mut c, r);
    }

    refresh_duplicates(&c).unwrap();
    assert_eq!(
        get(&c, &a.id).unwrap().duplicate_of.as_deref(),
        Some(b.id.as_str())
    );
    let w = get(&c, &b.id).unwrap();
    assert_eq!(w.notes.as_deref(), Some("boar build"));
    assert_eq!(w.tags, vec!["boar".to_string()]);
    assert_eq!(w.vitus_actual, Some(1044.0));
    let w2 = get(&c, &c2.id).unwrap();
    assert_eq!(
        (w2.notes.as_deref(), w2.vitus_actual),
        (Some("theirs"), Some(10.0))
    );
}

#[test]
fn vitus_is_rounded_cleared_and_range_checked() {
    let mut c = mem_conn();
    put(&mut c, &stub("2026-07-08_00-15-00", RunSource::Live, 0.0));
    let id = "2026-07-08_00-15-00";
    assert_eq!(
        set_vitus(&c, id, Some(1043.6)).unwrap().vitus_actual,
        Some(1044.0)
    );
    assert_eq!(get(&c, id).unwrap().vitus_actual, Some(1044.0));
    assert!(set_vitus(&c, id, Some(-1.0)).is_none());
    assert!(set_vitus(&c, id, Some(f64::NAN)).is_none());
    assert!(set_vitus(&c, id, Some(10_000_001.0)).is_none());
    assert_eq!(set_vitus(&c, id, None).unwrap().vitus_actual, None);
    assert!(set_vitus(&c, "nope", Some(1.0)).is_none());
}

// ---- importer ----

// Anchor: game time 0 = 2026-03-01 11:00:00 UTC (header line at ts 0.5).
const HEADER: &str =
    "0.500 Sys [Diag]: Current time: Sun Mar 01 12:00:00 2026 [UTC: Sun Mar 01 11:00:00 2026]";

fn multi_run_log() -> String {
    [
        HEADER.to_string(),
        "5.000 Sys [Info]: Some unrelated line".into(),
        mission(100.0, CASTA),
        drone(150.0),
        drone(200.0),
        // Two rotations each: one-rotation runs are not stored.
        reward(400.0),
        reward(500.0),
        mission(900.0, "Cetus (Earth)"),
        "950.000 Sys [Info]: idle in town".into(),
        mission(1000.0, BEREHYNIA),
        drone(1100.0),
        drone(1160.0),
        reward(1400.0),
        reward(1500.0),
        // file ends while the second run is active
    ]
    .join("\n")
}

fn write_tmp(name: &str, content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("cf-arbi-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, content).unwrap();
    p
}

fn import(db: &Mutex<Connection>, p: &Path) -> ArbiImportResult {
    import_log(p, db, MAX_SEGMENT_BYTES).unwrap()
}

fn fresh_db() -> Mutex<Connection> {
    Mutex::new(mem_conn())
}

#[test]
fn imports_every_run_with_wall_clock_anchoring() {
    let db = fresh_db();
    let r = import(&db, &write_tmp("multi.log", &multi_run_log()));
    assert_eq!((r.imported.len(), r.skipped), (2, 0));
    let (first, second) = (&r.imported[0], &r.imported[1]);

    assert_eq!(first.node, "Casta Defense (Ceres)");
    assert_eq!(first.mission_type, ArbiMissionType::Defense);
    assert_eq!(first.drones, 2);
    assert_eq!(first.source, RunSource::Imported);
    // The first run's end was observed (next mission line); the reason carries over.
    assert_eq!(first.end_reason, ArbiRunEndReason::NewMission);
    // UTC stamp: 11:00:00Z at ts 0.5 -> run start at ts 100 = 11:01:39.5Z
    let anchored = 1_772_362_800_000.0 - 500.0 + 100_000.0;
    assert_eq!(first.started_at, anchored);
    assert_eq!(first.id, format_id(anchored));

    assert_eq!(second.node, "Berehynia Interception (Sedna)");
    assert_eq!(second.mission_type, ArbiMissionType::Interception);
    assert_eq!(second.rotations, 2);
    // File ended mid-run: no observed end.
    assert_eq!(second.end_reason, ArbiRunEndReason::Imported);
    assert_eq!(list(&db.lock().unwrap()).unwrap().len(), 2);
}

#[test]
fn reimporting_the_same_log_does_not_duplicate() {
    let db = fresh_db();
    let p = write_tmp("again.log", &multi_run_log());
    assert_eq!(import(&db, &p).imported.len(), 2);
    let again = import(&db, &p);
    assert_eq!((again.imported.len(), again.skipped), (0, 2));
    assert_eq!(list(&db.lock().unwrap()).unwrap().len(), 2);
}

#[test]
fn a_log_without_arbitrations_imports_nothing() {
    let db = fresh_db();
    let log = [
        HEADER.to_string(),
        mission(100.0, "Cetus (Earth)"),
        drone(150.0),
    ]
    .join("\n");
    let r = import(&db, &write_tmp("none.log", &log));
    assert_eq!((r.imported.len(), r.skipped), (0, 0));
}

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(FIXTURES).join(name)
}

fn parse_fixture(name: &str) -> ArbiParsedRun {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    let mut p = ArbiParser::new();
    for l in text.lines() {
        p.feed_line(l);
    }
    p.finalize().unwrap()
}

#[test]
fn skips_a_real_aborted_run_below_the_rotation_floor() {
    let db = fresh_db();
    let r = import(&db, &fixture("oestrus-abort-ee.log"));
    assert_eq!((r.imported.len(), r.skipped), (0, 1));
    assert!(list(&db.lock().unwrap()).unwrap().is_empty());

    let run = parse_fixture("oestrus-abort-ee.log");
    assert_eq!(run.node, "Oestrus (Eris)");
    assert_eq!(run.mission_type, ArbiMissionType::Other);
    assert_eq!(run.mission_type_raw.as_deref(), Some("MT_PURIFY"));
    assert_eq!(run.sol_node.as_deref(), Some("SolNode167"));
    assert_eq!((run.rotations, run.drones, run.total_enemies), (0, 0, 0));
    assert!(run.stats.is_none());
}

#[test]
fn skips_a_real_one_rotation_survival_but_parses_past_the_end_of_match_screens() {
    let db = fresh_db();
    let r = import(&db, &fixture("mot-survival-ee.log"));
    assert_eq!((r.imported.len(), r.skipped), (0, 1));

    let run = parse_fixture("mot-survival-ee.log");
    assert_eq!(run.node, "Mot (Void)");
    assert_eq!(run.mission_type_raw.as_deref(), Some("MT_SURVIVAL"));
    assert_eq!(run.sol_node.as_deref(), Some("SolNode409"));
    assert_eq!((run.rotations, run.drones), (1, 7));
    // first drone 434.607 -> last drone 735.957
    assert!((run.duration_sec - 301.35).abs() < 0.01);
}

#[test]
fn skips_a_real_one_rotation_interception() {
    let db = fresh_db();
    let r = import(&db, &fixture("rhea-interception-ee.log"));
    assert_eq!((r.imported.len(), r.skipped), (0, 1));

    let run = parse_fixture("rhea-interception-ee.log");
    assert_eq!(run.node, "Rhea (Saturn)");
    assert_eq!(run.mission_type, ArbiMissionType::Interception);
    assert_eq!(run.mission_type_raw.as_deref(), Some("MT_TERRITORY"));
    assert_eq!((run.rotations, run.drones), (1, 5));
    // The rotation anchors on the DefenseReward, not the stray SurvivalReward.
    let s = run.stats.unwrap();
    assert_eq!(s.reward_timestamps, vec![356.94]);
    assert_eq!(s.precise_start_sec, Some(133.194));
    assert!((run.duration_sec - 223.746).abs() < 0.001);
}

#[test]
fn never_imports_a_client_side_run_however_long() {
    let db = fresh_db();
    // Client logs carry no OnAgentCreated lines: the host simulates the AI.
    let log = [
        HEADER.to_string(),
        mission(100.0, CASTA),
        reward(400.0),
        reward(500.0),
        reward(600.0),
        reward(700.0),
    ]
    .join("\n");
    let r = import(&db, &write_tmp("client.log", &log));
    assert_eq!((r.imported.len(), r.skipped), (0, 1));
}

#[test]
fn imports_a_real_defense_run_with_wave_map_and_foreign_timezone_anchor() {
    let db = fresh_db();
    let r = import(&db, &fixture("stoefler-defense-ee.log"));
    assert_eq!(r.imported.len(), 1);
    let run = &r.imported[0];
    assert_eq!(run.node, "Stöfler (Lua)");
    assert_eq!(run.mission_type, ArbiMissionType::Defense);
    assert_eq!(run.mission_type_raw.as_deref(), Some("MT_DEFENSE"));
    assert_eq!(run.sol_node.as_deref(), Some("SolNode305"));
    assert_eq!(run.end_reason, ArbiRunEndReason::MissionEnd);
    assert_eq!((run.rotations, run.drones), (2, 3));
    // 2x ShieldLancer + 2x ShotgunLancer + 3 drones; the kubrow also counts, since a single
    // spawn can never be proven non-ticking (reference-site parity).
    assert_eq!(run.total_enemies, 8);

    let s = run.stats.as_ref().unwrap();
    assert_eq!(s.waves_per_rotation, 3);
    assert_eq!(s.reward_timestamps, vec![16855.934, 16936.816]);
    // "Defense wave: 1" is the precise start.
    assert_eq!(s.precise_start_sec, Some(16786.896));
    assert!((run.duration_sec - (16936.816 - 16786.896)).abs() < 0.001);

    // Wave durations: sleep-marker windows, every 3rd wave via countdown-5.
    let waves = s.waves.as_ref().unwrap();
    assert_eq!(
        waves.iter().map(|w| w.index).collect::<Vec<_>>(),
        (1..=6).collect::<Vec<_>>()
    );
    let near = |got: f64, want: f64| assert!((got - want).abs() < 0.001, "{got} vs {want}");
    near(waves[0].duration_sec, 16.151);
    near(waves[1].duration_sec, 22.873);
    near(waves[2].duration_sec, 16855.932 - 5.0 - 16831.973);
    // Wave 4 ends on a "!"-prefixed sleep line; the timestamp must still parse.
    near(waves[3].duration_sec, 19.101);
    near(waves[5].duration_sec, 16936.807 - 5.0 - 16910.236);

    // The header is from another machine (UTC-6): the anchor must use the UTC stamp.
    let started = 1_783_460_449_000.0 - 127.0 + 16_596_750.0;
    assert_eq!(run.started_at, started);
    assert_eq!(run.id, format_id(started));
}

#[test]
fn falls_back_to_the_file_mtime_when_the_header_is_missing() {
    let db = fresh_db();
    let log = [
        mission(100.0, CASTA),
        drone(150.0),
        reward(120.0),
        reward(151.0),
    ]
    .join("\n");
    let p = write_tmp("nohdr.log", &log);
    let mtime = std::fs::metadata(&p)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64;
    let r = import(&db, &p);
    assert_eq!(r.imported.len(), 1);
    // last line ts 151, run start ts 100 -> mtime - 51s
    assert!((r.imported[0].started_at - (mtime - 51_000.0)).abs() < 1.0);
}

#[test]
fn the_raw_capture_stops_at_the_byte_cap() {
    let lines = [
        mission(100.0, CASTA),
        drone(150.0),
        format!("200.000 Sys [Info]: {}", "é".repeat(12)),
        reward(400.0),
        reward(500.0),
    ];
    let log = lines.join("\n");
    let bytes: usize = lines.iter().map(|l| l.len() + 1).sum();
    for (cap, includes_last) in [(bytes, true), (bytes - 1, false)] {
        let db = fresh_db();
        let r = import_log(&write_tmp("cap.log", &log), &db, cap).unwrap();
        assert_eq!(r.imported.len(), 1);
        let conn = db.lock().unwrap();
        let raw = store::gunzip(&store::log_gz(&conn, &r.imported[0].id).unwrap()).unwrap();
        assert_eq!(raw.contains(lines.last().unwrap()), includes_last);
        assert!(raw.len() <= cap);
    }
}

#[test]
fn an_unreadable_file_is_an_error_and_stores_nothing() {
    let db = fresh_db();
    let missing = std::env::temp_dir().join("cf-arbi-missing-nowhere.log");
    assert!(import_log(&missing, &db, MAX_SEGMENT_BYTES).is_err());
    assert!(list(&db.lock().unwrap()).unwrap().is_empty());
}
