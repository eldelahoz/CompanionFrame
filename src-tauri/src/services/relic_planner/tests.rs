use super::*;

fn det(era: &str, conf: f64, candidate: &str) -> EraDetection {
    EraDetection {
        era: Some(era.into()),
        confidence: conf,
        candidate: Some(candidate.into()),
        text_preview: String::new(),
    }
}

#[test]
fn void_tags_map_to_eras_and_anything_else_clears() {
    let mut p = Planner::default();
    for (tag, era) in [
        ("VoidT1", "lith"),
        ("VOIDT5", "requiem"),
        ("voidt6", "omnia"),
    ] {
        p.set_tag(tag);
        assert_eq!(p.known_era(0.0), Some(era));
    }
    p.set_tag("EndOfMission");
    assert_eq!(p.known_era(0.0), None);
}

#[test]
fn eelog_triggers_keep_900_ms_apart_but_manual_ones_do_not() {
    let mut p = Planner::default();
    let a = p.trigger(true, 10_000.0).unwrap();
    assert_eq!(p.trigger(true, 10_899.0), None);
    let b = p.trigger(true, 10_900.0).unwrap();
    assert!(
        b != a && p.is_current(b) && !p.is_current(a),
        "a new scan replaces the old"
    );
    let c = p.trigger(false, 10_901.0).unwrap();
    assert!(p.is_current(c));
}

#[test]
fn closing_mutes_the_log_for_3_s_and_invalidates_the_scan() {
    let mut p = Planner::default();
    let t = p.trigger(true, 10_000.0).unwrap();
    p.close(12_000.0);
    assert!(!p.is_current(t));
    assert_eq!(p.trigger(true, 14_999.0), None);
    assert!(p.trigger(true, 15_000.0).is_some());
    // A manual open ignores the mute.
    p.close(20_000.0);
    assert!(p.trigger(false, 20_001.0).is_some());
}

#[test]
fn init_mapping_right_after_the_open_is_entry_not_exit() {
    let mut p = Planner::default();
    assert!(!p.is_exit(5_000.0), "no picker open yet");
    p.trigger(true, 10_000.0);
    assert!(!p.is_exit(10_003.0));
    assert!(!p.is_exit(10_799.0));
    assert!(
        p.is_exit(10_800.0),
        "a fast back-out past the entry window closes"
    );
    assert!(!p.is_exit(11_000.0), "inside the 500 ms close cooldown");
}

#[test]
fn screen_tier_is_remembered_for_25_minutes_and_cleared_by_close() {
    let mut p = Planner::default();
    p.remember_screen_era("neo", 1_000.0);
    assert_eq!(p.known_era(1_000.0 + 24.0 * 60_000.0), Some("neo"));
    assert_eq!(p.known_era(1_000.0 + 25.0 * 60_000.0), None);
    p.remember_screen_era("axi", 2_000.0);
    p.close(3_000.0);
    assert_eq!(p.known_era(3_001.0), None);
}

#[test]
fn log_tier_outlives_a_picker_close_and_beats_the_screen_cache() {
    let mut p = Planner::default();
    p.set_tag("VoidT3");
    p.remember_screen_era("lith", 0.0);
    assert_eq!(p.known_era(1.0), Some("neo"));
    p.close(2.0);
    assert_eq!(p.known_era(3.0), Some("neo"));
    assert!(p.has_log_tier());
}

#[test]
fn only_a_confident_filter_label_overrides_the_tag() {
    let (era, _) = tag_or_label("lith", &det("omnia", 1.0, FILTER_LABEL));
    assert_eq!(era, "omnia");
    assert_eq!(
        tag_or_label("lith", &det("omnia", 0.8, FILTER_LABEL)).0,
        "lith"
    );
    assert_eq!(
        tag_or_label("lith", &det("meso", 1.0, "tile-slot-1")).0,
        "lith"
    );
    assert_eq!(
        tag_or_label("lith", &det("requiem", 1.0, FILTER_LABEL)).0,
        "lith"
    );
    assert_eq!(
        tag_or_label("lith", &EraDetection::default()),
        ("lith".into(), 1.0)
    );
}
