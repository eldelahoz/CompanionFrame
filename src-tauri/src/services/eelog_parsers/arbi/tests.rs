use super::*;

fn mission_line(ts: f64, name: &str) -> String {
    format!("{ts:.3} Script [Info]: ThemedSquadOverlay.lua: Mission name: {name}")
}
fn drone_line(ts: f64) -> String {
    format!("{ts:.3} Sys [Info]: OnAgentCreated /Npc/CorpusEliteShieldDroneAgent7")
}
fn enemy_line(ts: f64, name: &str, tick: u32) -> String {
    format!("{ts:.3} AI [Info]: OnAgentCreated /Npc/{name}3 pos MonitoredTicking {tick}")
}
fn reward_line(ts: f64) -> String {
    format!("{ts:.3} Sys [Info]: Created /Lotus/Interface/DefenseReward.swf")
}
fn tick_line(ts: f64, count: u32) -> String {
    format!("{ts:.3} AI [Info]: NpcManager status MonitoredTicking {count}")
}
fn wave_line(ts: f64, wave: u32) -> String {
    format!("{ts:.3} Script [Info]: WaveDefend.lua: Defense wave: {wave}")
}
fn wave_start_line(ts: f64, wave: u32) -> String {
    format!("{ts:.3} Script [Info]: WaveDefend.lua: Starting wave {wave} (32 simultaneous)")
}
fn sleep3_line(ts: f64) -> String {
    format!("{ts:.3} Script [Info]: WaveDefend.lua: _SleepBetweenWaves(3)")
}
fn countdown_line(ts: f64) -> String {
    format!("{ts:.3} Sys [Info]: Created /Lotus/Interface/ProjectionsCountdown.swf")
}
fn state_line(ts: f64, state: &str) -> String {
    format!("{ts:.3} Script [Info]: SentientArtifactMission.lua: Disruption: State change: {state}")
}
fn mt_line(ts: f64, mt: &str) -> String {
    format!("{ts:.3} Game [Info]: OnStateStarted, mission type={mt}")
}

fn feed(parser: &mut ArbiParser, lines: &[String]) {
    for l in lines {
        parser.feed_line(l);
    }
}

fn run_parser(lines: Vec<String>) -> Option<ArbiParsedRun> {
    let mut parser = ArbiParser::new();
    feed(&mut parser, &lines);
    parser.finalize()
}

fn start_type(event: Option<ArbiParserEvent>) -> ArbiMissionType {
    match event {
        Some(ArbiParserEvent::RunStart { mission_type, .. }) => mission_type,
        other => panic!("expected run-start, got {other:?}"),
    }
}

fn end(reason: RunEndReason) -> Option<ArbiParserEvent> {
    Some(ArbiParserEvent::RunEnd { reason })
}

const CASTA: &str = "Arbitration: Casta Defense (Ceres)";

#[test]
fn starts_a_run_on_an_arbitration_mission_name() {
    let mut p = ArbiParser::new();
    let event = p.feed_line(&mission_line(100.0, CASTA));
    assert_eq!(
        event,
        Some(ArbiParserEvent::RunStart {
            mission_name: CASTA.into(),
            node: "Casta Defense (Ceres)".into(),
            mission_type: ArbiMissionType::Defense,
            game_time_sec: 100.0,
        })
    );
    assert!(p.is_run_active());
}

#[test]
fn ignores_non_arbitration_missions_when_idle() {
    let mut p = ArbiParser::new();
    assert_eq!(p.feed_line(&mission_line(100.0, "Casta (Ceres)")), None);
    assert!(!p.is_run_active());
}

#[test]
fn classifies_interception_and_mirror_defense() {
    let mut p = ArbiParser::new();
    let inter = p.feed_line(&mission_line(
        1.0,
        "Arbitration: Berehynia Interception (Sedna)",
    ));
    assert_eq!(start_type(inter), ArbiMissionType::Interception);
    p.reset();
    let mirror = p.feed_line(&mission_line(1.0, "Arbitration: Tyana Pass (Mars)"));
    assert_eq!(start_type(mirror), ArbiMissionType::Defense);
}

#[test]
fn collects_squad_member_names_deduped_only_while_active() {
    let loadout =
        |ts: f64, name: &str| format!("{ts:.3} Game [Info]: {name} loadout loader finished.");
    let result = run_parser(vec![
        loadout(15.0, "OrbiterBoot"),
        mission_line(100.0, CASTA),
        loadout(105.0, "HostPlayer"),
        loadout(112.0, "ClientOne"),
        loadout(300.0, "HostPlayer"),
        reward_line(400.0),
        reward_line(700.0),
    ])
    .unwrap();
    assert_eq!(result.players, ["HostPlayer", "ClientOne"]);
}

