//! Ports of WFHelper's itemDatabase{Aliases,Locale,Overlay,Vaulting,RendererLookup}, itemNames,
//! displayName and gameLocale tests, on small fixtures instead of the bundled data.

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::*;

fn table(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn sources(exports: &[(&str, Value)], wfcd: Value) -> Sources {
    Sources {
        exports: exports
            .iter()
            .map(|(k, v)| ((*k).to_owned(), table(v.clone())))
            .collect(),
        wfcd: wfcd.as_array().unwrap().clone(),
        ..Sources::default()
    }
}

fn de() -> GameLocale {
    GameLocale::new(
        "de",
        HashMap::from([
            ("/Lotus/Language/Mods/Serration".into(), "Einkerbung".into()),
            (
                "/Lotus/Language/Items/BlueprintAndItem".into(),
                "|ITEM| Bauplan".into(),
            ),
        ]),
    )
}

const SERRATION: &str = "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod";
const BARREL_BP: &str = "/Lotus/Types/Recipes/Weapons/WeaponParts/DuviriRifleBarrelBlueprint";
const BARREL: &str = "/Lotus/Types/Recipes/Weapons/WeaponParts/DuviriRifleBarrel";
const RIFLE: &str = "/Lotus/Weapons/Duviri/Rifle";

/// A weapon whose barrel recipe has no name of its own, so WFCD has to name it.
fn aeolak() -> Sources {
    sources(
        &[
            (
                "ExportWeapons",
                json!({ RIFLE: { "name": "/Lotus/Language/Weapons/Aeolak" } }),
            ),
            (
                "ExportRecipes",
                json!({ BARREL_BP: { "resultType": BARREL, "ingredients": [{ "ItemType": "/Lotus/X", "ItemCount": 2 }] } }),
            ),
        ],
        json!([{
            "uniqueName": RIFLE, "name": "Aeolak", "category": "Primary", "tradable": false,
            "components": [{ "uniqueName": BARREL, "name": "Barrel", "ducats": 15 }],
        }]),
    )
}

// ---- itemDatabaseAliases ----

#[test]
fn generic_blueprint_names_become_market_facing_labels() {
    let db = ItemDb::build(&aeolak());
    let bp = db.get(BARREL_BP).unwrap();
    assert_eq!(bp.name, "Aeolak Barrel Blueprint");
    assert!(!bp.name_is_fallback);
}

#[test]
fn part_blueprints_link_to_parent_and_product_via_result_type() {
    let db = ItemDb::build(&aeolak());
    let bp = db.get(BARREL_BP).unwrap();
    assert!(bp.is_build_component);
    assert_eq!(bp.component_of.as_deref(), Some(RIFLE));
    let lookup = db.renderer_lookup(&GameLocale::default());
    assert_eq!(lookup[BARREL_BP].builds_product.as_deref(), Some(BARREL));
}

#[test]
fn renderer_lookup_exposes_the_recipe_on_the_built_item() {
    let db = ItemDb::build(&aeolak());
    let recipe = db.renderer_lookup(&GameLocale::default())[BARREL]
        .recipe
        .clone();
    // The barrel entry exists only through the WFCD component, but the recipe joins on resultType.
    let recipe = recipe.expect("recipe by resultType");
    assert_eq!(recipe.blueprint_unique_name, BARREL_BP);
    assert_eq!(recipe.ingredients[0].count, 2);
}

#[test]
fn unresolved_weapon_part_tradability_stays_unknown() {
    let handle = "/Lotus/Types/Recipes/Weapons/WeaponParts/GunScytheHandle";
    let src = sources(
        &[],
        json!([{
            "uniqueName": "/Lotus/Weapons/Scythe", "name": "Corufell", "tradable": false,
            "components": [{ "uniqueName": handle, "name": "Handle", "tradable": false }],
        }]),
    );
    let item = ItemDb::build(&src);
    let handle = item.get(handle).unwrap();
    assert_eq!(handle.name, "Corufell Handle");
    assert_eq!(handle.tradable, None);
}

#[test]
fn a_part_blueprint_is_named_once_when_its_component_already_reads_blueprint() {
    let chassis = "/Lotus/Types/Recipes/WarframeRecipes/CalibanPrimeChassisBlueprint";
    let src = sources(
        &[],
        json!([{
            "uniqueName": "/Lotus/Powersuits/Sentient/CalibanPrime", "name": "Caliban Prime", "category": "Warframes",
            "components": [{ "uniqueName": chassis, "name": "Chassis Blueprint" }],
        }]),
    );
    let db = ItemDb::build(&src);
    assert_eq!(
        db.get(chassis).unwrap().name,
        "Caliban Prime Chassis Blueprint"
    );
    let doubled = db
        .renderer_lookup(&GameLocale::default())
        .into_values()
        .filter(|e| e.name.to_lowercase().ends_with("blueprint blueprint"))
        .count();
    assert_eq!(doubled, 0);
}

#[test]
fn comp_names_overlapping_the_parent_tail_are_kept_whole() {
    let pod =
        "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/ThanomechPartWeaponPodItem";
    let src = sources(
        &[],
        json!([{
            "uniqueName": "/Lotus/Mech/Bonewidow", "name": "Bonewidow",
            "components": [{ "uniqueName": pod, "name": "Bonewidow Weapon Pod" }],
        }]),
    );
    assert_eq!(
        ItemDb::build(&src).get(pod).unwrap().name,
        "Bonewidow Weapon Pod"
    );
    assert_eq!(
        build_component_display_name("Cortege", "Barrel", false),
        "Cortege Barrel"
    );
    assert_eq!(
        build_component_display_name("Mandachord", "Mandachord Body", false),
        "Mandachord Body"
    );
}

#[test]
fn standalone_tradables_keep_their_own_name_as_comps() {
    let damaged =
        "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/DamagedMechPartSystemsItem";
    let src = sources(
        &[],
        json!([
            { "uniqueName": "/Lotus/Mech/Bonewidow", "name": "Bonewidow",
              "components": [{ "uniqueName": damaged, "name": "Damaged Necramech Pod" }] },
            { "uniqueName": damaged, "name": "Damaged Necramech Pod", "category": "Misc" },
        ]),
    );
    assert_eq!(
        ItemDb::build(&src).get(damaged).unwrap().name,
        "Damaged Necramech Pod"
    );
}

#[test]
fn crafted_necramech_parts_are_tradable_but_not_their_blueprints() {
    let casing =
        "/Lotus/Types/Gameplay/InfestedMicroplanet/Resources/Mechs/NecromechPartChassisItem";
    let bp = "/Lotus/Types/Recipes/DeimosRecipes/Mechs/NecromechPartChassisBlueprint";
    let src = sources(
        &[(
            "ExportRecipes",
            json!({ bp: { "resultType": casing, "ingredients": [] } }),
        )],
        json!([{ "uniqueName": casing, "name": "Necramech Casing", "tradable": false, "category": "Misc" }]),
    );
    let db = ItemDb::build(&src);
    assert_eq!(db.get(casing).unwrap().tradable, Some(true));
    assert_ne!(db.get(bp).and_then(|b| b.tradable), Some(true));
}

#[test]
fn blueprints_named_after_a_later_loaded_result() {
    let bp = "/Lotus/Weapons/ClanTech/Energy/LargeHundredTeamEnergyBlueprint";
    let result = "/Lotus/Weapons/ClanTech/Energy/LargeHundredTeamEnergy";
    let src = sources(
        &[
            (
                "ExportRecipes",
                json!({ bp: { "resultType": result, "ingredients": [] } }),
            ),
            (
                "ExportGear",
                json!({ result: { "name": "Squad Energy Restore (Large)", "icon": "/i/e.png" } }),
            ),
        ],
        json!([]),
    );
    let db = ItemDb::build(&src);
    let item = db.get(bp).unwrap();
    assert_eq!(item.name, "Squad Energy Restore (Large) Blueprint");
    assert!(!item.name_is_fallback);
    assert!(
        item.image_url.as_deref().unwrap().starts_with("https://"),
        "inherits the result's icon"
    );
}

#[test]
fn recipe_names_and_slug_lookup_prefer_the_part_entry() {
    let bp = "/Lotus/Types/Recipes/Weapons/AkariusPrimeBlueprint";
    let weapon = "/Lotus/Weapons/Tenno/Pistols/PrimeAkarius/PrimeAkariusWeapon";
    let src = sources(
        &[
            (
                "ExportWeapons",
                json!({ "/Lotus/X/Akarius": { "name": "Akarius Prime Blueprint" } }),
            ),
            (
                "ExportRecipes",
                json!({ bp: { "resultType": weapon, "ingredients": [] } }),
            ),
        ],
        json!([
            { "uniqueName": weapon, "name": "Akarius Prime",
              "components": [{ "uniqueName": bp, "name": "Blueprint", "ducats": 100 }] },
            { "uniqueName": bp, "name": "Akarius Prime Blueprint", "ducats": 100 },
        ]),
    );
    let db = ItemDb::build(&src);
    let (u, item) = db
        .lookup_by_name_or_slug(
            Some("Akarius Prime Blueprint"),
            Some("akarius_prime_blueprint"),
        )
        .unwrap();
    assert_eq!(u, bp);
    assert_eq!(item.ducats, Some(100));
    assert_eq!(item.component_of.as_deref(), Some(weapon));
}

#[test]
fn lookup_by_name_or_slug_resolves_from_either_half() {
    let db = ItemDb::build(&aeolak());
    for (name, slug) in [
        (None, Some("aeolak_barrel_blueprint")),
        (Some("Aeolak Barrel Blueprint"), None),
    ] {
        assert_eq!(db.lookup_by_name_or_slug(name, slug).unwrap().0, BARREL_BP);
    }
    assert!(db.lookup_by_name_or_slug(None, None).is_none());
    assert!(
        db.lookup_by_name_or_slug(Some("No Such Item"), Some("no_such_item"))
            .is_none()
    );
}

#[test]
fn a_rebuilt_database_hands_back_its_own_entries() {
    let src = aeolak();
    let first = ItemDb::build(&src);
    let second = ItemDb::build(&src);
    let a = second
        .lookup_by_name_or_slug(Some("Aeolak Barrel Blueprint"), None)
        .unwrap()
        .1;
    assert!(std::ptr::eq(a, second.get(BARREL_BP).unwrap()));
    assert!(!std::ptr::eq(a, first.get(BARREL_BP).unwrap()));
}

#[test]
fn icons_are_mirrored_instead_of_exposing_upstream_urls() {
    let src = sources(
        &[(
            "ExportWeapons",
            json!({ RIFLE: { "name": "Boar", "icon": "/Lotus/Interface/boar.png" } }),
        )],
        json!([]),
    );
    let url = ItemDb::build(&src)
        .get(RIFLE)
        .unwrap()
        .image_url
        .clone()
        .unwrap();
    assert_eq!(url, "https://browse.wf/Lotus/Interface/boar.png");
    assert_eq!(public_image_url(Some("ftp://x/y.png")), None);
}

// ---- reusable blueprints ----

#[test]
fn consume_on_use_comes_straight_off_des_export() {
    let cell = "/Lotus/Types/Recipes/Components/OrokinCellResourceBlueprint";
    let forma = "/Lotus/Types/Recipes/Components/FormaBlueprint";
    let src = sources(
        &[(
            "ExportRecipes",
            json!({
                cell: { "resultType": "/Lotus/Cell", "consumeOnUse": false, "ingredients": [] },
                forma: { "resultType": "/Lotus/Forma", "ingredients": [] },
            }),
        )],
        json!([]),
    );
    let db = ItemDb::build(&src);
    assert!(db.is_reusable_blueprint(cell));
    assert!(!db.is_reusable_blueprint(forma));
    assert!(!db.is_reusable_blueprint("/Lotus/Types/Nope"));
}

#[test]
fn reusable_flag_reaches_the_renderer_on_the_blueprint_entry() {
    let cell = "/Lotus/Types/Recipes/Components/OrokinCellResourceBlueprint";
    let src = sources(
        &[(
            "ExportRecipes",
            json!({ cell: { "name": "Cell BP", "resultType": "/Lotus/Cell", "consumeOnUse": false, "ingredients": [] } }),
        )],
        json!([]),
    );
    let lookup = ItemDb::build(&src).renderer_lookup(&GameLocale::default());
    assert_eq!(lookup[cell].reusable_blueprint, Some(true));
}

#[test]
fn mods_and_arcanes_carry_des_fusion_limit_as_max_rank() {
    let src = sources(
        &[
            (
                "ExportUpgrades",
                json!({
                    "/Lotus/Mods/AugurReach": { "name": "Augur Reach", "fusionLimit": 5 },
                    "/Lotus/Mods/Serration": { "name": "Serration", "fusionLimit": 10 },
                    "/Lotus/Mods/Locked": { "name": "Locked", "fusionLimit": 0 },
                    "/Lotus/Mods/NoLimit": { "name": "No Limit" },
                }),
            ),
            (
                "ExportArcanes",
                json!({ "/Lotus/Arcane/Grace": { "name": "Arcane Grace", "fusionLimit": 5 } }),
            ),
            (
                "ExportWarframes",
                json!({ "/Lotus/Powersuits/Ash": { "name": "Ash", "fusionLimit": 3 } }),
            ),
        ],
        json!([]),
    );
    let lookup = ItemDb::build(&src).renderer_lookup(&GameLocale::default());
    let cap = |k: &str| lookup[k].max_rank;
    assert_eq!(cap("/Lotus/Mods/AugurReach"), Some(5));
    assert_eq!(cap("/Lotus/Mods/Serration"), Some(10));
    assert_eq!(cap("/Lotus/Mods/Locked"), Some(0));
    assert_eq!(cap("/Lotus/Mods/NoLimit"), None);
    assert_eq!(cap("/Lotus/Arcane/Grace"), Some(5));
    assert_eq!(cap("/Lotus/Powersuits/Ash"), None);
}

// ---- vaulting ----

#[test]
fn wfcd_vault_status_reaches_prime_parts_and_their_blueprints() {
    let frame = "/Lotus/Powersuits/Wraith/SevagothPrime";
    let chassis = "/Lotus/Types/Recipes/WarframeRecipes/SevagothPrimeChassisBlueprint";
    let open = "/Lotus/Powersuits/Sentient/CalibanPrime";
    let src = sources(
        &[],
        json!([
            { "uniqueName": frame, "name": "Sevagoth Prime", "vaulted": true,
              "components": [{ "uniqueName": chassis, "name": "Chassis Blueprint" }] },
            { "uniqueName": open, "name": "Caliban Prime" },
        ]),
    );
    let db = ItemDb::build(&src);
    assert!(db.get(frame).unwrap().vaulted);
    assert!(db.get(chassis).unwrap().vaulted);
    assert!(!db.get(open).unwrap().vaulted);
}

#[test]
fn venari_follows_khora_vaulting_and_plain_venari_does_not() {
    let khora_prime = "/Lotus/Powersuits/Khora/KhoraPrime";
    let venari_prime = "/Lotus/Powersuits/Khora/Kavat/KhoraPrimeKavatPowerSuit";
    let venari = "/Lotus/Powersuits/Khora/Kavat/KhoraKavatPowerSuit";
    let src = sources(
        &[(
            "ExportWarframes",
            json!({ khora_prime: { "name": "Khora Prime" }, venari_prime: { "name": "Venari Prime" }, venari: { "name": "Venari" } }),
        )],
        json!([{ "uniqueName": khora_prime, "name": "Khora Prime", "vaulted": true }]),
    );
    let db = ItemDb::build(&src);
    assert!(db.get(khora_prime).unwrap().vaulted);
    assert!(db.get(venari_prime).unwrap().vaulted);
    assert!(!db.get(venari).unwrap().vaulted);
}

#[test]
fn prime_sentinel_weapons_take_their_sentinels_vault_status() {
    let shade = "/Lotus/Types/Sentinels/SentinelPowersuits/PrimeShadePowerSuit";
    let laser = "/Lotus/Types/Sentinels/SentinelWeapons/PrimeBurstLaserPistol";
    let src = sources(
        &[
            (
                "ExportSentinels",
                json!({ shade: { "name": "Shade Prime", "defaultWeapon": laser } }),
            ),
            (
                "ExportWeapons",
                json!({ laser: { "name": "Burst Laser Prime" } }),
            ),
        ],
        json!([{ "uniqueName": shade, "name": "Shade Prime", "vaulted": true }]),
    );
    let db = ItemDb::build(&src);
    assert!(db.get(shade).unwrap().vaulted);
    assert!(db.get(laser).unwrap().vaulted);
}

// ---- overlay gap-fill ----

fn overlay_sources() -> Sources {
    const SHOCK: &str =
        "/Lotus/Upgrades/Mods/Pistol/Event/Nightwave/NightwaveLasGooPistolAugmentMod";
    const LITH: &str = "/Lotus/Types/Game/Projections/T1VoidProjectionProteaIvaraVaultAGold";
    const BASE: &str = "/Lotus/Types/Game/Projections/T1VoidProjection";
    const BP: &str = "/Lotus/Types/Recipes/Components/SurgeEximusBallBlueprint";
    const RESULT: &str = "/Lotus/Types/Items/Gameplay/Eximus/SurgeEximusBall";
    let mut src = sources(
        &[
            (
                "ExportUpgrades",
                json!({ "/Lotus/Bundled": { "name": "Bundled Mod" } }),
            ),
            (
                "ExportResources",
                json!({ "/Lotus/Types/Game/Projections/T1VoidProjection": { "name": "Void Projection" } }),
            ),
        ],
        json!([]),
    );
    src.overlay = [
        ("ExportUpgrades", json!({ SHOCK: { "name": "Prototype Shock Coils", "icon": "/i/s.jpg" } })),
        (
            "ExportResources",
            json!({ LITH: { "name": "Lith B12 Relic" }, BASE: { "name": "Void Projection" }, RESULT: { "name": "Surge Eximus Specter" } }),
        ),
        (
            "ExportRecipes",
            json!({ BP: { "resultType": RESULT, "buildPrice": 5000, "num": 1, "ingredients": [{ "ItemType": "/C", "ItemCount": 10 }], "creditsCost": 5 } }),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), table(v)))
    .collect();
    src
}

