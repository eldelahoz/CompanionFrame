//! Ported from WFHelper's `worldStateParser.test.ts`. Names come from the real bundled export
//! tables (`pnpm game-data`); a checkout without them skips the name-dependent assertions.

use serde_json::{Value, json};

use super::*;
use crate::services::world_state::regions::test_ctx::bundled;

const NOW: i64 = 1_800_000_000_000;

fn date(ms: i64) -> Value {
    json!({ "$date": { "$numberLong": ms.to_string() } })
}

fn window() -> Value {
    json!({ "Activation": date(NOW - 60_000), "Expiry": date(NOW + 3_600_000) })
}

fn with_window(mut v: Value) -> Value {
    for (k, w) in window().as_object().unwrap() {
        v[k] = w.clone();
    }
    v
}

fn parse(raw: Value) -> WorldState {
    let ctx = bundled().unwrap_or_default();
    parse_raw(&raw, NOW, &ctx).unwrap()
}

/// Binds the real context or skips the test when the game data is not present.
macro_rules! real_ctx {
    () => {
        match bundled() {
            Some(c) => c,
            None => return eprintln!("skipped: run `pnpm game-data`"),
        }
    };
}

fn fissure(mission_type: &str, node: &str) -> Value {
    json!({ "ActiveMissions": [{
        "Modifier": "VoidT1", "MissionType": mission_type, "Node": node, "Expiry": date(NOW + 1000),
    }] })
}

#[test]
fn preserves_unknown_baro_costs_and_joins_store_paths() {
    let parsed = parse(json!({ "VoidTraders": {
        "Activation": date(NOW), "Expiry": date(NOW + 100_000_000), "Node": "EarthHUB",
        "Manifest": [
            { "ItemType": "/Lotus/StoreItems/Weapons/MissingCosts" },
            { "ItemType": "/Lotus/StoreItems/Weapons/Free", "PrimePrice": 0, "RegularPrice": 0 },
            { "ItemType": "/Lotus/Weapons/KnownDucats", "PrimePrice": 300 },
            { "ItemType": "/Lotus/StoreItems/Weapons/KnownCredits", "RegularPrice": 150000 },
            { "ItemType": "/Lotus/StoreItems/Weapons/InvalidCosts", "PrimePrice": -1, "RegularPrice": null },
            { "ItemType": "/Lotus/Types/BaroTreasureBox" },
        ],
    } }));
    let items = parsed.void_trader.unwrap().inventory;
    let names: Vec<_> = items.iter().map(|i| i.unique_name.as_str()).collect();
    assert_eq!(
        names,
        [
            "/Lotus/Weapons/MissingCosts",
            "/Lotus/Weapons/Free",
            "/Lotus/Weapons/KnownDucats",
            "/Lotus/Weapons/KnownCredits",
            "/Lotus/Weapons/InvalidCosts"
        ]
    );
    let costs: Vec<_> = items.iter().map(|i| (i.ducats, i.credits)).collect();
    assert_eq!(
        costs,
        [
            (None, None),
            (Some(0), Some(0)),
            (Some(300), None),
            (None, Some(150_000)),
            (None, None)
        ]
    );
}

#[test]
fn parses_fissures_and_traders() {
    let parsed = parse(json!({
        "ActiveMissions": [{ "Modifier": "VoidT4", "MissionType": "MT_CAPTURE", "Node": "Marduk",
            "Expiry": date(NOW + 60_000) }],
        "VoidTraders": { "Activation": date(NOW - 60_000), "Expiry": date(NOW + 3_600_000), "Node": "EarthHUB" },
        "PrimeVaultTraders": { "Activation": date(NOW - 60_000), "Expiry": date(NOW + 7_200_000),
            "Node": "MarsHUB", "Manifest": [{ "ItemType": "/Lotus/StoreItems/Types/Items/TestItem" }] },
        "Sorties": [{ "Expiry": date(NOW + 600_000) }],
        "Descents": [],
    }));
    assert_eq!(parsed.fissures.len(), 1);
    assert_eq!(parsed.fissures[0].tier, "Axi");
    assert_eq!(parsed.fissures[0].mission_type, "Capture");
    assert_eq!(
        parsed.void_trader.unwrap().location,
        "Larunda Relay (Earth)"
    );
    let vault = parsed.vault_trader.unwrap();
    assert_eq!(vault.location, "Strata Relay (Mars)");
    assert_eq!(vault.inventory[0].item, "TestItem");
    assert!(parsed.sortie.unwrap().expiry.is_some());
}