#[test]
fn classifies_unknown_modes_as_other_with_null_stats() {
    let r = run_parser(vec![
        mission_line(100.0, "Arbitration: Olympus (Mars)"),
        reward_line(400.0),
        reward_line(700.0),
    ])
    .unwrap();
    assert_eq!(r.mission_type, ArbiMissionType::Other);
    assert!(r.stats.is_none());
    assert_eq!(r.rotations, 2);
}

#[test]
fn upgrades_other_to_defense_when_wave_lines_appear() {
    let r = run_parser(vec![
        mission_line(100.0, "Arbitration: Hydron (Sedna)"),
        wave_line(110.0, 1),
        drone_line(150.0),
        drone_line(200.0),
    ])
    .unwrap();
    assert_eq!(r.mission_type, ArbiMissionType::Defense);
    assert!(r.stats.is_some());
}

#[test]
fn ends_the_run_on_a_new_mission_name() {
    let mut p = ArbiParser::new();
    p.feed_line(&mission_line(100.0, CASTA));
    assert_eq!(
        p.feed_line(&mission_line(900.0, "Cetus (Earth)")),
        end(RunEndReason::NewMission)
    );
}

#[test]
fn ignores_host_migration_replays_of_the_mission_name() {
    let mut p = ArbiParser::new();
    p.feed_line(&mission_line(100.0, CASTA));
    p.feed_line(&drone_line(500.0));
    assert_eq!(p.feed_line(&mission_line(300.0, CASTA)), None);
    assert!(p.is_run_active());
    assert_eq!(
        p.feed_line(&mission_line(600.0, CASTA)),
        end(RunEndReason::NewMission)
    );
}

#[test]
fn counts_drones_separately_and_skips_excluded_agents() {
    let r = run_parser(vec![
        mission_line(100.0, CASTA),
        drone_line(110.0),
        drone_line(120.0),
        enemy_line(130.0, "GrineerLancerAgent", 5),
        enemy_line(131.0, "GrineerLancerAgent", 6),
        "132.000 AI [Info]: OnAgentCreated /Npc/TurretAgent3 MonitoredTicking 7".into(),
    ])
    .unwrap();
    assert_eq!(r.drones, 2);
    // 2 valid enemy spawns + 2 drones; turret excluded
    assert_eq!(r.total_enemies, 4);
}

#[test]
fn excludes_agents_whose_tick_counter_never_advances() {
    let r = run_parser(vec![
        mission_line(100.0, CASTA),
        enemy_line(110.0, "DecorativeAgent", 5),
        enemy_line(111.0, "DecorativeAgent", 5),
        enemy_line(112.0, "DecorativeAgent", 5),
        enemy_line(113.0, "RealAgent", 5),
    ])
    .unwrap();
    assert_eq!(r.total_enemies, 1);
}

#[test]
fn skips_spam_lines_entirely() {
    let r = run_parser(vec![
        mission_line(100.0, CASTA),
        "110.000 Game [Warning]: OnAgentCreated /Npc/GrineerLancerAgent3 MonitoredTicking 5".into(),
        "111.000 AI [Info]: OnAgentCreated /Npc/LancerAgent3 DamagePct MonitoredTicking 6".into(),
    ])
    .unwrap();
    assert_eq!(r.total_enemies, 0);
}

#[test]
fn debounces_rotation_rewards_within_30s() {
    let close = run_parser(vec![
        mission_line(100.0, CASTA),
        reward_line(400.0),
        reward_line(420.0),
    ]);
    assert_eq!(close.unwrap().rotations, 1);
    let apart = run_parser(vec![
        mission_line(100.0, CASTA),
        reward_line(400.0),
        reward_line(440.0),
    ]);
    assert_eq!(apart.unwrap().rotations, 2);
}

#[test]
fn computes_duration_from_precise_start_to_last_activity() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(90.0, 1),
        drone_line(100.0),
        drone_line(130.0),
    ])
    .unwrap();
    assert_eq!(r.duration_sec, 40.0);
    let stats = r.stats.unwrap();
    assert_eq!(stats.precise_start_sec, Some(90.0));
    assert_eq!(stats.avg_drone_interval_sec, Some(30.0));
}

#[test]
fn falls_back_to_the_first_drone_when_no_precise_start_exists() {
    let r = run_parser(vec![
        mission_line(80.0, "Arbitration: Berehynia Interception (Sedna)"),
        drone_line(100.0),
        drone_line(150.0),
    ])
    .unwrap();
    assert_eq!(r.duration_sec, 50.0);
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn builds_the_saturation_histogram_from_tick_samples() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(90.0, 1),
        tick_line(100.0, 2),
        tick_line(110.0, 16),
        tick_line(120.0, 5),
        drone_line(130.0),
    ])
    .unwrap();
    let b = r.stats.unwrap().saturation_buckets;
    assert_eq!(b.len(), 10);
    assert_eq!(b[0].label, "0-2");
    close(b[0].pct, 50.0);
    assert_eq!(b[5].label, "15-17");
    close(b[5].pct, 50.0);
    assert_eq!(b[9].label, "27+");
}