#[test]
fn overlay_names_a_mod_the_bundle_never_shipped() {
    let db = ItemDb::build(&overlay_sources());
    let mod_ = db
        .get("/Lotus/Upgrades/Mods/Pistol/Event/Nightwave/NightwaveLasGooPistolAugmentMod")
        .unwrap();
    assert_eq!(
        (mod_.name.as_str(), mod_.category.as_str()),
        ("Prototype Shock Coils", "Mod")
    );
    assert!(!mod_.name_is_fallback);
    assert!(
        db.get("/Lotus/Bundled").is_some(),
        "bundled rows survive the merge"
    );
}

#[test]
fn overlay_relics_listed_only_under_resources_are_filed_as_relics() {
    let db = ItemDb::build(&overlay_sources());
    let lith = db
        .get("/Lotus/Types/Game/Projections/T1VoidProjectionProteaIvaraVaultAGold")
        .unwrap();
    assert_eq!(
        (lith.name.as_str(), lith.category.as_str()),
        ("Lith B12 Relic", "Relic")
    );
    assert_eq!(
        db.get("/Lotus/Types/Game/Projections/T1VoidProjection")
            .unwrap()
            .category,
        "Resource"
    );
}

#[test]
fn overlay_recipe_builds_without_claiming_a_market_offer() {
    let db = ItemDb::build(&overlay_sources());
    let result = "/Lotus/Types/Items/Gameplay/Eximus/SurgeEximusBall";
    assert_eq!(db.get(result).unwrap().name, "Surge Eximus Specter");
    assert_eq!(
        db.renderer_lookup(&GameLocale::default())["/Lotus/Types/Recipes/Components/SurgeEximusBallBlueprint"]
            .builds_product
            .as_deref(),
        Some(result)
    );
    assert!(!db.get(result).unwrap().market_buyable);
}