#[test]
fn hides_expired_fissures_and_keeps_steel_path_flag() {
    let parsed = parse(json!({ "ActiveMissions": [
        { "Modifier": "VoidT1Hard", "MissionType": "MT_CAPTURE", "Node": "X", "Expiry": date(NOW + 5) },
        { "Modifier": "VoidT1", "MissionType": "MT_CAPTURE", "Node": "X", "Expiry": date(NOW - 5) },
        { "Modifier": "NotAFissure", "Node": "X", "Expiry": date(NOW + 5) },
    ] }));
    assert_eq!(parsed.fissures.len(), 1);
    assert!(parsed.fissures[0].is_hard);
    assert_eq!(parsed.fissures[0].tier, "Lith");
}

#[test]
fn derives_the_real_mission_type_for_railjack_void_storms() {
    let ctx = real_ctx!();
    let raw = json!({ "VoidStorms": [
        { "Node": "CrewBattleNode515", "ActiveMissionTier": "VoidT3", "Expiry": date(NOW + 60_000) },
    ] });
    let parsed = parse_raw(&raw, NOW, &ctx).unwrap();
    let storm = parsed.fissures.iter().find(|f| f.is_storm).unwrap();
    assert_eq!(storm.tier, "Neo");
    // Resolves the node's railjack mission instead of a hardcoded label.
    assert_eq!(storm.mission_type, "Survival");
}

#[test]
fn labels_fissures_with_des_mission_type_meanings() {
    let ctx = real_ctx!();
    let label = |mt: &str| {
        parse_raw(&fissure(mt, "UnmappedNode"), NOW, &ctx)
            .unwrap()
            .fissures[0]
            .mission_type
            .clone()
    };
    assert_eq!(label("MT_TERRITORY"), "Interception");
    assert_eq!(label("MT_PURIFY"), "Infested Salvage");
    assert_eq!(label("MT_ARTIFACT"), "Disruption");
}

#[test]
fn title_cases_the_all_caps_mission_names_de_ships() {
    let ctx = real_ctx!();
    let raw = json!({ "ActiveMissions": [
        { "Modifier": "VoidT2", "MissionType": "MT_CORRUPTION", "Node": "SolNode230", "Expiry": date(NOW + 1000) },
        // MT_RAILJACK has no name in DE's mission-type table, so the node's own ALL CAPS
        // mission name is the fallback that has to be title-cased.
        { "Modifier": "VoidT2", "MissionType": "MT_RAILJACK", "Node": "CrewBattleNode524", "Expiry": date(NOW + 1000) },
    ] });
    let parsed = parse_raw(&raw, NOW, &ctx).unwrap();
    let labels: Vec<_> = parsed
        .fissures
        .iter()
        .map(|f| f.mission_type.as_str())
        .collect();
    assert_eq!(labels, ["Void Flood", "Volatile"]);
}

#[test]
fn fallback_mission_labels_stay_in_step_with_des_export() {
    let ctx = real_ctx!();
    for (mission_type, label) in MISSION_TYPE_LABELS {
        let parsed = parse_raw(&fissure(mission_type, "UnmappedNode"), NOW, &ctx).unwrap();
        assert_eq!(&parsed.fissures[0].mission_type, label, "{mission_type}");
    }
}

#[test]
fn parses_daily_deals_and_drops_expired_ones() {
    let parsed = parse(json!({ "DailyDeals": [
        { "StoreItem": "/Lotus/StoreItems/Types/Items/TestItem", "Expiry": date(NOW + 3_600_000),
          "Discount": 50, "OriginalPrice": 150, "SalePrice": 75, "AmountTotal": 300, "AmountSold": 97 },
        { "StoreItem": "/Lotus/StoreItems/Types/Items/OldItem", "Expiry": date(NOW - 1000) },
    ] }));
    assert_eq!(parsed.daily_deals.len(), 1);
    let deal = &parsed.daily_deals[0];
    assert_eq!(deal.unique_name, "/Lotus/Types/Items/TestItem");
    assert_eq!((deal.sale_price, deal.discount, deal.sold), (75, 50, 97));
    assert!(deal.expiry.is_some());
}