#[test]
fn excludes_paused_segments_from_saturation() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(90.0, 1),
        tick_line(100.0, 2),
        "105.000 Script [Info]: WaveDefend.lua: _SleepBetweenWaves(10)".into(),
        tick_line(110.0, 16),
        wave_start_line(115.0, 2),
        tick_line(120.0, 5),
        drone_line(130.0),
    ])
    .unwrap();
    // segment starting at 110 falls inside the [105,115] pause and is dropped
    let b = r.stats.unwrap().saturation_buckets;
    close(b[0].pct, 100.0);
    assert_eq!(b[5].pct, 0.0);
}

fn wave(index: u32, duration_sec: f64, saturation_pct: f64) -> ArbiWaveEntry {
    ArbiWaveEntry {
        index,
        duration_sec,
        saturation_pct,
    }
}

#[test]
fn computes_wave_durations_including_the_every_3rd_wave_countdown_path() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(100.0, 1),
        sleep3_line(130.0),
        wave_line(140.0, 2),
        sleep3_line(165.0),
        wave_line(170.0, 3),
        countdown_line(205.0),
        drone_line(210.0),
    ])
    .unwrap();
    assert_eq!(
        r.stats.unwrap().waves.unwrap(),
        [wave(1, 30.0, 0.0), wave(2, 25.0, 0.0), wave(3, 30.0, 0.0)]
    );
}

#[test]
fn computes_the_vitus_model_from_rotations_and_drones() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(90.0, 1),
        drone_line(100.0),
        reward_line(400.0),
    ])
    .unwrap();
    let s = r.stats.unwrap();
    assert!(s.expected_vitus_mean > 0.0);
    assert_eq!(s.waves_per_rotation, 3);
}

#[test]
fn returns_none_when_finalizing_with_no_active_run() {
    assert!(ArbiParser::new().finalize().is_none());
}

// Verbatim EE.log lines for an aborted Oestrus arbitration.
const PENDING_ELITE: &str =
    "158.074 Script [Info]: ThemedSquadOverlay.lua: Pending mission: SolNode167_EliteAlert";
const MISSION_NAME_SUFFIX: &str =
    "178.428 Script [Info]: ThemedSquadOverlay.lua: Mission name: Oestrus (Eris) - Arbitration";
const SYNC_CONSUMABLES_LINE: &str = "181.561 Sys [Info]: SyncAutoPopulatedConsumables for mission MT_PURIFY with location SolNode167";
const STATE_STARTED_LINE: &str = "184.403 Game [Info]: OnStateStarted, mission type=MT_PURIFY";
const ABORT_DIALOG: &str = "233.756 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=/Lotus/Language/Menu/AbortMissionConfirm, title= leftItem=/Menu/Confirm_Item_Yes, rightItem=/Menu/Confirm_Item_No)";
const ABORT_CONFIRMED_LINE: &str = "234.503 Script [Info]: TopMenu.lua: Abort: host/no session";
const EOM_COMMIT_LINE: &str = "234.503 Sys [Info]: EOM missionLocationUnlocked=1";
const CAPTURE_NAME: &str =
    "255.746 Script [Info]: ThemedSquadOverlay.lua: Mission name: Isos (Eris)";
// EndOfMatch.lua initializes in-mission, about 11s in, so it cannot end a run.
const IN_MISSION_END_OF_MATCH: &str = "432.123 Script [Info]: EndOfMatch.lua: Initialize";
const IN_MISSION_SUCCEEDED: &str = "432.123 Script [Info]: EndOfMatch.lua: Mission Succeeded";
const SURVIVAL_REWARD_LINE: &str =
    "735.449 Sys [Info]: Created /Lotus/Interface/SurvivalReward.swf";

fn started_with(lines: &[&str]) -> ArbiParser {
    let mut p = ArbiParser::new();
    for l in lines {
        p.feed_line(l);
    }
    p
}

#[test]
fn parses_the_node_from_the_suffix_name_format() {
    let mut p = ArbiParser::new();
    assert_eq!(
        p.feed_line(MISSION_NAME_SUFFIX),
        Some(ArbiParserEvent::RunStart {
            mission_name: "Oestrus (Eris) - Arbitration".into(),
            node: "Oestrus (Eris)".into(),
            mission_type: ArbiMissionType::Other,
            game_time_sec: 178.428,
        })
    );
}

#[test]
fn detects_an_arbi_via_the_elite_alert_sector_when_the_name_lacks_the_keyword() {
    let mut p = ArbiParser::new();
    p.feed_line(PENDING_ELITE);
    let event = p.feed_line(&mission_line(178.428, "Oestrus (Eris)"));
    assert!(matches!(event, Some(ArbiParserEvent::RunStart { .. })));
    p.feed_line(ABORT_CONFIRMED_LINE);
    assert_eq!(
        p.finalize().unwrap().sol_node.as_deref(),
        Some("SolNode167")
    );
    // The sector was consumed at run start: the next plain mission is not an arbi.
    assert_eq!(p.feed_line(CAPTURE_NAME), None);
    assert!(!p.is_run_active());
}