#[test]
fn bundled_recipes_do_claim_a_market_offer_and_credits() {
    let result = "/Lotus/Result";
    let src = sources(
        &[
            (
                "ExportRecipes",
                json!({ "/Lotus/A": { "resultType": result, "ingredients": [], "creditsCost": 25 } }),
            ),
            ("ExportMisc", json!({ result: { "name": "Result" } })),
            (
                "ExportDojoRecipes",
                json!({ "research": { "/Lotus/A": {} } }),
            ),
        ],
        json!([]),
    );
    let item = ItemDb::build(&src);
    let item = item.get(result).unwrap();
    assert!(item.market_buyable && item.dojo_research);
    assert_eq!(item.market_credits, Some(25.0));
}

// ---- fallback name provenance ----

#[test]
fn missing_source_names_carry_fallback_provenance_without_guessing() {
    let unresolved = "/Lotus/Weapons/Test/UnresolvedExportName";
    let literal = "/Lotus/Weapons/Test/LiteralExportName";
    let mut src = Sources::default();
    src.overlay.insert(
        "ExportWeapons".into(),
        table(json!({
            unresolved: { "name": "/Lotus/Language/Test/NotInDictionary" },
            literal: { "name": "Literal Export Name" },
        })),
    );
    let db = ItemDb::build(&src);
    let item = db.get(unresolved).unwrap();
    assert_eq!(item.name, "Unresolved Export Name");
    assert!(item.name_is_fallback);
    let lookup = db.renderer_lookup(&GameLocale::default());
    assert_eq!(lookup[unresolved].name_is_fallback, Some(true));
    assert_eq!(lookup[literal].name, "Literal Export Name");
    assert_eq!(lookup[literal].name_is_fallback, None);
}

