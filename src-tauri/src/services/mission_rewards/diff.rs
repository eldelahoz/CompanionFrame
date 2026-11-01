//! Inventory snapshots and the gains between two of them (port of WFHelper's `missionRewardsDiff`).
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use serde_json::{Map, Value};

use super::MissionRewardItem;

/// New entries here are builds, research or trade offers in progress, not items received.
static NON_REWARD_UNIQUE_ARRAYS: LazyLock<HashSet<&str>> =
    LazyLock::new(|| ["PendingRecipes", "PendingTrades", "PersonalTechProjects"].into());

/// The parts of a raw inventory a mission can add to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub stacks: HashMap<String, f64>,
    pub unique_ids: HashMap<String, String>,
    pub credits: f64,
    pub endo: f64,
}

#[derive(Debug, Default, PartialEq)]
pub struct Delta {
    pub items: Vec<MissionRewardItem>,
    pub credits: f64,
    pub endo: f64,
}

fn finite(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn item_id(entry: &Map<String, Value>) -> Option<&str> {
    let id = entry.get("ItemId")?;
    let id = id.as_str().or_else(|| id.get("$oid")?.as_str())?;
    (!id.is_empty()).then_some(id)
}

/// Stack counts summed per uniqueName across every ItemType + ItemCount array, and every
/// ItemId-keyed entry of the other arrays.
pub fn snapshot_inventory(inventory: &Value) -> Option<Snapshot> {
    let record = inventory.as_object()?;
    let mut s = Snapshot {
        credits: finite(record.get("RegularCredits")),
        endo: finite(record.get("FusionPoints")),
        ..Snapshot::default()
    };
    for (array_name, value) in record {
        let Some(entries) = value.as_array() else {
            continue;
        };
        let counts_uniques = !NON_REWARD_UNIQUE_ARRAYS.contains(array_name.as_str());
        for entry in entries {
            let Some(item) = entry.as_object() else {
                continue;
            };
            let Some(unique_name) = item
                .get("ItemType")
                .and_then(Value::as_str)
                .filter(|n| !n.is_empty())
            else {
                continue;
            };
            if let Some(count) = item.get("ItemCount").filter(|c| c.is_number()) {
                if let Some(n) = count.as_f64().filter(|n| n.is_finite()) {
                    *s.stacks.entry(unique_name.to_owned()).or_default() += n;
                }
                continue;
            }
            if !counts_uniques {
                continue;
            }
            if let Some(id) = item_id(item) {
                s.unique_ids.insert(id.to_owned(), unique_name.to_owned());
            }
        }
    }
    Some(s)
}

/// Gains only: anything spent, sold or consumed between the reads is ignored.
pub fn diff_snapshots(before: &Snapshot, after: &Snapshot) -> Delta {
    let mut gained: BTreeMap<&str, f64> = BTreeMap::new();
    for (name, count) in &after.stacks {
        let delta = count - before.stacks.get(name).copied().unwrap_or(0.0);
        if delta > 0.0 {
            gained.insert(name, delta);
        }
    }
    for (id, name) in &after.unique_ids {
        if !before.unique_ids.contains_key(id) {
            *gained.entry(name).or_default() += 1.0;
        }
    }
    Delta {
        items: gained
            .into_iter()
            .map(|(unique_name, count)| MissionRewardItem {
                unique_name: unique_name.to_owned(),
                count: count as u32,
            })
            .collect(),
        credits: (after.credits - before.credits).max(0.0),
        endo: (after.endo - before.endo).max(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const RELIC: &str = "/Lotus/Types/Game/Projections/T1VoidProjectionTestBronze";
    const PLASTIDS: &str = "/Lotus/Types/Items/MiscItems/Plastids";
    const MOD: &str = "/Lotus/Upgrades/Mods/Test/TestMod";
    const SCULPTURE: &str = "/Lotus/Types/Items/FusionTreasures/TestSculpture";
    const RIFLE: &str = "/Lotus/Weapons/Test/TestRifle";

    fn inventory() -> Value {
        json!({
            "RegularCredits": 1000, "FusionPoints": 50,
            "MiscItems": [{"ItemType": PLASTIDS, "ItemCount": 10}, {"ItemType": RELIC, "ItemCount": 2}],
            "RawUpgrades": [{"ItemType": MOD, "ItemCount": 1}],
            "Recipes": [],
            "LongGuns": [{"ItemType": RIFLE, "ItemId": {"$oid": "a1"}}],
        })
    }

    fn with(mut base: Value, patch: Value) -> Value {
        base.as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        base
    }

    fn diff(before: &Value, after: &Value) -> Delta {
        diff_snapshots(
            &snapshot_inventory(before).unwrap(),
            &snapshot_inventory(after).unwrap(),
        )
    }

    fn item(name: &str, count: u32) -> MissionRewardItem {
        MissionRewardItem {
            unique_name: name.into(),
            count,
        }
    }

    #[test]
    fn rejects_values_that_are_not_an_inventory_object() {
        assert!(snapshot_inventory(&Value::Null).is_none());
        assert!(snapshot_inventory(&json!([])).is_none());
        assert!(snapshot_inventory(&json!("inventory")).is_none());
    }

    #[test]
    fn sums_stack_entries_per_unique_name_and_ignores_malformed_ones() {
        let s = snapshot_inventory(&json!({
            "MiscItems": [
                {"ItemType": PLASTIDS, "ItemCount": 4}, {"ItemType": PLASTIDS, "ItemCount": 3},
                {"ItemType": "", "ItemCount": 9}, {"ItemCount": 9},
                {"ItemType": RELIC, "ItemCount": "x"}, null,
            ],
            "CrewShipRawSalvage": [{"ItemType": RELIC, "ItemCount": 1}],
            "RegularCredits": "lots",
        }))
        .unwrap();
        assert_eq!(s.stacks[PLASTIDS], 7.0);
        assert_eq!(s.stacks[RELIC], 1.0);
        assert_eq!(s.credits, 0.0);
    }

    #[test]
    fn keys_unique_items_by_item_id_in_either_form() {
        let s = snapshot_inventory(&json!({
            "LongGuns": [{"ItemType": RIFLE, "ItemId": {"$oid": "a1"}}, {"ItemType": MOD, "ItemId": "b2"},
                         {"ItemType": MOD, "ItemId": ""}, {"ItemType": MOD}],
        }))
        .unwrap();
        assert_eq!(s.unique_ids.len(), 2);
        assert_eq!(s.unique_ids["a1"], RIFLE);
        assert_eq!(s.unique_ids["b2"], MOD);
    }

    #[test]
    fn reports_stack_gains_new_stacks_credits_and_endo() {
        let after = with(
            inventory(),
            json!({"RegularCredits": 26000, "FusionPoints": 80,
                   "MiscItems": [{"ItemType": PLASTIDS, "ItemCount": 14}, {"ItemType": RELIC, "ItemCount": 2},
                                 {"ItemType": SCULPTURE, "ItemCount": 1}]}),
        );
        assert_eq!(
            diff(&inventory(), &after),
            Delta {
                items: vec![item(SCULPTURE, 1), item(PLASTIDS, 4)],
                credits: 25000.0,
                endo: 30.0
            }
        );
    }

    #[test]
    fn ignores_anything_spent_or_consumed() {
        let after = with(
            inventory(),
            json!({"RegularCredits": 10, "FusionPoints": 0, "MiscItems": [{"ItemType": PLASTIDS, "ItemCount": 1}]}),
        );
        assert_eq!(diff(&inventory(), &after), Delta::default());
    }

    #[test]
    fn counts_a_unique_item_once_per_new_item_id() {
        let after = with(
            inventory(),
            json!({"LongGuns": [{"ItemType": RIFLE, "ItemId": {"$oid": "a1"}},
                                {"ItemType": RIFLE, "ItemId": {"$oid": "a2"}}]}),
        );
        assert_eq!(diff(&inventory(), &after).items, vec![item(RIFLE, 1)]);
    }

    #[test]
    fn merges_a_stack_gain_and_a_new_unique_entry_of_one_name() {
        let before = json!({"MiscItems": [{"ItemType": MOD, "ItemCount": 1}]});
        let after = json!({"MiscItems": [{"ItemType": MOD, "ItemCount": 2}],
                           "Upgrades": [{"ItemType": MOD, "ItemId": {"$oid": "u1"}}]});
        assert_eq!(diff(&before, &after).items, vec![item(MOD, 2)]);
    }

    #[test]
    fn leaves_out_builds_research_and_trade_offers_that_only_started() {
        let after = with(
            inventory(),
            json!({"PendingRecipes": [{"ItemType": RIFLE, "ItemId": {"$oid": "p1"}}],
                   "PendingTrades": [{"ItemType": MOD, "ItemId": {"$oid": "p2"}}],
                   "PersonalTechProjects": [{"ItemType": MOD, "ItemId": {"$oid": "p3"}}]}),
        );
        assert_eq!(diff(&inventory(), &after), Delta::default());
    }

    #[test]
    fn sums_counts_across_socket_states_so_filling_a_sculpture_is_not_a_gain() {
        let before = json!({"MiscItems": [{"ItemType": SCULPTURE, "ItemCount": 2}]});
        let after = json!({"MiscItems": [{"ItemType": SCULPTURE, "ItemCount": 1}],
                           "FusionTreasures": [{"ItemType": SCULPTURE, "ItemCount": 1, "Sockets": 1}]});
        assert_eq!(diff(&before, &after), Delta::default());
    }
}