#[test]
fn does_not_end_the_run_on_the_abort_confirm_dialog_alone() {
    let mut p = started_with(&[MISSION_NAME_SUFFIX]);
    assert_eq!(p.feed_line(ABORT_DIALOG), None);
    assert!(p.is_run_active());
}

#[test]
fn ends_the_run_as_aborted_on_a_confirmed_abort() {
    let mut p = started_with(&[MISSION_NAME_SUFFIX]);
    assert_eq!(
        p.feed_line(ABORT_CONFIRMED_LINE),
        end(RunEndReason::Aborted)
    );
}

#[test]
fn ends_the_run_on_the_eom_inventory_commit() {
    let mut p = started_with(&[MISSION_NAME_SUFFIX]);
    assert_eq!(p.feed_line(EOM_COMMIT_LINE), end(RunEndReason::MissionEnd));
}

#[test]
fn does_not_end_the_run_on_in_mission_end_of_match_screens() {
    let mut p = started_with(&[MISSION_NAME_SUFFIX]);
    assert_eq!(p.feed_line(IN_MISSION_END_OF_MATCH), None);
    assert_eq!(p.feed_line(IN_MISSION_SUCCEEDED), None);
    assert!(p.is_run_active());
    assert_eq!(p.feed_line(EOM_COMMIT_LINE), end(RunEndReason::MissionEnd));
}

#[test]
fn counts_survival_rotation_rewards() {
    let r = run_parser(vec![
        mission_line(100.0, "Arbitration: Mot (Void)"),
        "110.000 Sys [Info]: SyncAutoPopulatedConsumables for mission MT_SURVIVAL with location SolNode409".into(),
        SURVIVAL_REWARD_LINE.into(),
        "1035.500 Sys [Info]: Created /Lotus/Interface/SurvivalReward.swf".into(),
    ])
    .unwrap();
    assert_eq!(r.rotations, 2);
}

#[test]
fn ignores_the_survival_reward_ui_outside_survivals() {
    // Real interception sequence: SurvivalReward.swf popped 25s before the true
    // DefenseReward and must not eat the rotation via the 30s debounce.
    let r = run_parser(vec![
        mission_line(99.488, "Rhea (Saturn) - Arbitration"),
        "103.240 Game [Info]: OnStateStarted, mission type=MT_TERRITORY".into(),
        drone_line(104.75),
        "133.194 Script [Info]: TerritoryMission.lua: Alpha has is now under Enemy control".into(),
        "331.811 Sys [Info]: Created /Lotus/Interface/SurvivalReward.swf".into(),
        drone_line(337.236),
        "356.940 Sys [Info]: Created /Lotus/Interface/DefenseReward.swf".into(),
    ])
    .unwrap();
    assert_eq!(r.rotations, 1);
    assert_eq!(r.stats.as_ref().unwrap().reward_timestamps, [356.94]);
    // duration runs to the real reward, not the last drone
    assert!((r.duration_sec - (356.94 - 133.194)).abs() < 1e-3);
}

#[test]
fn captures_the_engine_mission_type_and_node_id() {
    let r = run_parser(vec![
        MISSION_NAME_SUFFIX.into(),
        SYNC_CONSUMABLES_LINE.into(),
        STATE_STARTED_LINE.into(),
    ])
    .unwrap();
    assert_eq!(r.mission_type_raw.as_deref(), Some("MT_PURIFY"));
    assert_eq!(r.sol_node.as_deref(), Some("SolNode167"));
    assert_eq!(r.mission_type, ArbiMissionType::Other);
    assert!(r.stats.is_none());
}

#[test]
fn spans_duration_from_start_round_to_the_end_marker_when_no_combat_was_logged() {
    let mut p = started_with(&[
        MISSION_NAME_SUFFIX,
        STATE_STARTED_LINE,
        ABORT_CONFIRMED_LINE,
    ]);
    let r = p.finalize().unwrap();
    // 184.403 -> 234.503
    assert!((r.duration_sec - 50.1).abs() < 1e-3);
    assert_eq!(r.run_end_sec, Some(234.503));
}

#[test]
fn ignores_a_sub_second_load_drone_window_and_uses_the_end_marker() {
    // Level load can create a few drone agents within milliseconds; that must
    // not count as the combat window of an aborted run.
    let mut p = started_with(&[MISSION_NAME_SUFFIX, STATE_STARTED_LINE]);
    feed(
        &mut p,
        &[
            drone_line(200.001),
            drone_line(200.002),
            drone_line(200.003),
        ],
    );
    p.feed_line(ABORT_CONFIRMED_LINE);
    // first drone 200.001 -> abort 234.503
    assert!((p.finalize().unwrap().duration_sec - 34.502).abs() < 1e-3);
}