// ---- itemDatabaseLocale ----

fn mod_sources() -> Sources {
    let mut src = aeolak();
    src.exports.insert(
        "ExportUpgrades".into(),
        table(json!({
            SERRATION: { "name": "/Lotus/Language/Mods/Serration" },
            "/Lotus/Upgrades/Vit": { "name": "/Lotus/Language/Mods/Vitality" },
        })),
    );
    src.dict = table(
        json!({ "/Lotus/Language/Mods/Serration": "Serration", "/Lotus/Language/Mods/Vitality": "Vitality" }),
    );
    src
}

#[test]
fn english_ships_no_display_name_at_all() {
    let lookup = ItemDb::build(&mod_sources()).renderer_lookup(&GameLocale::default());
    assert_eq!(lookup[SERRATION].name, "Serration");
    assert!(lookup.values().all(|e| e.display_name.is_none()));
}

#[test]
fn a_game_language_adds_display_name_without_moving_the_english_join_key() {
    let lookup = ItemDb::build(&mod_sources()).renderer_lookup(&de());
    assert_eq!(lookup[SERRATION].name, "Serration");
    assert_eq!(
        lookup[SERRATION].display_name.as_deref(),
        Some("Einkerbung")
    );
    // A name a dictionary never covered, or one that did not change, stays English.
    assert_eq!(lookup["/Lotus/Upgrades/Vit"].display_name, None);
}

