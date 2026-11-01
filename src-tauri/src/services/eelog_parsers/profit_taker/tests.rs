use super::*;

const SINGLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/eelog/pt/host-single-run.log"
));
const TWO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/eelog/pt/host-two-runs.log"
));

/// Times come straight from EE.log floats; a millisecond of slack is plenty.
const TOL: f64 = 0.01;

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() < tol, "{a} != {b} (tol {tol})");
}

fn parse_all<S: AsRef<str>>(lines: &[S]) -> Vec<PtParsedRun> {
    let mut parser = ProfitTakerParser::new();
    let mut runs = Vec::new();
    for line in lines {
        let line = line.as_ref();
        if matches!(parser.feed_line(line), Some(PtParserEvent::RunEnd { .. })) {
            runs.extend(parser.finalize());
            parser.feed_line(line);
        }
    }
    if parser.is_run_active() {
        runs.extend(parser.finalize());
    }
    runs
}

fn fixture_lines(text: &str) -> Vec<String> {
    text.lines().map(String::from).collect()
}

fn parse_fixture(text: &str) -> Vec<PtParsedRun> {
    parse_all(&fixture_lines(text))
}

fn ts(v: f64) -> String {
    format!("{v:.3}")
}
fn session_line(t: f64) -> String {
    format!(
        "{} Script [Info]: EidolonMP.lua: Session map string: /Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyFour",
        ts(t)
    )
}
fn elevator_line(t: f64) -> String {
    format!(
        "{} Script [Info]: EidolonMP.lua: EIDOLONMP: Avatar left the zone: TennoAvatar1, player pos: Vector(0, 0, 0)",
        ts(t)
    )
}
fn phase_line(t: f64, which: &str) -> String {
    format!(
        "{} Script [Info]: CamperHeistOrbFight.lua: Orb Fight - Starting {which} attack Orb phase",
        ts(t)
    )
}
fn shield_line(t: f64, element: &str) -> String {
    format!(
        "{} AI [Info]: Camper->SwitchShieldVulnerability() - Switching shield damage vulnerability type to {element}",
        ts(t)
    )
}
fn shield_end_line(t: f64, id: &str) -> String {
    format!(
        "{} Sys [Info]: ResourceLoader 0x1 (/Lotus/Sounds/Dialog/FortunaOrbHeist/Business/{id}) Found 479 items to load (1ms)",
        ts(t)
    )
}
fn leg_line(t: f64, part: &str) -> String {
    format!(
        "{} AI [Info]: Camper->DestroyLeg() - Leg freshly destroyed at part: {part}",
        ts(t)
    )
}
fn vulnerable_line(t: f64) -> String {
    format!(
        "{} AI [Info]: Camper->StartVulnerable() - The Camper can now be damaged!",
        ts(t)
    )
}
fn state_line(t: f64, state: u32) -> String {
    format!(
        "{} Script [Info]: CamperHeistOrbFight.lua: Landscape - New State: {state} for /Lotus/Types/Gameplay/Venus/Encounters/Heists/CamperHeistOrbFight on DynamicRandomEncounterHint, was 0",
        ts(t)
    )
}
fn pylon_line(t: f64) -> String {
    format!(
        "{} Script [Info]: ArachnoidCamperScript.lua: Pylon launch complete",
        ts(t)
    )
}
fn eom_line(t: f64) -> String {
    format!("{} Sys [Info]: EOM missionLocationUnlocked=1", ts(t))
}

/// Replace a leading timestamp, like the TS fixtures' `.replace(/^\d+\.\d+/, ..)`.
fn restamp(line: &str, t: &str) -> String {
    let end = line.find(' ').unwrap();
    format!("{t}{}", &line[end..])
}

#[test]
fn does_not_turn_a_loadout_before_capture_into_a_recorded_squad_member() {
    let mut lines = fixture_lines(SINGLE);
    let i = lines.iter().position(|l| l.contains("ClientOne")).unwrap();
    let client = lines.remove(i);
    lines.insert(0, restamp(&client, "5200.000"));
    let runs = parse_all(&lines);
    assert!(runs[0].complete);
    assert_eq!(runs[0].players, ["HostPlayer"]);
}

#[test]
fn captures_players_loading_between_the_pt_mission_name_and_active_job_line() {
    let mut lines = fixture_lines(SINGLE);
    let i = lines.iter().position(|l| l.contains("ClientOne")).unwrap();
    let client = lines.remove(i);
    let active = lines
        .iter()
        .position(|l| l.contains("Active jobId"))
        .unwrap();
    lines.insert(active, restamp(&client, "5227.150"));
    lines.insert(
        0,
        "5200.000 Game [Info]: PreviousHubPlayer loadout loader finished.".into(),
    );
    let runs = parse_all(&lines);
    assert_eq!(runs[0].players, ["ClientOne", "HostPlayer"]);
}