#[test]
fn lets_the_engine_type_outrank_later_wave_line_heuristics() {
    let r = run_parser(vec![
        mission_line(100.0, "Arbitration: Hydron (Sedna)"),
        SYNC_CONSUMABLES_LINE.into(),
        wave_line(110.0, 1),
    ])
    .unwrap();
    assert_eq!(r.mission_type, ArbiMissionType::Other);
}

#[test]
fn maps_mt_defense_and_mt_territory_to_full_stats_types() {
    let defense = run_parser(vec![
        mission_line(100.0, "Arbitration: Olympus (Mars)"),
        "110.000 Sys [Info]: SyncAutoPopulatedConsumables for mission MT_DEFENSE with location SolNode30".into(),
    ])
    .unwrap();
    assert_eq!(defense.mission_type, ArbiMissionType::Defense);
    assert!(defense.stats.is_some());

    let interception = run_parser(vec![
        mission_line(100.0, "Arbitration: Olympus (Mars)"),
        "110.000 Game [Info]: OnStateStarted, mission type=MT_TERRITORY".into(),
    ])
    .unwrap();
    assert_eq!(interception.mission_type, ArbiMissionType::Interception);
}

#[test]
fn types_mt_artifact_as_disruption_and_counts_rounds_not_reward_popups() {
    let r = run_parser(vec![
        mission_line(100.0, "Apollo (Lua) - Arbitration"),
        mt_line(101.0, "MT_ARTIFACT"),
        state_line(110.0, "ARTIFACT_ROUND"),
        drone_line(150.0),
        enemy_line(160.0, "ButcherAgent", 4),
        // Disruption spams the survival reward UI; it must not count as a rotation.
        "200.000 Sys [Info]: Created /Lotus/Interface/SurvivalReward.swf".into(),
        "260.000 Sys [Info]: Created /Lotus/Interface/SurvivalReward.swf".into(),
        state_line(400.0, "ARTIFACT_ROUND_DONE"),
        state_line(401.0, "REWARDS (host)"),
        state_line(420.0, "INTERVAL"),
        state_line(440.0, "ARTIFACT_ROUND"),
        drone_line(500.0),
        state_line(700.0, "ARTIFACT_ROUND_DONE"),
    ])
    .unwrap();
    assert_eq!(r.mission_type, ArbiMissionType::Disruption);
    assert_eq!(r.mission_type_raw.as_deref(), Some("MT_ARTIFACT"));
    assert_eq!(r.rotations, 2);
    assert!(r.host_telemetry);
    let s = r.stats.unwrap();
    // Four conduits per round drive the vitus model.
    assert_eq!(s.waves_per_rotation, 4);
    assert_eq!(s.waves.unwrap(), [wave(1, 290.0, 0.0), wave(2, 260.0, 0.0)]);
    // The interval between rounds is downtime, not fight time.
    assert_eq!(s.precise_start_sec, Some(110.0));
}