#[test]
fn resolves_weapon_deals_to_their_in_game_name() {
    let ctx = real_ctx!();
    let raw = json!({ "DailyDeals": [{
        "StoreItem": "/Lotus/StoreItems/Weapons/Tenno/Melee/Glaives/Boomerang/BoomerangWeapon",
        "Expiry": date(NOW + 3_600_000),
    }] });
    let parsed = parse_raw(&raw, NOW, &ctx).unwrap();
    // "Boomerang Weapon" before ExportWeapons joined the lookup.
    assert_eq!(parsed.daily_deals[0].item, "Kestrel");
}

#[test]
fn null_input_has_no_world_state() {
    assert!(parse_raw(&Value::Null, NOW, &Ctx::default()).is_none());
    assert!(parse_raw(&json!([]), NOW, &Ctx::default()).is_none());
}

#[test]
fn resolves_sortie_variants_to_readable_node_mission_and_modifier() {
    let ctx = real_ctx!();
    let raw = json!({ "Sorties": [with_window(json!({
        "_id": { "$oid": "sortie1" }, "Boss": "SORTIE_BOSS_LEPHANTIS",
        "Variants": [
            { "missionType": "MT_SURVIVAL", "modifierType": "SORTIE_MODIFIER_ARMOR", "node": "SolNode15" },
            { "missionType": "MT_EXTERMINATION", "modifierType": "SORTIE_MODIFIER_MELEE_ONLY", "node": "SolNode11" },
            { "missionType": "MT_DEFENSE", "modifierType": "SORTIE_MODIFIER_MADE_UP", "node": "SolNode63" },
        ],
    }))] });
    let sortie = parse_raw(&raw, NOW, &ctx).unwrap().sortie.unwrap();
    // The id joins the inventory's LastSortieReward for completion tracking.
    assert_eq!(
        (sortie.id.as_str(), sortie.boss.as_str()),
        ("sortie1", "Lephantis")
    );
    let rows: Vec<_> = sortie
        .missions
        .iter()
        .map(|m| (m.node.as_str(), m.mission.as_str(), m.modifier.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            // The variant mission wins over the node's usual one (Pacific is Rescue).
            ("Pacific (Earth)", "Survival", "Enhanced Enemy Armor"),
            ("Tharsis (Mars)", "Exterminate", "Melee Only"),
            // An unmapped modifier degrades to readable text, not the raw enum.
            ("Mantle (Earth)", "Defense", "Made Up"),
        ]
    );
}

#[test]
fn parses_the_archon_hunt_and_yields_none_without_lite_sorties() {
    let ctx = real_ctx!();
    let raw = json!({ "LiteSorties": [with_window(json!({
        "_id": { "$oid": "lite1" }, "Boss": "SORTIE_BOSS_NIRA",
        "Missions": [
            { "missionType": "MT_RESCUE", "node": "SolNode25" },
            { "missionType": "MT_SURVIVAL", "node": "SolNode15" },
        ],
    }))] });
    let hunt = parse_raw(&raw, NOW, &ctx).unwrap().archon_hunt.unwrap();
    assert_eq!((hunt.id.as_str(), hunt.boss.as_str()), ("lite1", "Nira"));
    let rows: Vec<_> = hunt
        .missions
        .iter()
        .map(|m| (m.node.as_str(), m.mission.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Callisto (Jupiter)", "Rescue"),
            ("Pacific (Earth)", "Survival")
        ]
    );
    assert!(parse(json!({ "ActiveMissions": [] })).archon_hunt.is_none());
}

#[test]
fn drops_expired_nightwave_acts_but_keeps_ones_with_no_expiry() {
    let act = |id: &str, extra: Value| {
        let mut v = json!({ "_id": { "$oid": id },
            "Challenge": "/Lotus/Types/Challenges/Seasons/Daily/SeasonDailyAimGlide" });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let parsed = parse(json!({ "SeasonInfo": with_window(json!({
        "Season": 18, "Phase": 0,
        "AffiliationTag": "RadioLegionIntermission16Syndicate",
        "ActiveChallenges": [
            act("live1", window()),
            act("expired1", json!({ "Activation": date(NOW - 7_200_000), "Expiry": date(NOW - 60_000) })),
            act("undated1", json!({ "Activation": date(NOW - 60_000) })),
        ],
    })) }));
    let nightwave = parsed.nightwave.unwrap();
    let ids: Vec<_> = nightwave.challenges.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["live1", "undated1"]);
    // Carried for the standing join.
    assert_eq!(
        nightwave.affiliation_tag,
        "RadioLegionIntermission16Syndicate"
    );
}

