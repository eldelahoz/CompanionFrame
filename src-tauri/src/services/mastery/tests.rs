//! Ports of WFHelper's masteryProgress, masteryCategoryClassification, masteryAcquisitionSources
//! and recipeRows tests. They run on the real bundle and skip when `pnpm game-data` has not run.

use std::sync::OnceLock;

use serde_json::{Value, json};

use super::*;
use crate::services::item_db::Sources;

struct World {
    db: ItemDb,
    exports: MasteryExports,
    locale: GameLocale,
}

fn world() -> Option<&'static World> {
    static W: OnceLock<Option<World>> = OnceLock::new();
    W.get_or_init(|| {
        let root = crate::services::bundled_data::test_root()?;
        let bundled = BundledData::new(&root);
        Some(World {
            db: ItemDb::build(&Sources::load(&bundled, None)),
            exports: MasteryExports::load(&bundled),
            locale: GameLocale::default(),
        })
    })
    .as_ref()
}

fn run(inv: Value) -> Option<MasteryProgress> {
    let w = world()?;
    Some(compute_mastery_progress(
        &inv,
        &Ctx {
            db: &w.db,
            exports: &w.exports,
            locale: &w.locale,
        },
    ))
}

fn weapon_xp(rank: u32) -> u32 {
    500 * rank * rank
}
fn suit_xp(rank: u32) -> u32 {
    1_000 * rank * rank
}

fn find<'a>(p: &'a MasteryProgress, unique_name: &str) -> Option<&'a MasteryItem> {
    p.items.iter().find(|i| i.unique_name == unique_name)
}
fn named<'a>(p: &'a MasteryProgress, name: &str) -> Option<&'a MasteryItem> {
    p.items.iter().find(|i| i.name == name)
}

macro_rules! progress {
    ($inv:expr) => {
        match run($inv) {
            Some(p) => p,
            None => return,
        }
    };
}

const ACCELTRA: &str = "/Lotus/Weapons/Tenno/LongGuns/PrimeAcceltra/PrimeAcceltraWeapon";
const EXCALIBUR: &str = "/Lotus/Powersuits/Excalibur/Excalibur";
const CODA_BUBONICO: &str =
    "/Lotus/Weapons/Infested/InfestedLich/LongGuns/CodaBubonico/CodaBubonicoCannon";
const MOTE_AMP: &str =
    "/Lotus/Weapons/Sentients/OperatorAmplifiers/SentTrainingAmplifier/OperatorTrainingAmpWeapon";
const MOTE_PRISM: &str =
    "/Lotus/Weapons/Sentients/OperatorAmplifiers/SentTrainingAmplifier/SentAmpTrainingBarrel";

#[test]
fn forma_reset_mastered_gear_is_not_in_progress() {
    let p = progress!(json!({
        "LongGuns": [{ "ItemType": ACCELTRA, "XP": 0 }],
        "XPInfo": [{ "ItemType": ACCELTRA, "XP": weapon_xp(30) }],
    }));
    let item = find(&p, ACCELTRA).unwrap();
    assert_eq!(item.rank, 0);
    assert!(item.status == MasteryStatus::Mastered);
    assert_eq!(p.stats.in_progress, 0);
    // Mastery credit stays banked at the historical rank, not the reset one.
    assert_eq!(item.mastery_xp, 3_000.0);
}

#[test]
fn partially_releveled_forma_gear_credits_its_highest_rank() {
    let p = progress!(json!({
        "LongGuns": [{ "ItemType": ACCELTRA, "XP": weapon_xp(10) }],
        "XPInfo": [{ "ItemType": ACCELTRA, "XP": weapon_xp(22) }],
    }));
    let item = find(&p, ACCELTRA).unwrap();
    assert_eq!(item.rank, 10);
    assert!(item.status == MasteryStatus::Progress);
    assert_eq!(item.mastery_xp, 2_200.0);
    assert_eq!(item.mastery_xp_remaining, 800.0);
}