#[test]
fn switching_back_to_english_leaves_no_stale_names() {
    let db = ItemDb::build(&mod_sources());
    assert!(db.renderer_lookup(&de())[SERRATION].display_name.is_some());
    assert!(
        db.renderer_lookup(&GameLocale::default())[SERRATION]
            .display_name
            .is_none()
    );
}

#[test]
fn blueprint_names_localize_through_the_pattern_de_composes_them_with() {
    let db = ItemDb::build(&mod_sources());
    let lookup = db.renderer_lookup(&de());
    // The built barrel has no name key, so the pattern wraps its (English) name.
    assert_eq!(lookup[BARREL_BP].name, "Aeolak Barrel Blueprint");
    assert_eq!(
        lookup[BARREL_BP].display_name.as_deref(),
        Some("Aeolak Barrel Bauplan")
    );
    let es = GameLocale::new(
        "es",
        HashMap::from([(
            "/Lotus/Language/Items/BlueprintAndItem".into(),
            "Plano de |ITEM|".into(),
        )]),
    );
    assert_eq!(
        db.renderer_lookup(&es)[BARREL_BP].display_name.as_deref(),
        Some("Plano de Aeolak Barrel")
    );
}

#[test]
fn localized_name_covers_items_built_outside_the_lookup() {
    let db = ItemDb::build(&mod_sources());
    assert_eq!(
        db.localized_name(&de(), SERRATION, "Serration").as_deref(),
        Some("Einkerbung")
    );
    assert_eq!(
        db.localized_name(&GameLocale::default(), SERRATION, "Serration"),
        None
    );
    assert_eq!(db.localized_name(&de(), "/Lotus/Nope", "Nope"), None);
}

