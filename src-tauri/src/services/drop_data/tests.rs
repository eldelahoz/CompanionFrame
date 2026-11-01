use crate::db::mem_db;

use serde_json::json;

use super::*;

fn row(item: &str, place: &str, rarity: &str, chance: f64, kind: DropKind) -> DropRow {
    DropRow {
        item: item.into(),
        place: place.into(),
        rarity: rarity.into(),
        chance: Some(chance),
        kind,
        source_url: None,
    }
}

fn flattened() -> Vec<DropRow> {
    flatten(&json!({
        "missionRewards": {"Mercury": {"Apollodorus": {"gameMode": "Survival",
            "rewards": {"C": [{"itemName": "Vitus", "rarity": "Rare", "chance": 7}]}}}},
        "relics": [
            {"tier": "Axi", "relicName": "A1", "state": "Intact",
             "rewards": [{"itemName": "Nikana Prime Blueprint", "rarity": "Rare", "chance": 2}]},
            {"tier": "Axi", "relicName": "A1", "state": "Radiant",
             "rewards": [{"itemName": "Nikana Prime Blueprint", "rarity": "Rare", "chance": 10}]}
        ],
        "modLocations": [{"modName": "Serration",
            "enemies": [{"enemyName": "Grineer Lancer", "rarity": "Common", "chance": 11.06}]}],
        "enemyModTables": [{"enemyName": "Screamer",
            "mods": [{"modName": "Vitality", "rarity": "Uncommon", "chance": 12.5}]}],
        "resourceByAvatar": [{"source": "Motherboard Cluster",
            "items": [{"item": "Techrot Motherboard", "rarity": "Common", "chance": 100}]}],
        "syndicates": {"Kahl's Garrison": [{"item": "Styanax Systems Blueprint",
            "rarity": "Common", "chance": 100, "place": "Kahl's Garrison, Encampment"}]},
        "sortieRewards": [{"itemName": "Riven Mod", "rarity": "Rare", "chance": 3}],
        "keyRewards": [{"keyName": "Archon Amar",
            "rewards": {"A": [{"itemName": "Crimson Archon Shard", "rarity": "Common", "chance": 100}]}}],
        "solarisBountyRewards": [{"bountyLevel": "Level 5 - 15 Orb Vallis Bounty",
            "rewards": {"A": [{"itemName": "Endo", "rarity": "Common", "chance": 25, "stage": "Stage 1"}]}}],
        "transientRewards": [{"objectiveName": "Arbitrations",
            "rewards": [{"itemName": "Vitus Essence", "chance": 100}]}]
    }))
}

fn find<'a>(rows: &'a [DropRow], item: &str) -> &'a DropRow {
    rows.iter().find(|r| r.item == item).unwrap()
}

#[test]
fn flattens_mission_rewards_with_rotation_in_the_place() {
    assert_eq!(
        find(&flattened(), "Vitus"),
        &row(
            "Vitus",
            "Apollodorus (Mercury), Rotation C",
            "Rare",
            7.0,
            DropKind::Mission
        )
    );
}

#[test]
fn tags_every_row_with_its_table() {
    let rows = flattened();
    for (item, kind) in [
        ("Vitus Essence", DropKind::Mission),
        ("Nikana Prime Blueprint", DropKind::Relic),
        ("Serration", DropKind::Enemy),
        ("Vitality", DropKind::Enemy),
        ("Techrot Motherboard", DropKind::Enemy),
        ("Styanax Systems Blueprint", DropKind::Syndicate),
        ("Riven Mod", DropKind::Sortie),
        ("Crimson Archon Shard", DropKind::Quest),
        ("Endo", DropKind::Bounty),
    ] {
        assert_eq!(find(&rows, item).kind, kind, "{item}");
    }
}

#[test]
fn keeps_the_bounty_level_label_as_the_place() {
    assert_eq!(
        find(&flattened(), "Endo").place,
        "Level 5 - 15 Orb Vallis Bounty, Rotation A (Stage 1)"
    );
}

#[test]
fn keeps_only_the_intact_relic_state() {
    let rows = flattened();
    let nikana: Vec<_> = rows
        .iter()
        .filter(|r| r.item == "Nikana Prime Blueprint")
        .collect();
    assert_eq!(nikana.len(), 1);
    assert_eq!(nikana[0].place, "Axi A1 Relic");
    assert_eq!(nikana[0].chance, Some(2.0));
}