#[test]
fn vinquibus_evidence_merges_into_one_item() {
    let vinquibus = "/Lotus/Weapons/Tenno/Bayonet/TnBayonetRifleWeapon";
    let melee = "/Lotus/Weapons/Tenno/Bayonet/TnBayonetMeleeWeapon";
    let p = progress!(json!({
        "Melee": [{ "ItemType": melee, "XP": 0 }],
        "XPInfo": [{ "ItemType": vinquibus, "XP": weapon_xp(30) }],
    }));
    let primary = find(&p, vinquibus).unwrap();
    assert!(primary.status == MasteryStatus::Mastered);
    assert_eq!(primary.mastery_xp, 3_000.0);
    assert!(find(&p, melee).is_none());
}

#[test]
fn unmastered_owned_gear_below_max_rank_is_in_progress() {
    let p = progress!(json!({ "LongGuns": [{ "ItemType": ACCELTRA, "XP": weapon_xp(1) }] }));
    let item = find(&p, ACCELTRA).unwrap();
    assert_eq!(item.rank, 1);
    assert!(item.status == MasteryStatus::Progress);

    let sarofang = "/Lotus/Weapons/Tenno/Melee/Axe/PrimeVorunaAxeWeapon";
    let p = progress!(json!({ "Melee": [{ "ItemType": sarofang, "XP": weapon_xp(21) }] }));
    let item = find(&p, sarofang).unwrap();
    assert_eq!(item.rank, 21);
    assert!(item.status == MasteryStatus::Progress);
}

#[test]
fn coda_weapons_are_rank_40_and_read_overcap_feature_rank() {
    let p = progress!(json!({
        "LongGuns": [{ "ItemType": CODA_BUBONICO, "XP": 0, "Features": 35, "Polarized": 3 }],
    }));
    let item = find(&p, CODA_BUBONICO).unwrap();
    assert_eq!(item.rank, 36);
    assert_eq!(item.max_rank, 40);
    assert!(item.status == MasteryStatus::Progress);
}

#[test]
fn rank_40_mastery_is_capped_by_polarizations() {
    let p = progress!(json!({
        "LongGuns": [{ "ItemType": CODA_BUBONICO, "XP": weapon_xp(36), "Features": 35, "Polarized": 1 }],
        "XPInfo": [{ "ItemType": CODA_BUBONICO, "XP": weapon_xp(32) }],
    }));
    let item = find(&p, CODA_BUBONICO).unwrap();
    assert_eq!(item.rank, 32);
    assert_eq!(item.mastery_xp, 3_200.0);
    assert_eq!(item.mastery_xp_remaining, 800.0);
}

#[test]
fn modular_pets_credit_their_model_heads_without_phantom_chassis() {
    let p = progress!(json!({
        "MoaPets": [
            {
                "ItemType": "/Lotus/Types/Friendly/Pets/MoaPets/MoaPetPowerSuit",
                "XP": suit_xp(30), "Features": 8,
                "ModularParts": ["/Lotus/Types/Friendly/Pets/MoaPets/MoaPetParts/MoaPetHeadLambeo"],
            },
            // Hound builds without ModularParts still credit via the suit-model alias.
            { "ItemType": "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetAPowerSuit", "XP": suit_xp(30), "Features": 8 },
        ],
        // Plexus accrues suit-rate affinity (200 mastery per rank).
        "XPInfo": [{ "ItemType": "/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness", "XP": suit_xp(30) }],
    }));
    for name in ["Lambeo Moa", "Dorma Hound", "Plexus"] {
        assert!(
            named(&p, name).unwrap().status == MasteryStatus::Mastered,
            "{name}"
        );
    }
    assert!(named(&p, "Moa").is_none());
    assert!(named(&p, "Hound").is_none());
}

#[test]
fn ungilded_hounds_and_deimos_companions_get_no_credit() {
    let p = progress!(json!({
        "MoaPets": [{
            "ItemType": "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetCPowerSuit",
            "XP": suit_xp(30),
            "ModularParts": ["/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetParts/ZanukaPetPartHeadC"],
        }],
        "KubrowPets": [{
            "ItemType": "/Lotus/Types/Friendly/Pets/CreaturePets/ArmoredInfestedCatbrowPetPowerSuit",
            "XP": suit_xp(30),
            "ModularParts": [
                "/Lotus/Types/Friendly/Pets/CreaturePets/CreaturePetParts/Deimos/InfestedCritterAntigenB",
                "/Lotus/Types/Friendly/Pets/CreaturePets/CreaturePetParts/Deimos/InfestedCritterMutagenA",
            ],
        }],
    }));
    assert_eq!(named(&p, "Hec Hound").unwrap().mastery_xp, 0.0);
    assert_eq!(named(&p, "Panzer Vulpaphyla").unwrap().mastery_xp, 0.0);
}

