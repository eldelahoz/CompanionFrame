use serde_json::json;

use super::*;

fn ctx() -> Ctx {
    let obj = |v: Value| v.as_object().unwrap().clone();
    Ctx {
        regions: obj(json!({
            "SolNode149": {
                "name": "/Lotus/Language/Locations/Casta",
                "systemName": "/Lotus/Language/Locations/Ceres",
                "missionName": "/Lotus/Language/Missions/MissionName_Defense",
                "missionType": "MT_DEFENSE",
                "faction": "FC_GRINEER",
            },
            "SolNode167": {
                "name": "/Lotus/Language/Locations/Oestrus",
                "systemName": "/Lotus/Language/Locations/Eris",
                "missionName": "/Lotus/Language/Missions/MissionName_Purify",
                "missionType": "MT_PURIFY",
                "faction": "FC_INFESTATION",
            },
            "SolNodeVoid": { "name": "Mot", "missionType": "MT_SURVIVAL", "faction": "FC_OROKIN" },
        })),
        dict: obj(json!({
            "/Lotus/Language/Locations/Casta": "Casta",
            "/Lotus/Language/Locations/Ceres": "Ceres",
            "/Lotus/Language/Locations/Oestrus": "Oestrus",
            "/Lotus/Language/Locations/Eris": "Eris",
            "/Lotus/Language/Missions/MissionName_Defense": "DEFENSE",
            "/Lotus/Language/Missions/MissionName_Purify": "INFESTED SALVAGE",
        })),
        ..Ctx::default()
    }
}

fn entry(epoch_ms: i64, node_id: &str) -> ArbiScheduleEntry {
    ArbiScheduleEntry {
        epoch_ms: epoch_ms as f64,
        node_id: node_id.into(),
        node: "Casta (Ceres)".into(),
        mission: "Defense".into(),
        faction: "Grineer".into(),
    }
}

fn alerts() -> ArbiScheduleAlerts {
    ArbiScheduleAlerts {
        occurrences: vec![],
        favorite_nodes: vec![],
        minutes_before: 5,
    }
}

fn key(e: &ArbiScheduleEntry) -> String {
    occurrence_key(epoch_i64(e), &e.node_id)
}

#[test]
fn parses_epoch_node_lines_and_skips_garbage() {
    let text = "1751980800,SolNode149\n1751984400,SolNode167\n\nnot-a-line\nabc,SolNode1\n-5,SolNode1\n1751988000,bad node id!";
    let rows = parse_arbys_text(text);
    assert_eq!(
        rows,
        vec![
            ArbiRow {
                epoch: 1751980800,
                node_id: "SolNode149".into()
            },
            ArbiRow {
                epoch: 1751984400,
                node_id: "SolNode167".into()
            },
        ]
    );
}

#[test]
fn resolves_labels_from_region_data() {
    let rows = parse_arbys_text("1751980800,SolNode149\n1751984400,SolNode167");
    let e = build_entries(&rows, &ctx());
    assert_eq!(
        e[0],
        ArbiScheduleEntry {
            epoch_ms: 1751980800000.0,
            node_id: "SolNode149".into(),
            node: "Casta (Ceres)".into(),
            mission: "Defense".into(),
            faction: "Grineer".into(),
        }
    );
    // Uppercase dict label is title-cased; the community faction label wins over the dict.
    assert_eq!(e[1].mission, "Infested Salvage");
    assert_eq!(e[1].faction, "Infested");
}

#[test]
fn falls_back_to_mt_fc_tails_and_the_raw_node_id() {
    let rows = parse_arbys_text("1,SolNodeVoid\n2,SolNode999");
    let e = build_entries(&rows, &ctx());
    assert_eq!(e[0].mission, "Survival");
    assert_eq!(e[0].faction, "Corrupted");
    assert_eq!(e[1].node, "SolNode999");
    assert_eq!(e[1].mission, "Unknown");
}

#[test]
fn window_keeps_the_active_hour_and_sorts() {
    let (now, hour) = (10_000_000_000, 3_600_000);
    let all = [
        entry(now + hour, "A1"),
        entry(now - hour / 2, "A1"),
        entry(now - 2 * hour, "A1"),
    ];
    let got: Vec<_> = filter_window(&all, now).iter().map(epoch_i64).collect();
    assert_eq!(got, vec![now - hour / 2, now + hour]);
}