#[test]
fn maps_item_to_enemy_and_enemy_to_item() {
    let rows = flattened();
    assert_eq!(find(&rows, "Serration").place, "Grineer Lancer");
    assert_eq!(find(&rows, "Vitality").place, "Screamer");
    assert_eq!(
        find(&rows, "Techrot Motherboard").place,
        "Motherboard Cluster"
    );
    assert_eq!(
        find(&rows, "Styanax Systems Blueprint").place,
        "Kahl's Garrison, Encampment"
    );
}

#[test]
fn collapses_duplicates_and_prefers_the_enemy_row() {
    let rows = flatten(&json!({
        "sortieRewards": [{"itemName": "Orokin Cell", "rarity": "Rare", "chance": 5}],
        "resourceByAvatar": [
            {"source": "Sortie", "items": [{"item": "Orokin Cell", "rarity": "Rare", "chance": 5}]}
        ]
    }));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, DropKind::Enemy);
}

#[test]
fn corrects_relic_rarity_from_refinement_and_chance() {
    let rows = flatten(
        &json!({"relics": [{"tier": "Meso", "relicName": "L5", "state": "Intact",
        "rewards": [{"itemName": "Forma Blueprint", "rarity": "Uncommon", "chance": 25.33}]}]}),
    );
    assert_eq!(rows[0].rarity, "Common");
}

fn search_rows() -> Vec<DropRow> {
    vec![
        row(
            "Vitus Essence",
            "Arbitrations, Rotation C",
            "Uncommon",
            10.0,
            DropKind::Mission,
        ),
        row(
            "Vitus Essence",
            "Arbitration Shield Drone",
            "Common",
            6.0,
            DropKind::Enemy,
        ),
        row(
            "Survivalist Vitus",
            "Elsewhere",
            "Rare",
            1.0,
            DropKind::Other,
        ),
    ]
}

#[test]
fn ranks_prefix_above_mid_word_and_returns_the_total() {
    let res = search(&search_rows(), "vitus", DropSearchMode::Item, 300);
    assert_eq!(res.total, 3);
    assert_eq!(res.rows[0].item, "Vitus Essence");
}

#[test]
fn searches_by_place_and_ignores_an_empty_query() {
    assert_eq!(
        search(&search_rows(), "arbitration", DropSearchMode::Place, 300).total,
        2
    );
    assert_eq!(
        search(&search_rows(), "  ", DropSearchMode::Item, 300).total,
        0
    );
}

#[test]
fn limit_truncates_rows_but_not_total() {
    let res = search(&search_rows(), "vitus", DropSearchMode::Item, 1);
    assert_eq!((res.rows.len(), res.total), (1, 3));
}

fn enemy_rows() -> Vec<DropRow> {
    let mut rows = search_rows();
    rows.extend([
        row(
            "Cleaving Whirlwind",
            "Arid Butcher",
            "Rare",
            5.0,
            DropKind::Enemy,
        ),
        row("Sundering Weave", "Butcher", "Rare", 1.5, DropKind::Enemy),
        row(
            "Amprex Blueprint",
            "Energy Lab",
            "Common",
            100.0,
            DropKind::Dojo,
        ),
    ]);
    rows
}

#[test]
fn enemy_mode_drops_non_enemy_rows() {
    let rows = enemy_rows();
    assert_eq!(
        search(&rows, "arbitration", DropSearchMode::Place, 300).total,
        2
    );
    let res = search(&rows, "arbitration", DropSearchMode::Enemy, 300);
    assert_eq!(res.total, 1);
    assert_eq!(res.rows[0].place, "Arbitration Shield Drone");
}

#[test]
fn enemy_mode_ranks_prefix_above_word_start() {
    let places: Vec<_> = search(&enemy_rows(), "butcher", DropSearchMode::Enemy, 300)
        .rows
        .into_iter()
        .map(|r| r.place)
        .collect();
    assert_eq!(places, ["Butcher", "Arid Butcher"]);
}