#[test]
fn sold_model_heads_score_at_companion_rate() {
    let p = progress!(json!({
        "XPInfo": [
            { "ItemType": "/Lotus/Types/Friendly/Pets/MoaPets/MoaPetParts/MoaPetHeadLambeo", "XP": suit_xp(30) },
            { "ItemType": "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetParts/ZanukaPetPartHeadB", "XP": suit_xp(30) },
        ],
    }));
    assert_eq!(named(&p, "Lambeo Moa").unwrap().mastery_xp, 6_000.0);
    assert_eq!(named(&p, "Bhaira Hound").unwrap().mastery_xp, 6_000.0);
}

#[test]
fn sold_k_drives_score_at_suit_rate() {
    let deck = "/Lotus/Types/Vehicles/Hoverboard/HoverboardParts/PartComponents/HoverboardCorpusA/HoverboardCorpusADeck";
    let p = progress!(json!({ "XPInfo": [{ "ItemType": deck, "XP": suit_xp(22) }] }));
    let item = find(&p, deck).unwrap();
    assert_eq!(item.rank, 22);
    assert!(item.status == MasteryStatus::Progress);
}

#[test]
fn missing_archwings_and_k_drive_decks_have_suit_rate_mastery() {
    let p = progress!(json!({}));
    for name in ["Amesha", "Bad Baby"] {
        let item = named(&p, name).unwrap();
        assert!(item.status == MasteryStatus::Missing);
        assert_eq!(item.mastery_xp_remaining, 6_000.0, "{name}");
    }
}

#[test]
fn sold_hound_weapons_keep_weapon_rate_despite_their_pets_path() {
    let akaten = "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetMeleeWeaponPS";
    let p = progress!(json!({ "XPInfo": [{ "ItemType": akaten, "XP": weapon_xp(30) }] }));
    let item = find(&p, akaten).unwrap();
    assert_eq!(item.rank, 30);
    assert!(item.status == MasteryStatus::Mastered);
    assert_eq!(item.mastery_xp, 3_000.0);
}

#[test]
fn reports_the_mastery_an_owned_item_still_has_to_give() {
    let p = progress!(json!({ "Suits": [{ "ItemType": EXCALIBUR, "XP": suit_xp(20) }] }));
    let frame = find(&p, EXCALIBUR).unwrap();
    let unowned = p
        .items
        .iter()
        .find(|i| i.status == MasteryStatus::Missing)
        .unwrap();
    // 10 ranks left at 200 mastery each.
    assert_eq!(frame.mastery_xp_remaining, 2_000.0);
    assert!(unowned.mastery_xp_remaining > 0.0);
}

#[test]
fn hidden_profile_mastery_items_are_listed() {
    let Some(w) = world() else { return };
    let names = masterable_names(&Ctx {
        db: &w.db,
        exports: &w.exports,
        locale: &w.locale,
    });
    for n in ["Mote Prism", "Plexus", "Venari", "Venari Prime", "Sirocco"] {
        assert!(names.iter().any(|x| x == n), "{n}");
    }
    assert!(!names.iter().any(|x| x == "Mote Amp"));
}

#[test]
fn ungilded_mote_amp_grants_nothing_and_does_not_duplicate_its_prism() {
    let p = progress!(json!({
        "OperatorAmps": [{ "ItemType": MOTE_AMP, "XP": weapon_xp(28), "Features": 1, "ModularParts": [MOTE_PRISM] }],
    }));
    let prism = find(&p, MOTE_PRISM).unwrap();
    assert!(named(&p, "Mote Amp").is_none());
    assert_eq!(prism.rank, 0);
    assert_eq!(prism.mastery_xp, 0.0);
    assert_eq!(prism.needs_gilding, Some(true));
}

