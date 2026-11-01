//! Card enrichment on the real bundle (skipped when `pnpm game-data` has not run).
use std::path::Path;
use std::sync::OnceLock;

use serde_json::json;

use super::enrich::{MasteredSet, Ownership, enrich};
use crate::services::bundled_data::BundledData;
use crate::services::game_locale::GameLocale;
use crate::services::item_db::{ItemDb, Sources};
use crate::services::relics::{ItemLookup, RelicRewardItem, build, reward_items};

struct World {
    db: ItemDb,
    rewards: Vec<RelicRewardItem>,
}

fn world() -> Option<&'static World> {
    static W: OnceLock<Option<World>> = OnceLock::new();
    W.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/game-data");
        if !root.join("pep").exists() {
            return None;
        }
        let bundled = BundledData::new(&root);
        let db = ItemDb::build(&Sources::load(&bundled, None));
        let locale = GameLocale::default();
        let relics = build(
            &bundled.wfcd_items(&["Relics"]).unwrap(),
            &ItemLookup {
                db: &db,
                locale: &locale,
            },
        );
        let rewards = reward_items(
            &relics,
            &ItemLookup {
                db: &db,
                locale: &locale,
            },
        );
        Some(World { db, rewards })
    })
    .as_ref()
}

fn reward<'a>(w: &'a World, name: &str) -> &'a RelicRewardItem {
    w.rewards.iter().find(|r| r.name == name).unwrap()
}

const SYSTEMS: &str = "Khora Prime Systems Blueprint";

#[test]
fn a_part_shows_ducats_vaulting_and_the_set_it_belongs_to() {
    let Some(w) = world() else { return };
    let r = reward(w, SYSTEMS);
    let item = enrich(r, &w.db, &Ownership::default(), None);
    assert_eq!(item.name, SYSTEMS);
    assert!(item.ducats > 0, "ducats");
    assert_eq!(item.vaulted, Some(r.vaulted));
    assert_eq!(item.set_url_name.as_deref(), Some("khora_prime_set"));
    assert!(item.set_parts.len() >= 3, "{:?}", item.set_parts);
    assert_eq!(item.set_parts.iter().filter(|p| p.is_reward).count(), 1);
    assert_eq!((item.part_owned_count, item.set_owned_count), (0, 0));
    assert!(item.part_required_count >= 1 && item.set_required_count >= 3);
}

#[test]
fn owned_copies_and_the_foundry_show_on_the_card() {
    let Some(w) = world() else { return };
    let r = reward(w, SYSTEMS);
    let blank = enrich(r, &w.db, &Ownership::default(), None);
    let part = blank.set_parts.iter().find(|p| p.is_reward).unwrap();
    // The set lists the part by its component uniqueName; find it again through the db.
    let unique =
        w.db.lookup_by_name_or_slug(Some(SYSTEMS), r.url_name.as_deref())
            .map(|(u, _)| u.to_owned())
            .unwrap();
    let inv = json!({ "MiscItems": [{ "ItemType": unique, "ItemCount": 1 }] });
    let item = enrich(
        r,
        &w.db,
        &Ownership::from_inventory(Some(&inv), &w.db),
        None,
    );
    assert_eq!(item.part_owned_count, 1, "{part:?}");
    assert_eq!(item.set_owned_count, 1);

    let building = json!({ "PendingRecipes": [{ "ItemType": unique }] });
    let item = enrich(
        r,
        &w.db,
        &Ownership::from_inventory(Some(&building), &w.db),
        None,
    );
    assert!(item.building);
}

#[test]
fn mastered_comes_from_the_parent_and_only_when_the_parent_is_masterable() {
    let Some(w) = world() else { return };
    let r = reward(w, SYSTEMS);
    let none = enrich(
        r,
        &w.db,
        &Ownership::default(),
        Some(&MasteredSet::default()),
    );
    assert_eq!(none.mastered, None);
    let set = MasteredSet::new([("x".to_string(), "khora prime".to_string(), true)]);
    let item = enrich(r, &w.db, &Ownership::default(), Some(&set));
    assert_eq!(item.mastered, Some(true));
}

#[test]
fn forma_has_no_vaulted_chip_and_no_set() {
    let Some(w) = world() else { return };
    let Some(r) = w
        .rewards
        .iter()
        .find(|r| r.name.ends_with("Forma Blueprint"))
    else {
        return;
    };
    let item = enrich(r, &w.db, &Ownership::default(), None);
    assert_eq!((item.vaulted, item.set_url_name), (None, None));
}
