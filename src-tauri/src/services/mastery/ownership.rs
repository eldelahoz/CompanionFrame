//! Port of WFHelper's `componentOwnership`, `foundryPending`, `componentNames` and `masteryXp`.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

/// Collections that hold built equipment, one row per copy.
pub const EQUIPMENT_COLLECTIONS: [&str; 11] = [
    "Suits",
    "LongGuns",
    "Pistols",
    "Melee",
    "Sentinels",
    "SentinelWeapons",
    "SpaceSuits",
    "SpaceGuns",
    "SpaceMelee",
    "OperatorAmps",
    "MechSuits",
];
/// Modular collections DE keeps outside the equipment list.
pub const MODULAR_COLLECTIONS: [&str; 3] = ["Hoverboards", "MoaPets", "KubrowPets"];

fn built_gear() -> impl Iterator<Item = &'static str> {
    EQUIPMENT_COLLECTIONS
        .into_iter()
        .chain(MODULAR_COLLECTIONS)
        .chain(["SpecialItems"])
}

/// Unknown → finite number; unwraps BSON-style boxed numbers.
pub fn to_finite(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        Value::String(s) if !s.trim().is_empty() => s.parse().ok().filter(|f: &f64| f.is_finite()),
        Value::Object(o) => [
            "$numberInt",
            "$numberLong",
            "$numberDouble",
            "$numberDecimal",
            "$numberFloat",
        ]
        .iter()
        .find_map(|k| o.get(*k))
        .and_then(to_finite),
        _ => None,
    }
}

pub fn item_type(entry: &Value) -> &str {
    entry.get("ItemType").and_then(Value::as_str).unwrap_or("")
}

/// Owned copies per uniqueName. Feed it `without_foundry_pending` output.
pub fn aggregate_component_ownership(inventory: &Value) -> HashMap<String, u32> {
    let mut owned: HashMap<String, u32> = HashMap::new();
    let rows = |key: &str| inventory.get(key).and_then(Value::as_array);
    for key in ["MiscItems", "Recipes"] {
        for entry in rows(key).into_iter().flatten() {
            let t = item_type(entry);
            if t.is_empty() {
                continue;
            }
            let count = match entry.get("ItemCount") {
                Some(Value::Number(n)) => n.as_f64().map_or(1, |f| f.max(0.0) as u32),
                _ => 1,
            };
            *owned.entry(t.to_owned()).or_default() += count;
        }
    }
    // One row is one copy on built gear; an ItemCount there is not a stack.
    for key in built_gear() {
        for entry in rows(key).into_iter().flatten() {
            let t = item_type(entry);
            if !t.is_empty() {
                *owned.entry(t.to_owned()).or_default() += 1;
            }
        }
    }
    owned
}

/// uniqueName → builds the foundry is running for it, reusable ones included.
pub fn pending_recipe_counts(pending: &Value) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    for entry in pending.as_array().into_iter().flatten() {
        let t = item_type(entry);
        if !t.is_empty() {
            *counts.entry(t.to_owned()).or_default() += 1;
        }
    }
    counts
}

/// The same inventory minus the Recipes copies the foundry already spent.
/// `is_reusable` spares consumeOnUse=false blueprints, which survive their own build.
pub fn without_foundry_pending<'a>(
    data: &'a Value,
    is_reusable: &dyn Fn(&str) -> bool,
) -> Cow<'a, Value> {
    let mut pending = pending_recipe_counts(data.get("PendingRecipes").unwrap_or(&Value::Null));
    let Some(recipes) = data.get("Recipes").and_then(Value::as_array) else {
        return Cow::Borrowed(data);
    };
    if pending.is_empty() {
        return Cow::Borrowed(data);
    }
    let mut kept = Vec::with_capacity(recipes.len());
    for entry in recipes {
        let t = item_type(entry);
        let spent = if !t.is_empty() && !is_reusable(t) {
            pending.get(t).copied().unwrap_or(0)
        } else {
            0
        };
        if spent == 0 {
            kept.push(entry.clone());
            continue;
        }
        let held = entry
            .get("ItemCount")
            .and_then(Value::as_f64)
            .map_or(1, |f| f.floor().max(0.0) as u32);
        // Later stacks of the same blueprint absorb whatever this one could not.
        pending.insert(t.to_owned(), spent.saturating_sub(held));
        if held > spent {
            let mut e = entry.clone();
            e["ItemCount"] = (held - spent).into();
            kept.push(e);
        }
    }
    let mut out = data.clone();
    out["Recipes"] = Value::Array(kept);
    Cow::Owned(out)
}