#[test]
fn starts_a_run_on_the_client_load_when_no_mission_name_line_follows() {
    let mut p = ArbiParser::new();
    p.feed_line("2040.984 Script [Info]: ThemedSquadOverlay.lua: Cached mission name=Apollo (Lua) - Arbitration (SolNode308)");
    let event = p.feed_line(r#"2047.451 Script [Info]: ThemedSquadOverlay.lua: LoadLevelMsg received. Client joining mission in-progress: {"name":"SolNode308_EliteAlert"}"#);
    assert_eq!(
        event,
        Some(ArbiParserEvent::RunStart {
            mission_name: "Apollo (Lua) - Arbitration".into(),
            node: "Apollo (Lua)".into(),
            mission_type: ArbiMissionType::Other,
            game_time_sec: 2047.451,
        })
    );
    // The later "Client loaded" line must not open a second run.
    assert_eq!(
        p.feed_line(r#"2051.361 Script [Info]: Client loaded {"name":"SolNode308_EliteAlert"} with MissionInfo:"#),
        None
    );
    p.feed_line(&drone_line(2100.0));
    assert_eq!(
        p.finalize().unwrap().sol_node.as_deref(),
        Some("SolNode308")
    );
}

#[test]
fn ignores_a_client_load_for_a_normal_non_arbitration_mission() {
    let mut p = ArbiParser::new();
    let event = p.feed_line(r#"100.000 Script [Info]: ThemedSquadOverlay.lua: Client loaded {"name":"SolNode308"} with MissionInfo:"#);
    assert_eq!(event, None);
    assert!(!p.is_run_active());
}

fn iv(start: f64, end: f64) -> ArbiInterval {
    ArbiInterval { start, end }
}

#[test]
fn records_reward_pauses_and_tick_stream_stalls() {
    let sleep = |ts: f64| format!("{ts:.3} Script [Info]: WaveDefend.lua: _SleepBetweenWaves");
    let r = run_parser(vec![
        mission_line(100.0, CASTA),
        wave_line(110.0, 1),
        drone_line(120.0),
        drone_line(130.0),
        sleep(140.0),
        wave_start_line(160.0, 2),
        drone_line(170.0),
        tick_line(200.0, 5),
        tick_line(210.0, 6),
        // 90s without a tick line: a load screen or host stall, not gameplay.
        tick_line(300.0, 7),
        reward_line(400.0),
        wave_start_line(430.0, 4),
        drone_line(440.0),
        reward_line(700.0),
    ])
    .unwrap();
    let s = r.stats.unwrap();
    assert_eq!(s.pause_intervals, [iv(140.0, 160.0), iv(400.0, 430.0)]);
    assert_eq!(s.idle_intervals, [iv(210.0, 300.0)]);
}

#[test]
fn closes_a_reward_pause_the_log_never_unpauses() {
    let r = run_parser(vec![
        mission_line(100.0, CASTA),
        wave_line(110.0, 1),
        drone_line(120.0),
        reward_line(400.0),
        wave_start_line(430.0, 4),
        drone_line(440.0),
        reward_line(700.0),
        drone_line(750.0),
    ])
    .unwrap();
    assert_eq!(
        r.stats.unwrap().pause_intervals,
        [iv(400.0, 430.0), iv(700.0, 750.0)]
    );
}

fn spawn_point_line(ts: f64, point: &str, x: f64, y: f64, z: f64) -> String {
    format!(
        "{ts:.3} Script [Info]: WaveDefend.lua: Spawned a /Npc/ShieldLancer10 @ Vector(1, 2, 3), \
         spawn point: {point} @ Vector({x}, {y}, {z}), total spawned in current wave: 1"
    )
}

/// Per-wave counts for waves 1..n, padded out to the cap.
fn early_waves(counts: &[u32]) -> Option<Vec<u32>> {
    Some(
        (0..ARBI_EARLY_WAVE_CAP)
            .map(|i| counts.get(i).copied().unwrap_or(0))
            .collect(),
    )
}

fn point(id: &str, x: f64, y: f64, z: f64, count: u32, early: Option<Vec<u32>>) -> ArbiSpawnPoint {
    ArbiSpawnPoint {
        id: id.into(),
        x,
        y,
        z,
        count,
        early,
    }
}

#[test]
fn aggregates_spawn_points_by_id_keeping_the_points_own_vector_rounded() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(90.0, 1),
        spawn_point_line(91.0, "/Layer1/Layer1/NpcSpawnPoint37", 50.25, 98.0789, 53.0),
        spawn_point_line(92.0, "/Layer1/Layer1/NpcSpawnPoint37", 50.25, 98.0789, 53.0),
        spawn_point_line(93.0, "/Layer1/Layer1/NpcSpawnPoint177", 117.0, 106.75, 29.0),
        drone_line(100.0),
    ])
    .unwrap();
    let s = r.stats.unwrap();
    assert_eq!(
        s.spawn_points,
        [
            point(
                "/Layer1/Layer1/NpcSpawnPoint37",
                50.3,
                98.1,
                53.0,
                2,
                early_waves(&[2])
            ),
            point(
                "/Layer1/Layer1/NpcSpawnPoint177",
                117.0,
                106.8,
                29.0,
                1,
                early_waves(&[1])
            ),
        ]
    );
    assert_eq!(s.spawn_data_version, 2);
}

#[test]
fn ignores_spawn_lines_logged_before_the_run_starts() {
    let r = run_parser(vec![
        spawn_point_line(10.0, "/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0),
        mission_line(80.0, CASTA),
        wave_line(90.0, 1),
        spawn_point_line(91.0, "/Layer1/NpcSpawnPoint2", 4.0, 5.0, 6.0),
        drone_line(100.0),
    ])
    .unwrap();
    assert_eq!(
        r.stats.unwrap().spawn_points,
        [point(
            "/Layer1/NpcSpawnPoint2",
            4.0,
            5.0,
            6.0,
            1,
            early_waves(&[1])
        )]
    );
}

#[test]
fn splits_the_count_over_the_waves_it_fired_in_and_cuts_by_wave() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        spawn_point_line(85.0, "/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0),
        wave_line(90.0, 1),
        spawn_point_line(91.0, "/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0),
        wave_line(200.0, 7),
        spawn_point_line(201.0, "/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0),
        spawn_point_line(202.0, "/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0),
        drone_line(210.0),
    ])
    .unwrap();
    let p = r.stats.unwrap().spawn_points.remove(0);
    // The line before wave 1 belongs to wave 1, so the run stays fully counted.
    assert_eq!(
        p,
        point(
            "/Layer1/NpcSpawnPoint1",
            1.0,
            2.0,
            3.0,
            4,
            early_waves(&[2, 0, 0, 0, 0, 0, 2])
        )
    );
    assert_eq!(count_from_wave(&p, 1), 4);
    assert_eq!(count_from_wave(&p, 7), 2);
    assert_eq!(count_from_wave(&p, 8), 0);
    // Past the tracked window every tracked wave is cut, and all 4 are tracked.
    assert_eq!(count_from_wave(&p, ARBI_EARLY_WAVE_CAP + 2), 0);
}

#[test]
fn leaves_the_wave_counts_off_a_point_that_only_fires_past_the_cap() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(90.0, 16),
        spawn_point_line(91.0, "/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0),
        drone_line(100.0),
    ])
    .unwrap();
    let p = r.stats.unwrap().spawn_points.remove(0);
    assert_eq!(p, point("/Layer1/NpcSpawnPoint1", 1.0, 2.0, 3.0, 1, None));
    // Nothing inside the window, so no cut inside it can remove the spawn.
    assert_eq!(count_from_wave(&p, 7), 1);
}