#[test]
fn parses_the_descent_and_calendar_season_windows() {
    let parsed = parse(json!({
        "Descents": [window()],
        "KnownCalendarSeasons": [with_window(json!({ "Season": "CST_SUMMER" }))],
    }));
    assert!(parsed.descents.unwrap().expiry.is_some());
    assert_eq!(parsed.calendar_season.unwrap().season, "Summer");
    let empty = parse(json!({}));
    assert!(empty.descents.is_none() && empty.calendar_season.is_none());
}

fn calendar_days(ctx: &Ctx, days: Value) -> Vec<CalendarDay> {
    let raw = json!({ "KnownCalendarSeasons": [with_window(json!({ "Season": "CST_WINTER", "Days": days }))] });
    parse_raw(&raw, NOW, ctx)
        .unwrap()
        .calendar_season
        .unwrap()
        .days
}

fn reward_day(path: &str) -> Value {
    json!([{ "day": 13, "events": [{ "type": "CET_REWARD", "reward": path }] }])
}

#[test]
fn resolves_calendar_days_to_structured_events() {
    let ctx = real_ctx!();
    let days = calendar_days(
        &ctx,
        json!([
            { "day": 9, "events": [] },
            { "day": 13, "events": [
                { "type": "CET_REWARD", "reward": "/Lotus/StoreItems/Types/Restoratives/Consumable" },
                { "type": "CET_CHALLENGE", "challenge": "/Lotus/Types/Challenges/Calendar1999/CalendarKillEnemiesEasy" },
            ] },
        ]),
    );
    // The empty day drops; the filled one keeps one entry per event.
    assert_eq!(days.len(), 1);
    assert_eq!(days[0].day, 13);
    let kinds: Vec<_> = days[0].events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(kinds, ["reward", "challenge"]);
    assert!(days[0].events.iter().all(|e| !e.label.is_empty()));
}

#[test]
fn gives_a_calendar_challenge_both_its_title_and_its_objective() {
    let ctx = real_ctx!();
    let days = calendar_days(
        &ctx,
        json!([{ "day": 20, "events": [{ "type": "CET_CHALLENGE",
            "challenge": "/Lotus/Types/Challenges/Calendar1999/CalendarKillEnemiesEasy" }] }]),
    );
    assert_eq!(days[0].events[0].label, "Even the Odds");
    // |COUNT| resolves from the export's requiredCount, as nightwave acts do.
    assert_eq!(
        days[0].events[0].description.as_deref(),
        Some("Kill 250 Enemies")
    );
}

#[test]
fn keeps_the_reward_path_and_strips_dict_icon_tags_from_its_name() {
    let ctx = real_ctx!();
    let days = calendar_days(
        &ctx,
        reward_day("/Lotus/StoreItems/Types/Gameplay/NarmerSorties/ArchonCrystalBoreal"),
    );
    assert_eq!(
        days[0].events[0].unique_name.as_deref(),
        Some("/Lotus/Types/Gameplay/NarmerSorties/ArchonCrystalBoreal")
    );
    assert_eq!(days[0].events[0].label, "Azure Archon Shard");
}

#[test]
fn title_cases_a_shouted_reward_name() {
    let ctx = real_ctx!();
    let days = calendar_days(
        &ctx,
        reward_day("/Lotus/StoreItems/Types/Gameplay/NarmerSorties/ArchonCrystal"),
    );
    assert_eq!(days[0].events[0].label, "Archon Shard");
}

#[test]
fn prettifies_a_store_item_the_exports_do_not_carry() {
    let days = calendar_days(
        &Ctx::default(),
        reward_day("/Lotus/StoreItems/Types/Boosters/ModDropChanceBooster3DayStoreItem"),
    );
    assert_eq!(days[0].events[0].label, "Mod Drop Chance Booster 3 Day");
}

#[test]
fn emits_calendar_perk_choices_as_upgrade_events() {
    let days = calendar_days(
        &Ctx::default(),
        json!([{ "day": 24, "events": [
            { "type": "CET_UPGRADE", "upgrade": "/Lotus/Upgrades/Calendar/Armor" },
            { "type": "CET_UPGRADE", "upgrade": "/Lotus/Upgrades/Calendar/AbilityStrength" },
            { "type": "CET_UNKNOWN" },
        ] }]),
    );
    // The unknown kind drops rather than rendering a blank row.
    let labels: Vec<_> = days[0]
        .events
        .iter()
        .map(|e| (e.kind.as_str(), e.label.as_str()))
        .collect();
    assert_eq!(
        labels,
        [("upgrade", "Armor"), ("upgrade", "Ability Strength")]
    );
}

