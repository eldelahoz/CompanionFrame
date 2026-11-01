use serde_json::json;

use super::*;
use crate::services::item_db::Sources;

const SCINDO_BLADE: &str = "/Lotus/Types/Recipes/Weapons/WeaponParts/PrimeScindoBlade";

/// Every name resolves to `/Fixture/<name>`, except the ones in `known`.
struct Fixture;

impl RewardLookup for Fixture {
    fn by_name_or_slug(
        &self,
        name: Option<&str>,
        slug: Option<&str>,
    ) -> Option<(String, Option<u32>)> {
        if name == Some("Scindo Prime Blade") {
            return Some((SCINDO_BLADE.into(), Some(45)));
        }
        Some((format!("/Fixture/{}", name.or(slug)?), None))
    }

    fn ducats_of(&self, _: &str) -> Option<u32> {
        None
    }

    fn display_name(&self, unique_name: &str, english: &str) -> Option<String> {
        (unique_name == SCINDO_BLADE).then(|| format!("{english} (de)"))
    }
}

struct Nothing;

impl RewardLookup for Nothing {
    fn by_name_or_slug(&self, _: Option<&str>, _: Option<&str>) -> Option<(String, Option<u32>)> {
        None
    }

    fn ducats_of(&self, _: &str) -> Option<u32> {
        None
    }