#[test]
fn state_caches_the_projection_per_language_and_reports_changes() {
    let state = ItemDbState::default();
    state.replace(ItemDb::build(&mod_sources()));
    assert!(Arc::ptr_eq(&state.lookup(), &state.lookup()));
    assert_eq!(
        state.set_locale(GameLocale::default()),
        None,
        "english to english is no change"
    );
    assert_eq!(
        state.set_ui_locale(GameLocale::default()),
        None,
        "english to english is no change"
    );
    assert_eq!(state.set_ui_locale(de()), Some("de"));
    assert_eq!(
        state.lookup()[SERRATION].display_name.as_deref(),
        Some("Einkerbung")
    );
}

fn es() -> GameLocale {
    GameLocale::new(
        "es",
        HashMap::from([("/Lotus/Language/Mods/Serration".into(), "Serración".into())]),
    )
}

#[test]
fn names_follow_the_app_language_while_the_game_language_stays_for_the_screen() {
    let state = ItemDbState::default();
    state.replace(ItemDb::build(&mod_sources()));
    // Game in German, app in English: the UI reads English, the scanner still gets German.
    state.set_locale(de());
    assert_eq!(state.lookup()[SERRATION].display_name, None);
    let (db, game) = state.snapshot();
    assert_eq!(
        db.localized_name(&game, SERRATION, "Serration").as_deref(),
        Some("Einkerbung")
    );
    // Game in English, app in Spanish.
    state.set_locale(GameLocale::default());
    state.set_ui_locale(es());
    assert_eq!(
        state.lookup()[SERRATION].display_name.as_deref(),
        Some("Serración")
    );
    assert_eq!(state.snapshot().1.code(), "en");
}