#[test]
fn gilded_mote_amp_credits_once_through_the_prism() {
    let p = progress!(json!({
        "OperatorAmps": [{ "ItemType": MOTE_AMP, "XP": weapon_xp(30), "Features": 9, "ModularParts": [MOTE_PRISM] }],
    }));
    let prism = find(&p, MOTE_PRISM).unwrap();
    assert!(prism.status == MasteryStatus::Mastered);
    assert_eq!(prism.mastery_xp, 3_000.0);
}

#[test]
fn sirocco_needs_no_gilding() {
    let sirocco = "/Lotus/Weapons/Operator/Pistols/DrifterPistol/DrifterPistolPlayerWeapon";
    let p = progress!(json!({
        "OperatorAmps": [{ "ItemType": sirocco, "XP": weapon_xp(30), "Features": 1 }],
    }));
    let item = find(&p, sirocco).unwrap();
    assert!(item.status == MasteryStatus::Mastered);
    assert_eq!(item.mastery_xp, 3_000.0);
}

#[test]
fn both_infested_kitgun_chambers_map_their_mastery() {
    let sporelacer =
        "/Lotus/Weapons/Infested/Pistols/InfKitGun/Barrels/InfBarrelEgg/InfModularBarrelEggPart";
    let vermisplicer =
        "/Lotus/Weapons/Infested/Pistols/InfKitGun/Barrels/InfBarrelBeam/InfModularBarrelBeamPart";
    let p = progress!(json!({
        "LongGuns": [{
            "ItemType": "/Lotus/Weapons/SolarisUnited/Primary/LotusModularPrimaryBeam",
            "XP": weapon_xp(30), "Features": 8, "ModularParts": [vermisplicer],
        }],
        "XPInfo": [{ "ItemType": sporelacer, "XP": weapon_xp(30) }],
    }));
    assert_eq!(named(&p, "Sporelacer").unwrap().mastery_xp, 3_000.0);
    assert_eq!(named(&p, "Vermisplicer").unwrap().mastery_xp, 3_000.0);
}

#[test]
fn both_venari_companions_credit_at_companion_rate() {
    let p = progress!(json!({
        "XPInfo": [
            { "ItemType": "/Lotus/Powersuits/Khora/Kavat/KhoraKavatPowerSuit", "XP": suit_xp(30) },
            { "ItemType": "/Lotus/Powersuits/Khora/Kavat/KhoraPrimeKavatPowerSuit", "XP": suit_xp(30) },
        ],
    }));
    assert_eq!(named(&p, "Venari").unwrap().mastery_xp, 6_000.0);
    assert_eq!(named(&p, "Venari Prime").unwrap().mastery_xp, 6_000.0);
}

#[test]
fn account_mastery_xp_sums_gear_nodes_junctions_and_intrinsics() {
    let p = progress!(json!({
        "PlayerLevel": 2,
        "XPInfo": [{ "ItemType": EXCALIBUR, "XP": suit_xp(30) }],
        "Missions": [
            // E Prime, masteryExp 24 in ExportRegions
            { "Tag": "SolNode27", "Completes": 1 },
            // Junctions export 0 but grant 1000; Tier 1 = Steel Path grants again
            { "Tag": "EarthToVenusJunction", "Completes": 2, "Tier": 1 },
            { "Tag": "SolNode1", "Completes": 0 },
        ],
        "PlayerSkills": { "LPS_GUNNERY": 3, "LPP_SPACE": 99_999 },
    }));
    let pm = p.stats.profile_mastery.unwrap();
    // 6000 gear + 24 node + 2000 junction + 4500 intrinsics
    assert_eq!(pm.total_xp, Some(12_524.0));
    assert_eq!(pm.rank, Some(2.0));
    assert_eq!(pm.xp_into_rank, Some(2_524.0)); // MR2 -> MR3 spans 10000..22500
    assert_eq!(pm.xp_for_next, Some(12_500.0));
    assert_eq!(pm.percent_to_next, Some(20.2));
    assert!(!pm.test_ready);
}

