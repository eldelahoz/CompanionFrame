// The original has no state-machine unit tests (its overlay tests mock it), so these
// cover the documented flows of the TS source directly.
use super::*;

const T0: f64 = 1_000_000.0;
const OPEN: &str = "10.0 Sys [Info]: Created /Lotus/Interface/OmegaRerollSelection.swf";
const DIORAMA: &str = "11.0 Script [Info]: OmegaRerollSelection.lua: Diorama setup complete";
const CLOSE: &str = "50.0 Sys [Info]: NpcManager::ClearAgents() ReadyToCreateAgents = false";
const CYCLE: &str = "20.0 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Are you sure you want to cycle Braton Prime for 1,200?, title= leftItem=/x)";
const CHOICE: &str = "30.0 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Cycle Riven into current selection?, title= leftItem=/x)";
const SEND_OK: &str = "21.0 Script [Info]: Dialog.lua: Dialog::SendResult(4)";
const WEAPON: &str =
    "10.5 Sys [Info]: ResourceLoader 0x1A2B (/Lotus/Weapons/Tenno/Rifle/PrimeBraton) Found 5 items";

fn feed(m: &mut RivenStateMachine, line: &str, at: f64) -> RivenOutput {
    m.process(line, LineSource::Dbwin, true, T0 + at)
}

#[test]
fn walks_open_roll_confirm_choice_and_close() {
    let mut m = RivenStateMachine::new();
    assert_eq!(feed(&mut m, OPEN, 0.0).events, [RivenEvent::SessionOpen]);
    assert!(m.is_session_active());
    assert_eq!(
        feed(&mut m, WEAPON, 100.0).events,
        [RivenEvent::WeaponPath(
            "/Lotus/Weapons/Tenno/Rifle/PrimeBraton".into()
        )]
    );
    assert_eq!(
        feed(&mut m, DIORAMA, 200.0).events,
        [RivenEvent::DioramaSetup]
    );

    let pending = feed(&mut m, CYCLE, 1000.0);
    assert_eq!(
        pending.events,
        [RivenEvent::RollPending {
            weapon: "Braton Prime".into(),
            kuva_per_roll: 1200
        }]
    );
    let confirmed = feed(&mut m, SEND_OK, 2000.0);
    assert_eq!(confirmed.events, [RivenEvent::RollConfirmed]);
    assert!(confirmed.send_result_consumed);

    feed(&mut m, CHOICE, 3000.0);
    assert_eq!(
        feed(&mut m, SEND_OK, 4000.0).events,
        [RivenEvent::ChoiceConfirmed]
    );

    assert_eq!(
        feed(&mut m, CLOSE, 10_000.0).events,
        [RivenEvent::SessionClose]
    );
    assert!(!m.is_session_active());
}

#[test]
fn a_cancelled_dialog_dispatches_nothing() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    feed(&mut m, CYCLE, 1000.0);
    let out = feed(&mut m, "21.0 Dialog.lua: Dialog::SendResult(5)", 2000.0);
    assert!(out.events.is_empty());
    assert!(out.send_result_consumed);
}

#[test]
fn the_open_marker_burst_opens_one_session() {
    let mut m = RivenStateMachine::new();
    assert_eq!(feed(&mut m, OPEN, 0.0).events.len(), 1);
    for i in 1..30 {
        assert!(feed(&mut m, OPEN, i as f64).events.is_empty());
    }
}

#[test]
fn close_markers_are_ignored_until_the_diorama_is_ready() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    assert!(feed(&mut m, CLOSE, 100.0).events.is_empty());
    feed(&mut m, DIORAMA, 200.0);
    assert_eq!(
        feed(&mut m, CLOSE, 300.0).events,
        [RivenEvent::SessionClose]
    );
}

#[test]
fn a_trailing_close_marker_inside_the_next_session_is_ignored() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    feed(&mut m, DIORAMA, 100.0);
    feed(&mut m, CLOSE, 200.0);
    feed(&mut m, OPEN, 300.0);
    feed(&mut m, DIORAMA, 400.0);
    // Tail of the previous close, 1.3s after its marker: swallowed.
    assert!(feed(&mut m, CLOSE, 1500.0).events.is_empty());
    assert!(m.is_session_active());
}

#[test]
fn file_echoes_are_skipped_while_a_realtime_source_is_active() {
    let mut m = RivenStateMachine::new();
    let out = m.process(OPEN, LineSource::File, true, T0);
    assert!(out.events.is_empty());
    assert!(!m.is_session_active());
    // Without a realtime source the file poll drives the machine.
    assert_eq!(
        m.process(OPEN, LineSource::File, false, T0).events,
        [RivenEvent::SessionOpen]
    );
}