#[test]
fn switching_the_app_language_invalidates_the_cache_and_game_changes_do_not() {
    let state = ItemDbState::default();
    state.replace(ItemDb::build(&mod_sources()));
    let en = state.lookup();
    state.set_locale(de());
    assert!(Arc::ptr_eq(&en, &state.lookup()));
    state.set_ui_locale(es());
    let sp = state.lookup();
    assert!(!Arc::ptr_eq(&en, &sp));
    assert_eq!(sp[SERRATION].display_name.as_deref(), Some("Serración"));
}

// ---- displayName / relic rarity / slug helpers ----

#[test]
fn display_names_drop_the_leading_category_marker() {
    assert_eq!(sanitize_display_name("<ARCHWING> Amesha"), "Amesha");
    assert_eq!(sanitize_display_name("<ENDO>"), "<ENDO>");
    assert_eq!(sanitize_display_name("  Plain  "), "Plain");
    assert_eq!(
        fallback_name_from_unique_name("/Lotus/Language/Items/ArchonCrystalGreenName"),
        "Archon Crystal Green"
    );
    assert_eq!(
        fallback_name_from_unique_name("/Lotus/Weapons/UnresolvedExportName"),
        "Unresolved Export Name"
    );
    assert_eq!(fallback_name_from_unique_name(""), "Unknown");
}

#[test]
fn relic_drop_rarity_is_derived_from_refinement_and_chance() {
    assert_eq!(
        corrected_drop_rarity("Meso L5 Relic (Radiant)", 16.67, "Rare"),
        "Common"
    );
    assert_eq!(
        corrected_drop_rarity("Meso L5 Relic", 25.33, "Rare"),
        "Common"
    );
    assert_eq!(corrected_drop_rarity("Meso L5 Relic", 50.0, "Rare"), "Rare");
    assert_eq!(corrected_drop_rarity("Mission X", 25.33, "Rare"), "Rare");
}

#[test]
fn wfm_slugs_survive_but_display_names_fold() {
    assert_eq!(
        normalize_wfm_slug("akarius_prime_blueprint").as_deref(),
        Some("akarius_prime_blueprint")
    );
    assert_eq!(
        normalize_wfm_slug("zid-an_asheir").as_deref(),
        Some("zid-an_asheir")
    );
    assert_eq!(
        normalize_wfm_slug("Akarius Prime Blueprint").as_deref(),
        Some("akarius_prime_blueprint")
    );
    assert_eq!(
        normalize_wfm_slug("Rubico's Prime").as_deref(),
        Some("rubicos_prime")
    );
    assert_eq!(normalize_wfm_slug("!!"), None);
}