static ENDS_BLUEPRINT: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)Blueprint$").unwrap());
static ENDS_COMPONENT: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)Component$").unwrap());
static RECIPE_PATH: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)/Types/Recipes/").unwrap());

pub fn component_unique_name_aliases(unique_name: &str) -> Vec<String> {
    let mut aliases = vec![unique_name.to_owned()];
    let bp = ENDS_BLUEPRINT.is_match(unique_name);
    if bp {
        aliases.push(
            ENDS_BLUEPRINT
                .replace(unique_name, "Component")
                .into_owned(),
        );
    }
    if ENDS_COMPONENT.is_match(unique_name) {
        aliases.push(
            ENDS_COMPONENT
                .replace(unique_name, "Blueprint")
                .into_owned(),
        );
    }
    // Weapon parts: the set lists .../AkbroncoPrimeLink, the inventory ...LinkBlueprint.
    if !bp && RECIPE_PATH.is_match(unique_name) {
        aliases.push(format!("{unique_name}Blueprint"));
    }
    aliases
}

/// Owned count for a set component: max across aliases, one pile with two spellings.
pub fn owned_component_count(unique_name: &str, owned: &HashMap<String, u32>) -> u32 {
    if unique_name.is_empty() {
        return 0;
    }
    component_unique_name_aliases(unique_name)
        .iter()
        .map(|a| owned.get(a).copied().unwrap_or(0))
        .max()
        .unwrap_or(0)
}

const XP_PER_RANK_SQUARED: f64 = 2_500.0;
const XP_AT_RANK_30: f64 = XP_PER_RANK_SQUARED * 900.0;
const LEGENDARY_RANK_XP: f64 = 147_500.0;

pub fn mastery_rank_to_xp(rank: i64) -> f64 {
    let r = rank as f64;
    if rank <= 30 {
        XP_PER_RANK_SQUARED * r * r
    } else {
        XP_AT_RANK_30 + (r - 30.0) * LEGENDARY_RANK_XP
    }
}