#[test]
fn dojo_word_is_searchable_but_only_in_place_mode() {
    let rows = enemy_rows();
    assert_eq!(search(&rows, "dojo", DropSearchMode::Place, 300).total, 1);
    assert_eq!(search(&rows, "dojo", DropSearchMode::Enemy, 300).total, 0);
    assert_eq!(
        search(&rows, "energy lab", DropSearchMode::Enemy, 300).total,
        0
    );
    assert_eq!(search(&rows, "vitus", DropSearchMode::Item, 300).total, 3);
}

#[test]
fn for_item_lists_stacks_best_chance_first_and_exact_names_only() {
    let rows = vec![
        row(
            "Orokin Cell",
            "Saturn Proxima",
            "Rare",
            2.5,
            DropKind::Enemy,
        ),
        row(
            "2X Orokin Cell",
            "Gabii (Ceres), Rotation C",
            "Uncommon",
            12.0,
            DropKind::Mission,
        ),
        row(
            "Orokin Cell Blueprint",
            "Nowhere",
            "Rare",
            50.0,
            DropKind::Other,
        ),
        row(
            "Tarnished Morphics",
            "Duviri",
            "Rare",
            40.0,
            DropKind::Other,
        ),
    ];
    let places: Vec<_> = for_item(&rows, " orokin cell ")
        .into_iter()
        .map(|r| r.place)
        .collect();
    assert_eq!(places, ["Gabii (Ceres), Rotation C", "Saturn Proxima"]);
    assert!(for_item(&rows, "Morphics").is_empty());
    assert!(for_item(&rows, "").is_empty());
}

#[test]
fn bundled_rows_merge_once_onto_upstream() {
    let upstream = vec![row(
        "Vitus Essence",
        "Arbitrations, Rotation C",
        "Uncommon",
        10.0,
        DropKind::Mission,
    )];
    let served = served_rows(upstream, DOJO_RESEARCH);
    let energy = search(
        &served,
        "Squad Energy Restore (Large) Blueprint",
        DropSearchMode::Item,
        300,
    );
    assert_eq!(
        energy
            .rows
            .iter()
            .filter(|r| r.kind == DropKind::Dojo)
            .count(),
        1
    );
    let veil = search(&served, "helminth archon", DropSearchMode::Item, 300);
    assert_eq!(veil.rows.len(), 1);
    assert_eq!(veil.rows[0].chance, None);
    assert_eq!(
        search(&served, "veilbreaker", DropSearchMode::Place, 300).total,
        1
    );
    assert_eq!(
        search(&served, "veilbreaker", DropSearchMode::Enemy, 300).total,
        0
    );
    let dojo = search(&served, "dojo", DropSearchMode::Place, 300);
    assert!(dojo.total > 0);
    assert!(
        dojo.rows
            .iter()
            .all(|r| r.kind == DropKind::Dojo && !r.place.contains("Dojo"))
    );
}

#[test]
fn dojo_table_tolerates_bad_files_and_entries() {
    assert!(dojo_rows("{ not json").is_empty());
    assert!(dojo_rows(r#"{"entries": "nope"}"#).is_empty());
    let rows = dojo_rows(
        r#"{"entries": [
            {"item": "Amprex Blueprint", "lab": "Energy Lab"},
            {"item": 5, "lab": "Energy Lab"},
            {"lab": "Energy Lab"},
            {"item": " ", "lab": "Energy Lab"},
            {"item": "Amprex Blueprint", "lab": "Energy Lab"}]}"#,
    );
    assert_eq!(
        rows,
        [row(
            "Amprex Blueprint",
            "Energy Lab",
            "Common",
            100.0,
            DropKind::Dojo
        )]
    );
}

#[test]
fn cached_rows_round_trip_and_stale_layouts_are_ignored() {
    let db = mem_db();
    let store = DropStore::default();
    assert!(!store.load_cached(&db));
    let payload = CachePayload {
        version: CACHE_VERSION,
        hash: "h".into(),
        rows: search_rows(),
    };
    cache::kv_put(&db, CACHE_KEY, &serde_json::to_string(&payload).unwrap()).unwrap();
    assert!(store.load_cached(&db));
    assert_eq!(
        store.with_rows(|r| search(r, "vitus", DropSearchMode::Item, 300).total),
        3
    );
    let stale = json!({"version": 1, "hash": "h", "rows": []}).to_string();
    cache::kv_put(&db, CACHE_KEY, &stale).unwrap();
    assert!(!DropStore::default().load_cached(&db));
}