#[test]
fn wfcd_relic_drops_are_rarity_corrected() {
    let src = sources(
        &[],
        json!([{ "uniqueName": "/Lotus/Part", "name": "Part",
                 "drops": [{ "location": "Lith A1 Relic (Radiant)", "type": "Part", "chance": 16.67, "rarity": "Rare" }] }]),
    );
    let lookup = ItemDb::build(&src).renderer_lookup(&GameLocale::default());
    assert_eq!(lookup["/Lotus/Part"].drops[0].rarity, "Common");
}

// ---- real bundle (skipped when `pnpm game-data` has not run) ----

#[test]
fn bundled_data_builds_and_resolves_known_items() {
    let Some(root) = crate::services::bundled_data::test_root() else {
        return;
    };
    let bundled = BundledData::new(&root);
    let db = ItemDb::build(&Sources::load(&bundled, None));
    assert!(db.len() > 10_000, "{} items", db.len());
    let lookup = db.renderer_lookup(&GameLocale::default());
    assert_eq!(lookup[SERRATION].name, "Serration");
    assert_eq!(lookup[SERRATION].display_name, None);
    let de = db.renderer_lookup(&GameLocale::load("de", &bundled));
    assert_eq!(de[SERRATION].name, "Serration");
    assert_eq!(de[SERRATION].display_name.as_deref(), Some("Einkerbung"));
    assert_eq!(
        de["/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod"]
            .display_name
            .as_deref(),
        Some("Vitalität")
    );
    let (u, _) = db
        .lookup_by_name_or_slug(
            Some("Akarius Prime Blueprint"),
            Some("akarius_prime_blueprint"),
        )
        .unwrap();
    assert_eq!(u, "/Lotus/Types/Recipes/Weapons/AkariusPrimeBlueprint");
    assert!(
        db.is_reusable_blueprint("/Lotus/Types/Recipes/Components/OrokinCellResourceBlueprint")
    );
}

#[test]
fn wfcd_art_is_exposed_as_fallback_when_de_icon_wins() {
    let weapon = "/Lotus/Weapons/Tenno/LongGuns/PrimeAfentis/PrimeAfentisWeapon";
    let src = sources(
        &[(
            "ExportWeapons",
            json!({ weapon: { "name": "Afentis Prime", "icon": "/Lotus/Interface/AfentisPrime.png" } }),
        )],
        json!([{ "uniqueName": weapon, "name": "Afentis Prime", "category": "Primary", "imageName": "AfentisPrime.png" }]),
    );
    let entry = &ItemDb::build(&src).renderer_lookup(&GameLocale::default())[weapon];
    assert!(
        entry
            .image_url
            .as_deref()
            .unwrap()
            .starts_with("https://browse.wf/")
    );
    assert!(
        entry
            .fallback_image_url
            .as_deref()
            .unwrap()
            .ends_with("/AfentisPrime.png")
    );
}

#[test]
fn french_trade_dialog_names_map_back_to_english_items() {
    let Some(root) = crate::services::bundled_data::test_root() else {
        return;
    };
    let bundled = BundledData::new(&root);
    let db = ItemDb::build(&Sources::load(&bundled, None));
    let fr = db.english_by_localized(&GameLocale::load("fr", &bundled));
    for (shown, english) in [
        ("Nova Prime (Schéma)", "Nova Prime Blueprint"),
        (
            "Wukong Prime - Systèmes (Schéma)",
            "Wukong Prime Systems Blueprint",
        ),
        (
            "Nova Prime - Systèmes (Schéma)",
            "Nova Prime Systems Blueprint",
        ),
    ] {
        assert_eq!(
            fr.get(&shown.to_lowercase()).map(String::as_str),
            Some(english),
            "{shown}"
        );
    }
    assert!(db.english_by_localized(&GameLocale::default()).is_empty());
}

#[test]
fn localized_names_translate_back_through_the_blueprint_pattern() {
    let db = ItemDb::build(&mod_sources());
    let map = db.english_by_localized(&de());
    assert_eq!(map.get("einkerbung").map(String::as_str), Some("Serration"));
    assert_eq!(
        map.get("aeolak barrel bauplan").map(String::as_str),
        Some("Aeolak Barrel Blueprint")
    );
}