#[test]
fn xp_bar_is_dropped_when_the_reconstruction_undercounts_the_game_rank() {
    let p = progress!(json!({
        "PlayerLevel": 36,
        "XPInfo": [{ "ItemType": EXCALIBUR, "XP": suit_xp(30) }],
    }));
    let pm = p.stats.profile_mastery.unwrap();
    assert_eq!(pm.rank, Some(36.0));
    assert!(pm.xp_into_rank.is_none() && pm.xp_for_next.is_none() && pm.percent_to_next.is_none());
    assert!(!pm.test_ready);
}

#[test]
fn next_mastery_test_is_ready_once_xp_passes_the_threshold() {
    let p = progress!(json!({
        "PlayerLevel": 0,
        "XPInfo": [{ "ItemType": EXCALIBUR, "XP": suit_xp(30) }],
    }));
    let pm = p.stats.profile_mastery.unwrap();
    assert_eq!(pm.total_xp, Some(6_000.0));
    assert_eq!(pm.rank, Some(0.0));
    assert!(pm.test_ready);
    assert_eq!(pm.percent_to_next, Some(100.0));
}

#[test]
fn no_xp_info_falls_back_to_the_percent_fields() {
    let p = progress!(json!({ "MasteryRank": 12, "MasteryPercent": 41.26 }));
    let pm = p.stats.profile_mastery.unwrap();
    assert_eq!(
        (pm.rank, pm.percent_to_next, pm.total_xp),
        (Some(12.0), Some(41.3), None)
    );
}

// ---- recipes that consume the same ingredient twice ----

const BRONCO_PRIME: &str = "/Lotus/Weapons/Tenno/Pistol/BroncoPrime";
const AKBRONCO_PRIME: &str = "/Lotus/Weapons/Tenno/Akimbo/PrimeAkimboShotGun";
const AKBRONCO_BLUEPRINT: &str = "/Lotus/Types/Recipes/Weapons/AkbroncoPrimeBlueprint";
const AKBRONCO_LINK: &str = "/Lotus/Types/Recipes/Weapons/WeaponParts/AkbroncoPrimeLink";

fn akbronco_with_one_bronco() -> Option<Vec<MasteryComponent>> {
    let p = run(json!({
        "Pistols": [{ "ItemType": BRONCO_PRIME, "XP": weapon_xp(30) }],
        // The set spells the part bare, the inventory holds it as ...LinkBlueprint.
        "MiscItems": [{ "ItemType": format!("{AKBRONCO_LINK}Blueprint"), "ItemCount": 1 }],
        "Recipes": [{ "ItemType": AKBRONCO_BLUEPRINT, "ItemCount": 1 }],
    }))?;
    Some(find(&p, AKBRONCO_PRIME)?.components.clone())
}

#[test]
fn duplicate_ingredient_rows_merge_and_one_copy_does_not_satisfy_both_halves() {
    let Some(components) = akbronco_with_one_bronco() else {
        return;
    };
    let rows: Vec<_> = components
        .iter()
        .filter(|c| c.unique_name == BRONCO_PRIME)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].item_count, 2);
    assert_eq!(rows[0].owned_count, 1);
    assert!(!rows[0].owned);
}

#[test]
fn single_copy_ingredient_is_satisfied_under_its_inventory_spelling() {
    let Some(components) = akbronco_with_one_bronco() else {
        return;
    };
    let link = components
        .iter()
        .find(|c| c.unique_name == AKBRONCO_LINK)
        .unwrap();
    assert_eq!(link.item_count, 1);
    assert!(link.owned);
}

#[test]
fn merge_sums_by_unique_name_keeps_position_and_reads_zero_as_one() {
    use crate::services::item_db::Comp;
    let comp = |u: &str, n: u32| Comp {
        unique_name: u.into(),
        name: u.into(),
        name_key: None,
        image_name: None,
        tradable: None,
        ducats: None,
        item_count: n,
        drops: vec![],
    };
    let rows = [
        comp("/a", 0),
        comp("/b", 1),
        comp("/a", 1),
        comp("", 1),
        comp("", 1),
    ];
    let merged = merge_duplicate_ingredients(&rows);
    let shape: Vec<_> = merged
        .iter()
        .map(|c| (c.unique_name.as_str(), c.item_count))
        .collect();
    assert_eq!(shape, [("/a", 2), ("/b", 1), ("", 1), ("", 1)]);
}