#[test]
fn caps_distinct_points_but_keeps_counting_the_ones_it_already_knows() {
    let mut lines = vec![mission_line(80.0, CASTA), wave_line(90.0, 1)];
    for i in 0..305 {
        let i = i as f64;
        lines.push(spawn_point_line(
            100.0 + i,
            &format!("/Layer1/NpcSpawnPoint{i}"),
            i,
            0.0,
            i,
        ));
    }
    lines.push(spawn_point_line(
        500.0,
        "/Layer1/NpcSpawnPoint0",
        0.0,
        0.0,
        0.0,
    ));
    lines.push(drone_line(510.0));

    let points = run_parser(lines).unwrap().stats.unwrap().spawn_points;
    assert_eq!(points.len(), 300);
    assert_eq!(
        points[0],
        point(
            "/Layer1/NpcSpawnPoint0",
            0.0,
            0.0,
            0.0,
            2,
            early_waves(&[2])
        )
    );
    assert!(!points.iter().any(|p| p.id == "/Layer1/NpcSpawnPoint300"));
}

#[test]
fn emits_an_empty_spawn_list_for_interception() {
    let r = run_parser(vec![
        mission_line(99.0, "Rhea (Saturn) - Arbitration"),
        "103.240 Game [Info]: OnStateStarted, mission type=MT_TERRITORY".into(),
        drone_line(120.0),
        "133.194 Script [Info]: TerritoryMission.lua: Alpha has is now under Enemy control".into(),
        "356.940 Sys [Info]: Created /Lotus/Interface/DefenseReward.swf".into(),
    ])
    .unwrap();
    assert_eq!(r.mission_type, ArbiMissionType::Interception);
    assert!(r.stats.unwrap().spawn_points.is_empty());
}

#[test]
fn scores_each_wave_over_its_own_window() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(100.0, 1),
        tick_line(100.0, 20),
        tick_line(110.0, 2),
        tick_line(120.0, 2),
        sleep3_line(130.0),
        wave_line(140.0, 2),
        tick_line(140.0, 20),
        tick_line(150.0, 20),
        tick_line(160.0, 20),
        sleep3_line(165.0),
        drone_line(170.0),
    ])
    .unwrap();
    let waves = r.stats.unwrap().waves.unwrap();
    assert_eq!(
        waves.iter().map(|w| w.duration_sec).collect::<Vec<_>>(),
        [30.0, 25.0]
    );
    close(waves[0].saturation_pct, 100.0 / 3.0);
    close(waves[1].saturation_pct, 100.0);
}

#[test]
fn scores_one_rotation_window_per_reward_and_drops_paused_segments() {
    let r = run_parser(vec![
        mission_line(80.0, CASTA),
        wave_line(100.0, 1),
        tick_line(100.0, 20),
        tick_line(110.0, 20),
        tick_line(120.0, 2),
        reward_line(130.0),
        wave_start_line(140.0, 2),
        tick_line(140.0, 20),
        tick_line(150.0, 2),
        tick_line(160.0, 2),
        reward_line(200.0),
        drone_line(210.0),
    ])
    .unwrap();
    let s = r.stats.unwrap();
    assert_eq!(s.reward_timestamps, [130.0, 200.0]);
    assert_eq!(s.rotation_saturation_pct.len(), s.reward_timestamps.len());
    close(s.rotation_saturation_pct[0], 200.0 / 3.0);
    // Without the [130,140] pause exclusion this window would read 33.3%.
    close(s.rotation_saturation_pct[1], 50.0);
}

// Real EE.log excerpts (spec fixtures from WFHelper).
macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/eelog/arbi/",
            $name
        ))
    };
}

/// Feed the whole file and finalize once: the skipped fixtures never reach a record.
fn parse_fixture(text: &str) -> ArbiParsedRun {
    let mut p = ArbiParser::new();
    for line in text.lines() {
        p.feed_line(line);
    }
    p.finalize().unwrap()
}