#[test]
fn matches_the_reference_analyser_on_a_recorded_host_run() {
    let runs = parse_fixture(SINGLE);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];

    assert!(run.confirmed && run.complete && run.host_telemetry);
    assert!(!run.bugged && !run.aborted && !run.host_migration && !run.flight_unreliable);
    assert_eq!(run.players, ["HostPlayer", "ClientOne"]);

    near(run.run_start_sec, 5316.394, 1e-3);
    near(run.run_end_sec.unwrap(), 5429.365, 1e-3);
    near(run.flight_sec, 11.339, TOL);
    near(run.duration_sec, 112.971, TOL);
    near(run.shield_sec, 27.25, TOL);
    near(run.leg_sec, 27.107, TOL);
    near(run.body_sec, 2.131, TOL);
    near(run.pylon_sec, 24.573, TOL);

    assert_eq!(
        run.phases.iter().map(|p| p.index).collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );
    for (p, want) in run.phases.iter().zip([39.662, 9.728, 39.193, 13.049]) {
        near(p.total_sec, want, TOL);
    }
}

fn shield(element: &str, seconds: f64, p: &PtShieldChange) {
    assert_eq!(p.element, element);
    near(p.seconds, seconds, TOL);
}

#[test]
fn splits_each_phase_into_shields_legs_body_and_pylons() {
    let run = parse_fixture(SINGLE).remove(0);
    let [p1, p2, p3, p4] = &run.phases[..] else {
        panic!("want 4 phases")
    };

    assert_eq!(p1.shields.len(), 4);
    shield("electric", 4.488, &p1.shields[0]);
    shield("toxin", 4.372, &p1.shields[1]);
    shield("slash", 3.521, &p1.shields[2]);
    shield("magnetic", 0.006, &p1.shields[3]);
    assert_eq!(
        p1.legs.iter().map(|l| l.leg).collect::<Vec<_>>(),
        [
            PtLeg::BackLeft,
            PtLeg::FrontRight,
            PtLeg::FrontLeft,
            PtLeg::BackRight
        ]
    );
    near(p1.legs[0].seconds, 2.017, TOL);
    near(p1.shield_sec, 12.387, TOL);
    near(p1.leg_sec, 6.687, TOL);
    near(p1.body_sec, 0.887, TOL);
    near(p1.pylon_sec, 12.228, TOL);

    // Phase 2 has no shield phase and no pylons.
    assert!(p2.shields.is_empty());
    near(p2.leg_sec, 7.282, TOL);
    near(p2.body_sec, 0.206, TOL);
    assert_eq!(p2.pylon_sec, 0.0);

    let elements: Vec<&str> = p3.shields.iter().map(|s| s.element.as_str()).collect();
    assert_eq!(
        elements,
        [
            "cold",
            "radiation",
            "corrosive",
            "viral",
            "puncture",
            "heat"
        ]
    );
    near(p3.shield_sec, 10.436, TOL);
    near(p3.leg_sec, 7.195, TOL);
    near(p3.body_sec, 0.631, TOL);
    near(p3.pylon_sec, 12.345, TOL);

    // The phase-3 pylon switch carries its element into phase 4.
    shield("gas", 3.28, &p4.shields[0]);
    near(p4.shield_sec, 4.427, TOL);
    near(p4.leg_sec, 5.943, TOL);
    near(p4.body_sec, 0.407, TOL);
    assert_eq!(p4.pylon_sec, 0.0);
}

#[test]
fn reads_both_runs_out_of_a_two_run_log_and_falls_back_for_a_missing_elevator_exit() {
    let runs = parse_fixture(TWO);
    assert_eq!(runs.len(), 2);
    let second = &runs[1];

    assert!(second.complete);
    assert_eq!(second.players, ["HostPlayer"]);
    // No "Avatar left the zone" line, so flight is measured from the job-start line.
    assert!(second.flight_unreliable);
    near(second.run_start_sec, 5704.634, 1e-3);
    near(second.flight_sec, 15.851, TOL);
    near(second.duration_sec, 100.618, TOL);
}

#[test]
fn phase_totals_plus_flight_equal_the_run_duration() {
    for run in parse_fixture(TWO) {
        let sum = run
            .phases
            .iter()
            .fold(run.flight_sec, |t, p| t + p.total_sec);
        assert!((sum - run.duration_sec).abs() < TOL);
    }
}

