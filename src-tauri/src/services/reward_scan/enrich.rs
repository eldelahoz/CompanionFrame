//! Matched reward → what the card shows: ducats, owned parts, set progress, vaulted, mastered
//! (WFHelper `ipc/overlay/scan.ts` `enrichRewardItems`). Prices are joined later, they are async.
use std::collections::HashMap;

use serde_json::Value;

use super::fold::strip_quantity_prefix;
use super::payload::{RewardOverlayItem, RewardOverlayPart};
use crate::services::item_db::{Item, ItemDb, normalize_wfm_slug};
use crate::services::mastery::ownership::{
    aggregate_component_ownership, component_unique_name_aliases, owned_component_count,
    pending_recipe_counts, without_foundry_pending,
};
use crate::services::relics::RelicRewardItem;

/// WFM lists a set only when it has at least this many tradable parts.
const MIN_TRADABLE_SET_PARTS: usize = 2;

/// What the player owns and has in the foundry, from one inventory snapshot.
#[derive(Default)]
pub struct Ownership {
    counts: HashMap<String, u32>,
    pending: HashMap<String, u32>,
}

impl Ownership {
    pub fn from_inventory(inv: Option<&Value>, db: &ItemDb) -> Self {
        let Some(inv) = inv else {
            return Self::default();
        };
        let usable = without_foundry_pending(inv, &|u| db.is_reusable_blueprint(u));
        Self {
            counts: aggregate_component_ownership(&usable),
            pending: pending_recipe_counts(inv.get("PendingRecipes").unwrap_or(&Value::Null)),
        }
    }

    fn in_foundry(&self, unique_name: &str) -> bool {
        !self.pending.is_empty()
            && component_unique_name_aliases(unique_name)
                .iter()
                .any(|a| self.pending.contains_key(a))
    }
}

/// Mastered flags keyed by uniqueName and lowercase name. Items that cannot be mastered (Forma,
/// Kuva...) have no entry, so they get no chip.
#[derive(Default)]
pub struct MasteredSet(HashMap<String, bool>);

impl MasteredSet {
    pub fn new(items: impl IntoIterator<Item = (String, String, bool)>) -> Self {
        let mut m = HashMap::new();
        for (unique, name, mastered) in items {
            m.insert(unique, mastered);
            m.insert(name.to_lowercase(), mastered);
        }
        Self(m)
    }

    fn of(&self, unique_name: &str, name: &str) -> Option<bool> {
        self.0
            .get(unique_name)
            .or_else(|| self.0.get(&name.to_lowercase()))
            .copied()
    }
}

fn required_count(parent: Option<&Item>, unique_name: Option<&str>) -> u32 {
    let (Some(parent), Some(u)) = (parent, unique_name) else {
        return 1;
    };
    let aliases = component_unique_name_aliases(u);
    parent
        .components
        .iter()
        .find(|c| aliases.contains(&c.unique_name))
        .map_or(1, |c| c.item_count.max(1))
}

/// Builds the card for `reward` (the relic database row of the matched English name).
pub fn enrich(
    reward: &RelicRewardItem,
    db: &ItemDb,
    own: &Ownership,
    mastered: Option<&MasteredSet>,
) -> RewardOverlayItem {
    let resolved = db
        .lookup_by_name_or_slug(Some(&reward.name), reward.url_name.as_deref())
        .map(|(u, i)| (u.to_owned(), i))
        .or_else(|| {
            let u = reward.unique_name.as_deref()?;
            Some((u.to_owned(), db.get(u)?))
        });
    let unique = resolved.as_ref().map(|(u, _)| u.as_str());
    let entry = resolved.as_ref().map(|(_, i)| *i);
    let parent_unique = entry.and_then(|e| e.component_of.as_deref());
    let parent = parent_unique.and_then(|u| db.get(u));

    let reward_aliases = unique
        .map(component_unique_name_aliases)
        .unwrap_or_default();
    let (mut set_owned, mut set_required) = (0, 0);
    let mut parts = Vec::new();
    for c in parent.iter().flat_map(|p| &p.components) {
        if c.unique_name.is_empty() || c.tradable == Some(false) {
            continue;
        }
        let needed = c.item_count.max(1);
        let count = owned_component_count(&c.unique_name, &own.counts);
        set_required += needed;
        set_owned += count.min(needed);
        parts.push(RewardOverlayPart {
            name: if c.name.is_empty() {
                "Part".into()
            } else {
                c.name.clone()
            },
            image_url: db.get(&c.unique_name).and_then(|i| i.image_url.clone()),
            owned_count: count,
            required_count: needed,
            is_reward: reward_aliases.contains(&c.unique_name),
            building: own.in_foundry(&c.unique_name),
        });
    }
    let has_progress = set_required > 0;
    // Forma Blueprint builds into Forma, which is no set: WFM has no forma_set.
    let set_url_name = parent
        .filter(|_| parts.len() >= MIN_TRADABLE_SET_PARTS)
        .and_then(|p| normalize_wfm_slug(&format!("{} Set", p.name)));
    let is_forma = strip_quantity_prefix(&reward.name) == "Forma Blueprint";

    RewardOverlayItem {
        name: reward.name.clone(),
        rarity: reward.rarity.clone(),
        ducats: reward
            .ducats
            .filter(|d| *d > 0)
            .or_else(|| entry.and_then(|e| e.ducats))
            .unwrap_or(0),
        part_owned_count: unique.map_or(0, |u| owned_component_count(u, &own.counts)),
        part_required_count: required_count(parent, unique),
        mastered: mastered
            .zip(parent_unique.zip(parent))
            .and_then(|(m, (u, p))| m.of(u, &p.name)),
        vaulted: (!is_forma).then_some(reward.vaulted),
        building: unique.is_some_and(|u| own.in_foundry(u)),
        set_owned_count: if has_progress { set_owned } else { 0 },
        set_required_count: set_required,
        set_url_name,
        set_parts: parts,
    }
}