#[test]
fn resolves_nightwave_acts_flags_elites_and_degrades_unknown_challenges() {
    let ctx = real_ctx!();
    let act = |id: &str, challenge: &str, daily: bool| {
        with_window(json!({ "_id": { "$oid": id }, "Daily": daily, "Challenge": challenge }))
    };
    let raw = json!({ "SeasonInfo": with_window(json!({ "Season": 18, "Phase": 0, "ActiveChallenges": [
        act("daily1", "/Lotus/Types/Challenges/Seasons/Daily/SeasonDailyAimGlide", true),
        act("elite1", "/Lotus/Types/Challenges/Seasons/WeeklyHard/SeasonWeeklyHardRiseOfTheMachine", false),
        act("unknown1", "/Lotus/Types/Challenges/Seasons/Daily/SeasonDailyMadeUpAct", false),
        act("radiation", "/Lotus/Types/Challenges/Seasons/Daily/SeasonDailyKillEnemiesWithRadiation", false),
    ] })) });
    let nightwave = parse_raw(&raw, NOW, &ctx).unwrap().nightwave.unwrap();
    assert_eq!((nightwave.season, nightwave.phase), (18, 0));
    let [daily, elite, unknown, radiation] = &nightwave.challenges[..] else {
        panic!("expected four acts");
    };
    // The path tail joins the inventory's ChallengeProgress records.
    assert_eq!(daily.name, "SeasonDailyAimGlide");
    assert_eq!(daily.title, "Glider");
    assert_eq!(daily.description, "Kill 15 Enemies while Aim Gliding");
    assert_eq!((daily.standing, daily.required_count), (Some(1000), 15));
    assert!(daily.is_daily && !daily.is_elite);
    assert_eq!(elite.title, "Rise of the Machine");
    assert_eq!(elite.standing, Some(7000));
    assert!(!elite.is_daily && elite.is_elite);
    assert_eq!(unknown.title, "Season Daily Made Up Act");
    assert_eq!(unknown.description, "");
    assert_eq!((unknown.standing, unknown.required_count), (None, 0));
    // Damage icon tokens are removed while the objective survives.
    assert_eq!(radiation.title, "Reactor");
    assert_eq!(
        radiation.description,
        "Kill 150 Enemies with Radiation Damage."
    );
}

#[test]
fn resolves_alerts_and_drops_expired_ones() {
    let ctx = real_ctx!();
    let raw = json!({ "Alerts": [
        with_window(json!({ "_id": { "$oid": "alert1" }, "MissionInfo": {
            "location": "SolNode25", "missionType": "MT_TERRITORY", "faction": "FC_CORPUS",
            "minEnemyLevel": 1, "maxEnemyLevel": 2,
            "missionReward": { "credits": 50000,
                "countedItems": [{ "ItemType": "/Lotus/Types/Items/MiscItems/WaterFightBucks", "ItemCount": 175 }],
                "items": ["/Lotus/Types/Items/MiscItems/MadeUpThing"] },
        } })),
        { "_id": { "$oid": "expired" }, "Expiry": date(NOW - 1000),
          "MissionInfo": { "location": "SolNode15", "missionType": "MT_RESCUE" } },
    ] });
    let alerts = parse_raw(&raw, NOW, &ctx).unwrap().alerts;
    assert_eq!(alerts.len(), 1);
    let a = &alerts[0];
    // MT_TERRITORY is Interception in DE's own table, not the legacy label.
    assert_eq!(
        (a.id.as_str(), a.node.as_str(), a.mission.as_str()),
        ("alert1", "Callisto (Jupiter)", "Interception")
    );
    assert_eq!(
        (a.faction.as_str(), a.min_level, a.max_level, a.credits),
        ("Corpus", 1, 2, 50000)
    );
    let items: Vec<_> = a.items.iter().map(|i| (i.name.as_str(), i.count)).collect();
    assert_eq!(items, [("Nakak Pearls", 175), ("Made Up Thing", 1)]);
}