    fn display_name(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

fn relic(name: &str, vaulted: bool, rewards: &[&str], drops: usize) -> Value {
    json!({
        "uniqueName": format!("{RELIC_PROJECTION_PATH}{}", name.replace(' ', "")),
        "name": name,
        "vaulted": vaulted,
        "rewards": rewards.iter().map(|r| json!({"chance": 25.33, "rarity": "Common", "item": {"name": r}})).collect::<Vec<_>>(),
        "drops": vec![json!({"location": "Void/Mithra", "chance": 0.29}); drops],
    })
}

fn rewards(db: &RelicDatabase) -> Vec<&RelicReward> {
    db.groups
        .values()
        .flat_map(|g| g.qualities.values().flat_map(|q| &q.rewards))
        .collect()
}

#[test]
fn groups_refinements_under_one_relic_and_skips_unknown_names() {
    let rows = vec![
        relic("Lith A1 Intact", false, &["Forma Blueprint"], 0),
        relic("Lith A1 Radiant", false, &["Forma Blueprint"], 0),
        relic("Neo C3 Flawless", false, &["Forma Blueprint"], 0),
        relic("Omni Z9 Intact", false, &["Forma Blueprint"], 0),
        relic("Lith A1 Pristine", false, &["Forma Blueprint"], 0),
        relic("Lith Intact", false, &["Forma Blueprint"], 0),
    ];
    let db = build(&rows, &Nothing);
    assert_eq!(db.groups.keys().collect::<Vec<_>>(), ["Lith A1", "Neo C3"]);
    let a1 = &db.groups["Lith A1"];
    assert_eq!((&*a1.tier, &*a1.code), ("Lith", "A1"));
    assert_eq!(
        a1.qualities.keys().collect::<Vec<_>>(),
        ["intact", "radiant"]
    );
    let r = &db.by_unique_name[&format!("{RELIC_PROJECTION_PATH}LithA1Radiant")];
    assert_eq!((&*r.group_key, &*r.quality), ("Lith A1", "radiant"));
}

#[test]
fn a_relic_is_vaulted_only_when_every_refinement_is_and_it_has_no_drops() {
    // The Citrine quirk: flagged vaulted while mission drops are still listed.
    let rows = vec![
        relic("Axi C12 Intact", true, &["A"], 2),
        relic("Axi V1 Intact", true, &["A"], 0),
        relic("Axi V1 Radiant", true, &["A"], 0),
        relic("Axi M1 Intact", true, &["A"], 0),
        relic("Axi M1 Radiant", false, &["A"], 0),
    ];
    let db = build(&rows, &Nothing);
    let vaulted = |k: &str| db.groups[k].vaulted;
    assert!(!vaulted("Axi C12"));
    assert!(vaulted("Axi V1"));
    assert!(!vaulted("Axi M1"));
    // A downloaded row carries dropCount instead of the drops list.
    let mut row = relic("Axi D1 Intact", true, &["A"], 0);
    row["dropCount"] = json!(3);
    assert!(!build(&[row], &Nothing).groups["Axi D1"].vaulted);
}

#[test]
fn resolves_every_reward_through_the_item_database_never_keeping_the_relics_own() {
    let mut row = relic(
        "Axi S3 Intact",
        false,
        &["Scindo Prime Blade", "Forma Blueprint"],
        0,
    );
    row["rewards"][0]["item"]["uniqueName"] = json!(format!("{RELIC_PROJECTION_PATH}AxiS3Bronze"));
    row["rewards"][1]["item"]["uniqueName"] = json!("/Lotus/Types/Items/MiscItems/Forma");
    let db = build(&[row], &Fixture);
    let got = rewards(&db);
    assert_eq!(got[0].unique_name.as_deref(), Some(SCINDO_BLADE));
    assert_eq!(
        got[0].display_name.as_deref(),
        Some("Scindo Prime Blade (de)")
    );
    assert_eq!(
        got[1].unique_name.as_deref(),
        Some("/Lotus/Types/Items/MiscItems/Forma")
    );
    assert_eq!(got[1].display_name, None);
    // Without a lookup hit the relic's own path is dropped, not leaked.
    let mut row = relic("Axi S3 Intact", false, &["Mystery"], 0);
    row["rewards"][0]["item"]["uniqueName"] = json!(format!("{RELIC_PROJECTION_PATH}AxiS3Bronze"));
    assert_eq!(rewards(&build(&[row], &Nothing))[0].unique_name, None);
}

#[test]
fn corrects_rarity_and_normalizes_the_market_fields() {
    let mut row = relic("Meso B2 Radiant", false, &["Ash Prime Systems"], 0);
    row["rewards"][0] = json!({
        "chance": 16.67, "rarity": "Uncommon",
        "item": {"name": "Ash Prime Systems", "ducats": 44.6, "imageName": "ash.png",
                 "warframeMarket": {"id": "abc", "urlName": "Ash_Prime_Systems"}}
    });
    let db = build(&[row], &Nothing);
    let r = &rewards(&db)[0];
    assert_eq!(r.rarity, "Common");
    assert_eq!(r.ducats, Some(45));
    assert_eq!(r.wfm_id.as_deref(), Some("abc"));
    assert_eq!(r.url_name.as_deref(), Some("ash_prime_systems"));
    assert!(
        r.image_url
            .as_deref()
            .unwrap()
            .starts_with("https://cdn.warframestat.us/img/")
    );
}

#[test]
fn the_intact_image_wins_the_group_image() {
    let mut radiant = relic("Lith A1 Radiant", false, &["A"], 0);
    radiant["imageName"] = json!("radiant.png");
    let mut intact = relic("Lith A1 Intact", false, &["A"], 0);
    intact["imageName"] = json!("intact.png");
    let want = wfcd_image(Some("intact.png"));
    assert_eq!(
        build(&[radiant.clone(), intact.clone()], &Nothing).groups["Lith A1"].image_url,
        want
    );
    assert_eq!(
        build(&[intact, radiant], &Nothing).groups["Lith A1"].image_url,
        want
    );
}

#[test]
fn lists_each_reward_once_vaulted_only_while_every_relic_dropping_it_is_vaulted() {
    let rows = vec![
        relic(
            "Lith A1 Intact",
            true,
            &["Alpha Prime Blueprint", "Shared Prime Barrel"],
            0,
        ),
        relic(
            "Lith A1 Radiant",
            true,
            &["Alpha Prime Blueprint", "Shared Prime Barrel"],
            0,
        ),
        relic(
            "Meso B2 Intact",
            false,
            &["Shared Prime Barrel", "Bravo Prime Stock"],
            0,
        ),
        relic("Axi D4 Intact", false, &["Forma Blueprint"], 0),
        relic("Neo C3 Intact", true, &["Forma Blueprint"], 0),
        relic(
            "Axi C12 Intact",
            true,
            &["Charlie Prime Neuroptics Blueprint"],
            2,
        ),
    ];
    let items = reward_items(&build(&rows, &Nothing), &Nothing);
    let got: Vec<_> = items.iter().map(|i| (i.name.as_str(), i.vaulted)).collect();
    assert_eq!(
        got,
        [
            ("Alpha Prime Blueprint", true),
            ("Bravo Prime Stock", false),
            ("Charlie Prime Neuroptics Blueprint", false),
            ("Forma Blueprint", false),
            ("Shared Prime Barrel", false),
        ]
    );
}

fn doc(version: &str) -> RelicDoc {
    RelicDoc {
        version: version.into(),
        published_at: Some("2026-09-25T10:00:00.000Z".into()),
        relics: vec![],
    }
}

#[test]
fn info_reports_the_downloaded_copy_until_it_is_reverted() {
    let state = RelicState::default();
    let bundled = || state.info(Some("1.1276.6".into()));
    assert_eq!(
        bundled(),
        RelicDataInfo {
            version: Some("1.1276.6".into()),
            source: RelicDataSource::Bundled,
            published_at: None,
            update_error: None,
            update_offline: false,
            newer_unusable: false,
        }
    );
    state.set_downloaded(Some(doc("1.1277.0")));
    let info = bundled();
    assert_eq!(
        (info.version.as_deref(), info.source),
        (Some("1.1277.0"), RelicDataSource::Downloaded)
    );
    assert_eq!(
        info.published_at.as_deref(),
        Some("2026-09-25T10:00:00.000Z")
    );
    state.set_downloaded(None);
    assert_eq!(bundled().source, RelicDataSource::Bundled);
}

// ---- real bundle (skipped when `pnpm game-data` has not run) ----

#[test]
fn bundled_relics_resolve_every_reward_to_the_item_not_the_relic() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/game-data");
    if !root.join("pep").exists() {
        return;
    }
    let bundled = BundledData::new(&root);
    let items = ItemDb::build(&Sources::load(&bundled, None));
    let locale = GameLocale::default();
    let rows = bundled.wfcd_items(&["Relics"]).unwrap();
    let db = build(
        &rows,
        &ItemLookup {
            db: &items,
            locale: &locale,
        },
    );

    assert!(db.groups.len() > 100, "{} groups", db.groups.len());
    let all = rewards(&db);
    assert!(all.len() > 10_000, "{} rewards", all.len());
    let leaked: Vec<_> = all
        .iter()
        .filter(|r| {
            r.unique_name
                .as_deref()
                .is_some_and(|u| u.starts_with(RELIC_PROJECTION_PATH))
        })
        .map(|r| &r.name)
        .collect();
    assert!(leaked.is_empty(), "{leaked:?}");
    let unresolved: Vec<_> = all
        .iter()
        .filter(|r| r.unique_name.is_none())
        .map(|r| &r.name)
        .collect();
    assert!(unresolved.len() < 20, "{unresolved:?}");

    let blade = db.groups["Axi S3"]
        .qualities
        .values()
        .flat_map(|q| &q.rewards)
        .find(|r| r.name == "Scindo Prime Blade")
        .unwrap();
    assert_eq!(blade.unique_name.as_deref(), Some(SCINDO_BLADE));
    assert!(
        !reward_items(
            &db,
            &ItemLookup {
                db: &items,
                locale: &locale
            }
        )
        .is_empty()
    );
}