#[test]
fn the_idle_backstop_closes_a_session_whose_close_marker_was_missed() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    assert!(m.tick(T0 + 599_000.0).is_empty());
    assert_eq!(m.tick(T0 + 600_000.0), [RivenEvent::SessionClose]);
    assert!(!m.is_session_active());
}

#[test]
fn an_overlong_session_is_force_closed_even_while_matches_continue() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    let mut t = 0.0;
    while t + 300_000.0 < 29.0 * 60_000.0 {
        t += 300_000.0;
        feed(&mut m, DIORAMA, t);
    }
    let out = feed(&mut m, DIORAMA, 30.0 * 60_000.0 + 1.0);
    assert!(out.events.contains(&RivenEvent::SessionClose));
    assert!(!m.is_session_active());
    // The force-end cooldown keeps a stray cycle dialog from re-arming the session.
    assert!(feed(&mut m, CYCLE, 30.0 * 60_000.0 + 2.0).events.is_empty());
}

#[test]
fn a_chat_linked_riven_is_confirmed_by_populate_info_after_a_hudvis_increase() {
    let mut m = RivenStateMachine::new();
    feed(
        &mut m,
        "1.0 ThemedDetailedPurchaseDialog.lua: DBG: HudVis 1",
        0.0,
    );
    feed(&mut m, WEAPON, 100.0);
    let out = feed(
        &mut m,
        "1.1 ThemedDetailedPurchaseDialog.lua: PopulateInfo->/Lotus/StoreItems/Upgrades/Mods/Randomized/LotusRifleRandomModRare",
        500.0,
    );
    assert_eq!(
        out.events,
        [
            RivenEvent::ChatView,
            RivenEvent::WeaponPath("/Lotus/Weapons/Tenno/Rifle/PrimeBraton".into())
        ]
    );
    // HudVis dropping below the level it was confirmed at closes the view.
    let closed = feed(
        &mut m,
        "2.0 ThemedDetailedPurchaseDialog.lua: DBG: HudVis 0",
        900.0,
    );
    assert_eq!(closed.events, [RivenEvent::SessionClose]);
}

#[test]
fn populate_info_outside_the_hudvis_window_is_not_a_riven() {
    let mut m = RivenStateMachine::new();
    feed(
        &mut m,
        "1.0 ThemedDetailedPurchaseDialog.lua: DBG: HudVis 1",
        0.0,
    );
    let out = feed(
        &mut m,
        "1.1 ThemedDetailedPurchaseDialog.lua: PopulateInfo->/Lotus/StoreItems/Upgrades/Mods/Randomized/X",
        2500.0,
    );
    assert!(out.events.is_empty());
}

#[test]
fn the_generic_dialog_fallback_alternates_roll_and_choice() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    let generic =
        "20.0 Dialog.lua: Dialog::CreateOkCancel(description=Zyklus?, title= leftItem=/x)";
    assert_eq!(
        feed(&mut m, generic, 1000.0).events,
        [RivenEvent::RollPending {
            weapon: String::new(),
            kuva_per_roll: 0
        }]
    );
    assert_eq!(
        feed(&mut m, SEND_OK, 2000.0).events,
        [RivenEvent::RollConfirmed]
    );
    feed(&mut m, generic, 3000.0);
    assert_eq!(
        feed(&mut m, SEND_OK, 4000.0).events,
        [RivenEvent::ChoiceConfirmed]
    );
}

#[test]
fn a_localized_cycle_dialog_still_reports_its_kuva_cost() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    let fr = "20.0 Dialog.lua: Dialog::CreateOkCancel(description=Voulez-vous vraiment cycler Braton Prime pour 1\u{a0}200\u{a0}?, title= leftItem=/x)";
    assert_eq!(
        feed(&mut m, fr, 1000.0).events,
        [RivenEvent::RollPending {
            weapon: String::new(),
            kuva_per_roll: 1200
        }]
    );
}

#[test]
fn resume_rearms_over_a_screen_whose_open_marker_is_gone() {
    let mut m = RivenStateMachine::new();
    m.resume_session(T0);
    assert!(m.is_session_active());
    // Diorama is assumed ready, so a close marker closes immediately.
    assert_eq!(
        feed(&mut m, CLOSE, 100.0).events,
        [RivenEvent::SessionClose]
    );
}

#[test]
fn force_end_clears_state_and_blocks_a_dialog_for_the_cooldown() {
    let mut m = RivenStateMachine::new();
    feed(&mut m, OPEN, 0.0);
    m.force_end_session(T0 + 1000.0);
    assert!(!m.is_session_active());
    assert!(feed(&mut m, CYCLE, 2000.0).events.is_empty());
    assert_eq!(feed(&mut m, CYCLE, 7000.0).events.len(), 1);
}