#[test]
fn survives_scalars_where_arrays_belong() {
    // DE has shipped scalars in array slots before; one bad field must not cost the rest.
    let ctx = real_ctx!();
    let raw = json!({
        "ActiveMissions": [],
        "Sorties": [with_window(json!({ "Boss": "SORTIE_BOSS_LEPHANTIS", "Variants": {} }))],
        "LiteSorties": [with_window(json!({ "Boss": "SORTIE_BOSS_NIRA", "Missions": "nope" }))],
        "SeasonInfo": with_window(json!({ "Season": 18, "Phase": 0, "ActiveChallenges": {} })),
        "Alerts": {},
    });
    let parsed = parse_raw(&raw, NOW, &ctx).unwrap();
    assert!(
        parsed
            .sortie
            .as_ref()
            .is_some_and(|s| s.boss == "Lephantis" && s.missions.is_empty())
    );
    assert!(
        parsed
            .archon_hunt
            .as_ref()
            .is_some_and(|s| s.boss == "Nira" && s.missions.is_empty())
    );
    assert!(
        parsed
            .nightwave
            .as_ref()
            .is_some_and(|n| n.season == 18 && n.challenges.is_empty())
    );
    assert!(parsed.alerts.is_empty());

    let malformed = json!({ "Alerts": [with_window(json!({
        "_id": { "$oid": "alert-1" },
        "MissionInfo": { "location": "SolNode25", "missionType": "MT_RESCUE", "faction": "FC_CORPUS",
            "missionReward": { "credits": 5000, "countedItems": {}, "items": "nope" } },
    }))] });
    let alerts = parse_raw(&malformed, NOW, &ctx).unwrap().alerts;
    assert_eq!(
        (alerts.len(), alerts[0].credits, alerts[0].items.len()),
        (1, 5000, 0)
    );
}

#[test]
fn payload_with_none_of_the_four_keys_parses() {
    let parsed = parse(json!({ "ActiveMissions": [] }));
    assert!(parsed.sortie.is_none() && parsed.archon_hunt.is_none() && parsed.nightwave.is_none());
    assert!(parsed.alerts.is_empty());
}

#[test]
fn parses_raw_invasions_and_syndicate_jobs() {
    let parsed = parse(json!({
        "Invasions": [
            { "_id": { "$oid": "inv1" }, "Node": "Nowhere", "Faction": "FC_GRINEER", "DefenderFaction": "FC_CORPUS",
              "Goal": 1000, "Count": 250,
              "AttackerReward": { "credits": 5000 },
              "DefenderReward": { "countedItems": [{ "ItemType": "/Lotus/Types/Items/MadeUp", "ItemCount": 2 }] } },
            { "_id": { "$oid": "done" }, "Completed": true },
        ],
        "SyndicateMissions": [{ "Tag": "CetusSyndicate", "Expiry": date(NOW + 1000), "Jobs": [
            { "jobType": "/Lotus/Types/Gameplay/Eidolon/Jobs/AttritionBountyExt", "minEnemyLevel": 5, "maxEnemyLevel": 15,
              "xpAmounts": [100, 200], "masteryReq": 3 },
            { "minEnemyLevel": 1 },
        ] }, { "Tag": "Nope", "Expiry": date(NOW + 1000), "Jobs": [{ "jobType": "x" }] }],
    }));
    assert_eq!(parsed.invasions.len(), 1);
    let inv = &parsed.invasions[0];
    assert_eq!(
        (
            inv.completion,
            inv.attacker.faction.as_str(),
            inv.vs_infestation
        ),
        (25.0, "Grineer", false)
    );
    assert_eq!(inv.attacker.reward.credits, 5000);
    assert_eq!(inv.defender.reward.counted_items[0].count, 2);
    assert_eq!(parsed.bounties.len(), 1);
    let job = &parsed.bounties[0].jobs[..];
    assert_eq!(job.len(), 1);
    assert_eq!(job[0].kind, "Attrition Bounty");
    assert_eq!((job[0].enemy_levels, job[0].min_mr), ([5, 15], 3));
    assert_eq!(job[0].standing_stages, [100, 200]);
}

#[test]
fn circuit_choices_use_the_active_schedule_window() {
    let raw = json!({ "EndlessXpSchedule": [
        { "Activation": date(NOW - 10), "Expiry": date(NOW + 10), "CategoryChoices": [
            { "Category": "EXC_NORMAL", "Choices": ["Braton", "LatoVandal"] },
            { "Category": "EXC_HARD", "Choices": [] },
        ] },
        { "Activation": date(NOW + 10), "CategoryChoices": [{ "Category": "EXC_NORMAL", "Choices": ["Later"] }] },
    ] });
    let choices = parse_circuit_choices(&raw, NOW);
    assert_eq!(choices.len(), 1);
    assert_eq!(
        (choices[0].category.as_str(), &choices[0].choices[..]),
        (
            "normal",
            &["Braton".to_owned(), "Lato Vandal".to_owned()][..]
        )
    );
}