#[test]
fn fixture_aborted_oestrus_parses_as_a_non_full_stats_run() {
    let r = parse_fixture(fixture!("oestrus-abort-ee.log"));
    assert_eq!(r.node, "Oestrus (Eris)");
    assert_eq!(r.mission_type, ArbiMissionType::Other);
    assert_eq!(r.mission_type_raw.as_deref(), Some("MT_PURIFY"));
    assert_eq!(r.sol_node.as_deref(), Some("SolNode167"));
    assert_eq!(r.rotations, 0);
    assert!(r.stats.is_none());
    assert_eq!(r.drones, 0);
    // FlightAgents never advanced MonitoredTicking - the post-pass drops them.
    assert_eq!(r.total_enemies, 0);
}

#[test]
fn fixture_one_rotation_survival_parses_past_the_end_of_match_screens() {
    let r = parse_fixture(fixture!("mot-survival-ee.log"));
    assert_eq!(r.node, "Mot (Void)");
    assert_eq!(r.mission_type, ArbiMissionType::Other);
    assert_eq!(r.mission_type_raw.as_deref(), Some("MT_SURVIVAL"));
    assert_eq!(r.sol_node.as_deref(), Some("SolNode409"));
    // The EndOfMatch.lua screens at 432/577 must not have ended the run.
    assert_eq!(r.rotations, 1);
    assert_eq!(r.drones, 7);
    // first drone 434.607 -> last drone 735.957
    assert!((r.duration_sec - 301.35).abs() < 5e-3);
}

#[test]
fn fixture_one_rotation_interception_ignores_the_stray_survival_reward_ui() {
    let r = parse_fixture(fixture!("rhea-interception-ee.log"));
    assert_eq!(r.node, "Rhea (Saturn)");
    assert_eq!(r.mission_type, ArbiMissionType::Interception);
    assert_eq!(r.mission_type_raw.as_deref(), Some("MT_TERRITORY"));
    assert_eq!(r.sol_node.as_deref(), Some("SolNode18"));
    assert_eq!(r.rotations, 1);
    assert_eq!(r.drones, 5);
    // The rotation anchors on the DefenseReward, not the stray SurvivalReward.
    let s = r.stats.unwrap();
    assert_eq!(s.reward_timestamps, [356.94]);
    assert_eq!(s.precise_start_sec, Some(133.194));
    // first territory control 133.194 -> reward 356.940 (matches the reference site)
    assert!((r.duration_sec - 223.746).abs() < 5e-4);
}

#[test]
fn fixture_defense_run_has_a_wave_map_and_ends_on_the_eom_commit() {
    // The importer closes a run on its end event, so segment the same way.
    let mut p = ArbiParser::new();
    let mut runs = Vec::new();
    for line in fixture!("stoefler-defense-ee.log").lines() {
        if matches!(p.feed_line(line), Some(ArbiParserEvent::RunEnd { .. })) {
            runs.extend(p.finalize());
        }
    }
    runs.extend(p.finalize());
    assert_eq!(runs.len(), 1);
    let r = &runs[0];
    assert_eq!(r.node, "Stöfler (Lua)");
    assert_eq!(r.mission_type, ArbiMissionType::Defense);
    assert_eq!(r.mission_type_raw.as_deref(), Some("MT_DEFENSE"));
    assert_eq!(r.sol_node.as_deref(), Some("SolNode305"));
    assert_eq!(r.rotations, 2);
    assert_eq!(r.drones, 3);
    // 2x ShieldLancer + 2x ShotgunLancer + 3 drones; the kubrow also counts -
    // a single spawn can never be proven non-ticking (reference-site parity).
    assert_eq!(r.total_enemies, 8);

    let s = r.stats.as_ref().unwrap();
    assert_eq!(s.waves_per_rotation, 3);
    assert_eq!(s.reward_timestamps, [16855.934, 16936.816]);
    // "Defense wave: 1" is the precise start.
    assert_eq!(s.precise_start_sec, Some(16786.896));
    assert!((r.duration_sec - (16936.816 - 16786.896)).abs() < 5e-4);

    // Wave durations: sleep-marker windows, every 3rd wave via countdown-5.
    let waves = s.waves.as_ref().unwrap();
    assert_eq!(
        waves.iter().map(|w| w.index).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6]
    );
    let near = |a: f64, b: f64| assert!((a - b).abs() < 5e-4, "{a} != {b}");
    near(waves[0].duration_sec, 16.151);
    near(waves[1].duration_sec, 22.873);
    near(waves[2].duration_sec, 16855.932 - 5.0 - 16831.973);
    // Wave 4 ends on a "!"-prefixed sleep line - the timestamp must still parse.
    near(waves[3].duration_sec, 19.101);
    near(waves[5].duration_sec, 16936.807 - 5.0 - 16910.236);
}