pub fn mastery_xp_to_rank(xp: f64) -> i64 {
    if xp >= XP_AT_RANK_30 {
        30 + ((xp - XP_AT_RANK_30) / LEGENDARY_RANK_XP).floor() as i64
    } else {
        (xp / XP_PER_RANK_SQUARED).sqrt().floor() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HILDRYN: &str = "/Lotus/Types/Recipes/WarframeRecipes/HildrynPrimeBlueprint";
    const FORMA: &str = "/Lotus/Types/Recipes/Components/FormaBlueprint";
    const CELL_BP: &str = "/Lotus/Types/Recipes/Components/OrokinCellResourceBlueprint";
    const CELL: &str = "/Lotus/Types/Items/MiscItems/OrokinCell";
    const LEX: &str = "/Lotus/Weapons/Tenno/Pistols/PrimeLex/PrimeLex";
    const LEX_RECEIVER: &str = "/Lotus/Types/Recipes/Weapons/WeaponParts/LexPrimeReceiver";

    fn reusable(u: &str) -> bool {
        u == CELL_BP
    }
    fn never(_: &str) -> bool {
        false
    }
    fn spent(v: Value, f: &dyn Fn(&str) -> bool) -> Value {
        without_foundry_pending(&v, f).into_owned()["Recipes"].clone()
    }

    #[test]
    fn built_gear_counts_one_copy_per_row() {
        let owned = aggregate_component_ownership(&json!({
            "MiscItems": [{ "ItemType": LEX_RECEIVER, "ItemCount": 2 }],
            "Pistols": [{ "ItemType": LEX, "XP": 4053738 }, { "ItemType": LEX }],
            "Suits": [{ "ItemType": "/Lotus/Powersuits/Rhino/Rhino", "ItemCount": 7 }],
        }));
        assert_eq!(owned[LEX], 2);
        assert_eq!(owned[LEX_RECEIVER], 2);
        assert_eq!(owned["/Lotus/Powersuits/Rhino/Rhino"], 1);
    }

    #[test]
    fn ownership_adds_built_copies_to_stacked_parts_and_tolerates_junk() {
        let owned = aggregate_component_ownership(&json!({
            "MiscItems": [{ "ItemType": LEX, "ItemCount": 1 }],
            "Pistols": [{ "ItemType": LEX }, null],
            "Melee": "nonsense",
            "Suits": [null, {}, { "ItemType": 42 }],
        }));
        assert_eq!(owned[LEX], 2);
        assert_eq!(owned.len(), 1);
    }

    #[test]
    fn pending_counts_ignore_junk() {
        let c = pending_recipe_counts(
            &json!([{ "ItemType": FORMA }, { "ItemType": FORMA }, null, {}, { "ItemType": 7 }]),
        );
        assert_eq!(c[FORMA], 2);
        assert_eq!(c.len(), 1);
        assert!(pending_recipe_counts(&Value::Null).is_empty());
    }

    #[test]
    fn foundry_pending_spends_across_stacks() {
        let one = json!([{ "ItemType": HILDRYN }]);
        assert_eq!(
            spent(
                json!({ "Recipes": [{ "ItemType": HILDRYN, "ItemCount": 1 }], "PendingRecipes": one }),
                &never
            ),
            json!([])
        );
        let two = json!([{ "ItemType": FORMA }, { "ItemType": FORMA }]);
        assert_eq!(
            spent(
                json!({ "Recipes": [{ "ItemType": FORMA, "ItemCount": 10 }], "PendingRecipes": two }),
                &never
            ),
            json!([{ "ItemType": FORMA, "ItemCount": 8 }])
        );
        assert_eq!(
            spent(
                json!({ "Recipes": [{ "ItemType": FORMA, "ItemCount": 1 }, { "ItemType": FORMA, "ItemCount": 3 }], "PendingRecipes": two }),
                &never
            ),
            json!([{ "ItemType": FORMA, "ItemCount": 2 }])
        );
        // A missing ItemCount is one copy.
        assert_eq!(
            spent(
                json!({ "Recipes": [{ "ItemType": HILDRYN }], "PendingRecipes": one }),
                &never
            ),
            json!([])
        );
    }

    #[test]
    fn foundry_pending_returns_input_when_nothing_to_spend() {
        let idle = json!({ "Recipes": [{ "ItemType": HILDRYN, "ItemCount": 1 }] });
        assert!(matches!(
            without_foundry_pending(&idle, &never),
            Cow::Borrowed(_)
        ));
        let no_recipes = json!({ "PendingRecipes": [{ "ItemType": HILDRYN }] });
        assert!(matches!(
            without_foundry_pending(&no_recipes, &never),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn reusable_blueprints_survive_their_own_build() {
        let data = json!({
            "MiscItems": [{ "ItemType": CELL, "ItemCount": 100 }],
            "Recipes": [{ "ItemType": CELL_BP, "ItemCount": 1 }, { "ItemType": HILDRYN, "ItemCount": 1 }],
            "PendingRecipes": [{ "ItemType": CELL_BP }, { "ItemType": HILDRYN }],
        });
        let usable = without_foundry_pending(&data, &reusable);
        let owned = aggregate_component_ownership(&usable);
        assert_eq!(owned[CELL], 100);
        assert_eq!(owned[CELL_BP], 1);
        assert!(!owned.contains_key(HILDRYN));
        // Without the exemption the reusable copy is dropped too.
        let all = without_foundry_pending(&data, &never);
        assert!(!aggregate_component_ownership(&all).contains_key(CELL_BP));
    }

    #[test]
    fn aliases_cover_blueprint_component_and_bare_part_spellings() {
        let owned = HashMap::from([(format!("{LEX_RECEIVER}Blueprint"), 3)]);
        assert_eq!(owned_component_count(LEX_RECEIVER, &owned), 3);
        assert_eq!(owned_component_count("", &owned), 0);
        assert_eq!(
            component_unique_name_aliases("/Lotus/X/FooBlueprint"),
            ["/Lotus/X/FooBlueprint", "/Lotus/X/FooComponent"]
        );
    }

    #[test]
    fn mastery_rank_xp_round_trips() {
        assert_eq!(mastery_rank_to_xp(3), 22_500.0);
        assert_eq!(mastery_rank_to_xp(32), 2_250_000.0 + 295_000.0);
        for rank in [0, 1, 29, 30, 31, 36] {
            assert_eq!(mastery_xp_to_rank(mastery_rank_to_xp(rank)), rank);
        }
    }

    #[test]
    fn boxed_numbers_unwrap() {
        assert_eq!(to_finite(&json!({ "$numberInt": "5" })), Some(5.0));
        assert_eq!(to_finite(&json!(" ")), None);
        assert_eq!(to_finite(&json!("2.5")), Some(2.5));
    }
}
