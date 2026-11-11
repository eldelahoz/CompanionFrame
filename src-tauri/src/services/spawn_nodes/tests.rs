use serde_json::{Value, json};

use super::*;

fn bundle() -> Option<BundledData> {
    crate::services::bundled_data::test_root().map(BundledData::new)
}

fn catalog(code: &str) -> Option<Vec<SpawnNode>> {
    let b = bundle()?;
    Some(build(
        &b.export("ExportRegions")
            .expect("ExportRegions in the bundle"),
        &b.dict("en").expect("dict.en.json in the bundle"),
        &GameLocale::load(code, &b),
    ))
}

fn node<'a>(rows: &'a [SpawnNode], id: &str) -> &'a SpawnNode {
    rows.iter().find(|r| r.node_id == id).unwrap()
}

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn longest_level_prefix_wins_and_unknown_levels_are_none() {
    let level = |p: &str| Some(format!("{LEVEL_ROOT}{p}"));
    let tile = |id: &str, p: &str| region_tile_set(id, level(p).as_deref());
    assert_eq!(tile("X", "Proc/Zariman/ZarimanHub"), Some(None));
    assert_eq!(
        tile("X", "Proc/Zariman/ZarimanSomething"),
        Some(Some("Zariman (Tileset)"))
    );
    assert_eq!(tile("X", "Proc/Brand/New"), None);
    assert_eq!(region_tile_set("X", Some("/Elsewhere/Foo")), None);
    assert_eq!(region_tile_set("X", None), None);
    // A node override beats the level path.
    assert_eq!(tile("SolNode761", "Proc/Grineer/GrineerForest"), Some(None));
    assert_eq!(
        region_tile_set("SolNode701", None),
        Some(Some("Infested Ship"))
    );
}

#[test]
fn builds_a_node_with_localized_and_english_names() {
    let dict = obj(json!({
        "/N/Isos": "Isos", "/P/Eris": "Eris", "/M/Capture": "CAPTURE",
    }));
    let regions = obj(json!({"SolNode162": {
        "name": "/N/Isos", "systemName": "/P/Eris", "systemIndex": 9, "missionName": "/M/Capture",
        "missionType": "MT_CAPTURE", "faction": "FC_INFESTATION", "secondaryFaction": "FC_GRINEER",
        "minEnemyLevel": 32, "maxEnemyLevel": 36,
        "levelOverride": "/Lotus/Levels/Proc/Infestation/InfestedCorpusShip"
    }}));
    let de = GameLocale::new("de", [("/N/Isos".to_owned(), "Isos-DE".to_owned())].into());
    let rows = build(&regions, &dict, &de);
    assert_eq!(
        rows,
        [SpawnNode {
            node_id: "SolNode162".into(),
            node: "Isos-DE".into(),
            node_en: "Isos".into(),
            planet: "Eris".into(),
            planet_en: "Eris".into(),
            planet_index: 9,
            mission_key: "MT_CAPTURE".into(),
            mission: "Capture".into(),
            mission_en: "Capture".into(),
            faction_keys: vec!["infestation".into(), "grineer".into()],
            min_level: 32,
            max_level: 36,
            tile_set: "Infested Ship".into(),
            dark_sector: false,
        }]
    );
}

#[test]
fn skips_hubs_unfactioned_and_incomplete_nodes() {
    let dict = obj(json!({"/N": "N", "/P": "P"}));
    let base = json!({
        "name": "/N", "systemName": "/P", "systemIndex": 1, "missionType": "MT_SURVIVAL",
        "faction": "FC_CORPUS", "minEnemyLevel": 1, "maxEnemyLevel": 2,
        "levelOverride": "/Lotus/Levels/Proc/Corpus/CorpusShip"
    });
    let with = |k: &str, v: Value| {
        let mut b = base.clone();
        b[k] = v;
        b
    };
    let regions = obj(json!({
        "Ok": base,
        "Hub": with("levelOverride", json!("/Lotus/Levels/Proc/Hub/Relay")),
        "Tenno": with("faction", json!("FC_TENNO")),
        "NoLevel": with("minEnemyLevel", Value::Null),
        "Unknown": with("levelOverride", json!("/Lotus/Levels/Proc/Brand/New")),
    }));
    let rows = build(&regions, &dict, &GameLocale::default());
    assert_eq!(
        rows.iter().map(|r| r.node_id.as_str()).collect::<Vec<_>>(),
        ["Ok"]
    );
    assert_eq!(rows[0].mission, "Survival");
}

#[test]
fn every_bundled_node_is_placed_in_a_tileset_or_the_skip_list() {
    let Some(b) = bundle() else { return };
    let regions = b.export("ExportRegions").unwrap();
    assert!(regions.len() > 300);
    let unclassified: Vec<_> = regions
        .iter()
        .filter(|(id, r)| {
            region_tile_set(id, r.get("levelOverride").and_then(Value::as_str)).is_none()
        })
        .map(|(id, _)| id.as_str())
        .collect();
    assert!(unclassified.is_empty(), "{unclassified:?}");
}

#[test]
fn bundled_catalog_matches_the_wiki_rows_for_swarm_mutalist_moa() {
    let Some(rows) = catalog("en") else { return };
    let tikal = node(&rows, "ClanNode3");
    assert_eq!(
        (
            &*tikal.node,
            &*tikal.planet,
            &*tikal.mission,
            tikal.min_level,
            tikal.max_level
        ),
        ("Tikal", "Earth", "Excavation", 6, 16)
    );
    assert_eq!(tikal.tile_set, "Grineer Forest");
    assert!(tikal.dark_sector);
    let cholistan = node(&rows, "ClanNode7");
    assert_eq!(
        (&*cholistan.node, &*cholistan.planet, &*cholistan.tile_set),
        ("Cholistan", "Europa", "Corpus Ice Planet")
    );
    let isos = node(&rows, "SolNode162");
    assert_eq!(isos.mission_key, "MT_CAPTURE");
    assert_eq!((isos.min_level, isos.max_level), (32, 36));
    assert_eq!(isos.faction_keys, ["infestation"]);
    assert!(!isos.dark_sector);
    assert_eq!(node(&rows, "SolNode153").mission, "Rescue");
    let horend = node(&rows, "SolNode706");
    assert_eq!(
        (&*horend.planet, &*horend.tile_set),
        ("Deimos", "Orokin Derelict")
    );
}

#[test]
fn crossfire_nodes_list_both_factions_and_unfactioned_ones_are_left_out() {
    let Some(rows) = catalog("en") else { return };
    assert_eq!(
        node(&rows, "SolNode103").faction_keys,
        ["infestation", "grineer"]
    );
    // Testudo sits in a mapped tileset but carries no faction.
    assert!(rows.iter().all(|r| r.node_id != "SolNode720"));
}

#[test]
fn names_nodes_in_the_game_language_and_keeps_english_for_matching() {
    let (Some(de), Some(en)) = (catalog("de"), catalog("en")) else {
        return;
    };
    let tikal = node(&de, "ClanNode3");
    assert_eq!(
        (
            &*tikal.planet,
            &*tikal.planet_en,
            &*tikal.mission,
            &*tikal.mission_en
        ),
        ("Erde", "Earth", "Ausgrabung", "Excavation")
    );
    assert_eq!(node(&en, "ClanNode3").planet, "Earth");
}