#[test]
fn due_alerts_fire_inside_the_lead_window_only() {
    let now = 20_000_000_000;
    let in_lead = entry(now + 3 * 60_000, "SolNode149");
    let outside = entry(now + 30 * 60_000, "SolNode167");
    let started = entry(now - 1, "SolNode149");
    let entries = [in_lead.clone(), outside.clone(), started];
    let a = ArbiScheduleAlerts {
        occurrences: vec![key(&in_lead), key(&outside)],
        ..alerts()
    };
    let due = due_alerts(&entries, &a, &HashSet::new(), now);
    assert_eq!(
        due.into_iter().map(|d| d.1).collect::<Vec<_>>(),
        vec![key(&in_lead)]
    );
    // Nothing belled: nothing fires, including for the already-started entry.
    assert!(due_alerts(&entries, &alerts(), &HashSet::new(), now).is_empty());
}

#[test]
fn favorites_fire_for_every_occurrence_and_respect_fired_keys() {
    let now = 20_000_000_000;
    let e = entry(now + 3 * 60_000, "SolNode149");
    let a = ArbiScheduleAlerts {
        favorite_nodes: vec!["SolNode149".into()],
        ..alerts()
    };
    assert_eq!(
        due_alerts(std::slice::from_ref(&e), &a, &HashSet::new(), now).len(),
        1
    );
    let fired = HashSet::from([key(&e)]);
    assert!(due_alerts(&[e], &a, &fired, now).is_empty());
}

#[test]
fn prunes_keys_past_the_retention_window() {
    let now = 30_000_000_000;
    let fresh = format!("{}:SolNode1", now - 60_000);
    let mut keys = vec![
        fresh.clone(),
        format!("{}:SolNode1", now - 3 * 3_600_000),
        "junk".into(),
    ];
    prune_keys(&mut keys, now);
    assert_eq!(keys, vec![fresh]);
}

#[test]
fn revive_sanitizes_a_hand_edited_store() {
    let s = AlertStore::revive(
        r#"{"occurrences":["1751980800000:SolNode149","1751980800000:SolNode149","garbage",5],
            "favoriteNodes":["SolNode167","bad id!"],"minutesBefore":500,"firedKeys":"nope"}"#,
    );
    assert_eq!(s.occurrences, vec!["1751980800000:SolNode149"]);
    assert_eq!(s.favorite_nodes, vec!["SolNode167"]);
    assert_eq!(s.minutes_before, DEFAULT_LEAD);
    assert!(s.fired_keys.is_empty());
    assert_eq!(AlertStore::revive("not json"), AlertStore::default());
}

fn service(entries: Vec<ArbiScheduleEntry>) -> ArbiSchedule {
    let s = ArbiSchedule::default();
    s.inner.lock().unwrap().entries = entries;
    s
}

#[test]
fn check_fires_a_belled_occurrence_once_and_consumes_it() {
    let now = 20_000_000_000;
    let soon = entry(now + 2 * 60_000, "SolNode149");
    let s = service(vec![soon.clone()]);
    s.edit(|st| {
        toggle(&mut st.occurrences, &key(&soon), true);
        true
    });
    let mut sent = vec![];
    let json = s.check(now, true, |t, b| sent.push((t.to_owned(), b)));
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "Arbitration Alert");
    assert!(sent[0].1.contains("Casta (Ceres)") && sent[0].1.contains("~2 min"));
    let saved = AlertStore::revive(&json.unwrap());
    assert!(saved.occurrences.is_empty());
    assert_eq!(saved.fired_keys, vec![key(&soon)]);
    assert_eq!(
        s.check(now, true, |t, b| sent.push((t.to_owned(), b))),
        None
    );
    assert_eq!(sent.len(), 1);
}

#[test]
fn check_keeps_favorites_armed_and_leaves_bells_while_undeliverable() {
    let now = 20_000_000_000;
    let (a, b) = (
        entry(now + 120_000, "SolNode149"),
        entry(now + 240_000, "SolNode149"),
    );
    let s = service(vec![a, b]);
    s.edit(|st| {
        toggle(&mut st.favorite_nodes, "SolNode149", true);
        true
    });
    let mut n = 0;
    assert_eq!(s.check(now, false, |_, _| n += 1), None);
    assert_eq!(n, 0);
    s.check(now, true, |_, _| n += 1);
    assert_eq!(n, 2);
    assert_eq!(s.payload(now).alerts.favorite_nodes, vec!["SolNode149"]);
}

#[test]
fn rejects_out_of_range_input() {
    assert!(valid_occurrence_key("1751980800000:SolNode149"));
    assert!(!valid_occurrence_key("garbage"));
    assert!(!valid_node_id("a"));
}