// ---- category overrides ----

#[test]
fn category_overrides() {
    let Some(w) = world() else { return };
    let ctx = Ctx {
        db: &w.db,
        exports: &w.exports,
        locale: &w.locale,
    };
    let all = ctx.masterable_items();
    let get = |n: &str| all.iter().find(|m| m.name == n).unwrap();

    let bhaira = get("Bhaira Hound");
    assert_eq!(bhaira.category, "Companions");
    assert!(bhaira.debug_reason.contains("cat:override:pets"));

    let baby = get("Bad Baby");
    assert_eq!(baby.category, "Misc");
    assert!(baby.debug_reason.contains("cat:override:k-drive"));
    assert!(
        baby.keywords.iter().any(|k| k == "k-drive") && baby.keywords.iter().any(|k| k == "kdrive")
    );

    for n in [
        "Balla",
        "Cyath",
        "Dehtat",
        "Dokrahm",
        "Kronsh",
        "Mewan",
        "Ooltha",
        "Plague Keewar",
        "Plague Kripath",
        "Rabvee",
        "Sepfahn",
    ] {
        let zaw = get(n);
        assert_eq!(zaw.category, "Melee", "{n}");
        assert!(zaw.debug_reason.contains("cat:override:zaw-strike"), "{n}");
    }
}

// ---- market and dojo acquisition ----

#[test]
fn market_and_dojo_sources_reach_the_masterable_list() {
    let Some(w) = world() else { return };
    let lookup = |u: &str| w.db.get(u).unwrap();
    let mag = lookup("/Lotus/Powersuits/Mag/Mag");
    assert!(mag.market_buyable);
    assert_eq!(mag.market_credits, Some(25_000.0));
    assert_eq!(lookup(EXCALIBUR).market_credits, Some(35_000.0));
    assert_eq!(
        lookup("/Lotus/Weapons/Tenno/Melee/LongSword/LongSword").market_credits,
        Some(15_000.0)
    );
    assert_eq!(
        lookup("/Lotus/Types/Sentinels/SentinelPowersuits/TnSentinelCrossPowerSuit").market_credits,
        Some(5_000.0)
    );
    assert!(!lookup("/Lotus/Powersuits/Ninja/AshPrime").market_buyable);
    assert!(!lookup("/Lotus/Weapons/ClanTech/Energy/EnergyRifle").market_buyable);
    assert!(lookup("/Lotus/Weapons/ClanTech/Energy/EnergyRifle").dojo_research);
    assert!(
        lookup("/Lotus/Weapons/Tenno/Archwing/Primary/RocketArtillery/ArchRocketCrossbow")
            .dojo_research
    );
    assert!(!mag.dojo_research);
    // A Prime blueprint is never a credit Market purchase.
    assert!(!w.db.iter().any(|(_, i)| i.market_buyable && i.is_prime));

    let items = Ctx {
        db: &w.db,
        exports: &w.exports,
        locale: &w.locale,
    }
    .masterable_items();
    let market: Vec<_> = items.iter().filter(|i| i.market_buyable).collect();
    assert!(market.len() > 100, "{}", market.len());
    assert!(items.iter().filter(|i| i.dojo_research).count() > 60);
    assert!(market.iter().all(|i| i.market_credits.is_some()));
}

#[test]
fn a_public_profile_response_fills_mastery() {
    let raw = include_str!("../../../tests/fixtures/profile/profile_viewing_data.json");
    let payload: Value = serde_json::from_str(raw).unwrap();
    let inv = crate::services::profile::parse_profile_inventory(&payload).unwrap();
    assert_eq!(inv["XPInfo"].as_array().unwrap().len(), 2);
    let p = progress!(inv);
    // 900k XP is rank 30 for a frame; 112.5k is rank 15 for a weapon.
    assert!(find(&p, EXCALIBUR).is_some_and(|i| i.status == MasteryStatus::Mastered));
    assert!(find(&p, ACCELTRA).is_some_and(|i| i.status != MasteryStatus::Mastered));
}