#[test]
fn ends_the_run_as_aborted_when_the_mission_state_closes_early() {
    let runs = parse_all(&[
        session_line(10.0),
        elevator_line(11.0),
        phase_line(20.0, "first"),
        shield_line(21.0, "DT_FIRE"),
        format!(
            "{} Net [Info]: GameRulesImpl - changing state from SS_STARTED to SS_ENDING",
            ts(25.0)
        ),
    ]);
    assert_eq!(runs.len(), 1);
    assert!(runs[0].aborted && !runs[0].complete);
    near(runs[0].run_end_sec.unwrap(), 25.0, 1e-3);
}

#[test]
fn treats_a_shield_switch_after_the_phase_3_pylons_as_the_missing_phase_4() {
    let runs = parse_all(&[
        session_line(10.0),
        elevator_line(11.0),
        phase_line(20.0, "first"),
        phase_line(30.0, "second"),
        phase_line(40.0, "third"),
        shield_line(41.0, "DT_FIRE"),
        shield_line(43.0, "DT_FREEZE"),
        shield_end_line(45.0, "DBntyFourInterPrTk0890TheBusiness"),
        leg_line(50.0, "ARM_LEFT"),
        pylon_line(60.0),
        shield_line(61.0, "DT_GAS"),
        shield_line(65.0, "DT_VIRAL"),
        eom_line(70.0),
    ]);
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert!(run.bugged);
    // Phase 3 closed on the guessed marker, so its pylon window is not counted.
    let phase3 = run.phases.iter().find(|p| p.index == 3).unwrap();
    near(phase3.total_sec, 25.0, 1e-3);
    assert_eq!(phase3.pylon_sec, 0.0);
    assert_eq!(run.pylon_sec, 0.0);
}

#[test]
fn flags_more_than_four_leg_breaks_in_one_phase_as_bugged() {
    let runs = parse_all(&[
        session_line(10.0),
        elevator_line(11.0),
        phase_line(20.0, "first"),
        leg_line(21.0, "ARM_LEFT"),
        leg_line(22.0, "ARM_RIGHT"),
        leg_line(23.0, "LEG_LEFT"),
        leg_line(24.0, "LEG_RIGHT"),
        leg_line(25.0, "ARM_LEFT"),
        phase_line(30.0, "second"),
        eom_line(40.0),
    ]);
    assert!(runs[0].bugged);
    assert_eq!(runs[0].phases[0].legs.len(), 5);
}

#[test]
fn keeps_an_unmapped_damage_type_as_its_lowercased_token() {
    let runs = parse_all(&[
        session_line(10.0),
        elevator_line(11.0),
        phase_line(20.0, "first"),
        shield_line(21.0, "DT_MYSTERY"),
        shield_line(24.0, "DT_FIRE"),
        phase_line(30.0, "second"),
        eom_line(40.0),
    ]);
    assert_eq!(runs[0].phases[0].shields.len(), 1);
    shield("dt_mystery", 4.0, &runs[0].phases[0].shields[0]);
}

#[test]
fn marks_a_client_side_run_whose_log_has_no_camper_telemetry() {
    let runs = parse_all(&[
        session_line(10.0),
        elevator_line(11.0),
        phase_line(20.0, "first"),
        state_line(25.0, 3),
        phase_line(30.0, "second"),
        eom_line(40.0),
    ]);
    assert!(!runs[0].host_telemetry);
    assert_eq!(runs[0].phases.len(), 1);
}

#[test]
fn counts_the_third_vulnerability_window_in_a_phase_as_the_kill() {
    let runs = parse_all(&[
        session_line(10.0),
        elevator_line(11.0),
        phase_line(20.0, "first"),
        vulnerable_line(30.0),
        vulnerable_line(30.0),
        vulnerable_line(30.5),
    ]);
    assert_eq!(runs.len(), 1);
    assert!(runs[0].complete);
    near(runs[0].run_end_sec.unwrap(), 30.5, 1e-3);
    near(runs[0].phases[0].body_sec, 0.5, 1e-3);
}

#[test]
fn ignores_the_other_heist_bounties_and_records_a_host_migration() {
    let runs = parse_all(&[
        format!(
            "{} Script [Info]: ThemedSquadOverlay.lua: Active jobId set to /Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyOne_-6",
            ts(5.0)
        ),
        format!(
            "{} Script [Info]: EidolonMP.lua: Session map string: /Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyTwo",
            ts(6.0)
        ),
        session_line(10.0),
        elevator_line(11.0),
        format!(
            "{}     \"jobId\" : \"/Lotus/Types/Gameplay/Venus/Jobs/Heists/HeistProfitTakerBountyFour_-6\",",
            ts(15.0)
        ),
        phase_line(20.0, "first"),
        phase_line(30.0, "second"),
        eom_line(40.0),
    ]);
    assert_eq!(runs.len(), 1);
    assert!(runs[0].host_migration);
}