// ---- parseBountyCycleBounties ----

fn cycle(syndicate: &str, jobs: Value) -> Value {
    json!({ "expiry": NOW, "bounties": { syndicate: jobs } })
}

#[test]
fn assigns_static_per_tier_enemy_levels() {
    let ctx = real_ctx!();
    let jobs: Vec<_> = (0..5).map(|_| json!({ "node": "SolNode1" })).collect();
    let out = parse_bounty_cycle(&cycle("ZarimanSyndicate", Value::Array(jobs)), &ctx);
    assert_eq!(out[0].syndicate, "The Holdfasts");
    let levels: Vec<_> = out[0].jobs.iter().map(|j| j.enemy_levels).collect();
    assert_eq!(levels, [[50, 55], [60, 65], [70, 75], [90, 95], [110, 115]]);
    assert_eq!(out[0].expiry.as_deref(), Some(iso_ms(NOW).as_str()));
}

#[test]
fn carries_tier_index_for_reward_pool_lookup_plus_single_stage_standing() {
    let ctx = Ctx::default();
    let out = parse_bounty_cycle(
        &cycle("HexSyndicate", json!([{ "node": "A" }, { "node": "B" }])),
        &ctx,
    );
    let jobs = &out[0].jobs;
    assert_eq!((jobs[0].tier_index, jobs[1].tier_index), (Some(0), Some(1)));
    assert_eq!(
        (&jobs[0].standing_stages[..], &jobs[1].standing_stages[..]),
        (&[1000][..], &[2000][..])
    );
    assert_eq!(jobs[0].enemy_levels, [65, 70]);
}

#[test]
fn skips_unknown_syndicates_and_falls_back_to_region_levels_past_the_tier_table() {
    let mut ctx = Ctx::default();
    ctx.regions.insert(
        "Deep".into(),
        json!({ "missionType": "MT_SURVIVAL", "minEnemyLevel": 80, "maxEnemyLevel": 90 }),
    );
    assert!(
        parse_bounty_cycle(&cycle("MadeUpSyndicate", json!([{ "node": "A" }])), &ctx).is_empty()
    );
    let jobs: Vec<_> = (0..6).map(|_| json!({ "node": "Deep" })).collect();
    let out = parse_bounty_cycle(&cycle("ZarimanSyndicate", Value::Array(jobs)), &ctx);
    let sixth = &out[0].jobs[5];
    assert_eq!(sixth.enemy_levels, [80, 90]);
    assert!(sixth.standing_stages.is_empty());
    assert_eq!(sixth.kind, "Survival");
}

#[test]
fn bounty_cycle_challenge_text_resolves_through_the_dict() {
    let mut ctx = Ctx::default();
    ctx.dict.insert(
        "/Lotus/Language/Challenges/Challenge_KillThings_Desc".into(),
        json!("Kill |COUNT| things with |ALLY|\n"),
    );
    ctx.challenges.insert(
        "/Lotus/Types/Challenges/KillThingsHard".into(),
        json!({ "requiredCount": 7 }),
    );
    let jobs = json!([{ "node": "A", "challenge": "/Lotus/Types/Challenges/KillThingsHard",
        "ally": "/Lotus/Types/Gameplay/QuincyAllyAgent" }]);
    let out = parse_bounty_cycle(&cycle("HexSyndicate", jobs), &ctx);
    assert_eq!(
        out[0].jobs[0].challenge_desc.as_deref(),
        Some("Kill 7 things with Quincy")
    );
}

#[test]
fn text_helpers() {
    assert_eq!(title_case("INFESTED SALVAGE"), "Infested Salvage");
    assert_eq!(
        pretty_path_slug("/a/b/SeasonDailyAimGlide"),
        "Season Daily Aim Glide"
    );
    assert_eq!(store_item_path("/Lotus/StoreItems/X"), "/Lotus/X");
    assert_eq!(store_item_path("/Lotus/X"), "/Lotus/X");
    assert_eq!(strip_dict_tags("<SHARD_BLUE_SIMPLE> Azure"), "Azure");
}
