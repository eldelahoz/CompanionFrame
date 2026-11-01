//! The name pool the matcher ranks against. The card prints the game-language name, so every
//! localized reward also enters the pool under it; a hit is handed back under the English join key.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::fold::{has_quantity_prefix, strip_quantity_prefix};
use super::matcher::{folded_name, normalize_reward_text};
use crate::services::relics::{RelicDatabase, RelicRewardItem, RewardLookup, reward_items};

#[derive(Clone, Debug)]
pub struct PoolItem {
    pub name: String,
    /// Set on a localized alias: the English name the scan result must report.
    pub english_name: Option<String>,
    pub(super) normalized: String,
    pub(super) folded: String,
}

impl PoolItem {
    pub fn new(name: impl Into<String>, english_name: Option<String>) -> Self {
        let name = name.into();
        let normalized = normalize_reward_text(&name);
        Self {
            folded: folded_name(&normalized),
            normalized,
            name,
            english_name,
        }
    }

    /// The name the rest of the app joins on.
    pub fn english(&self) -> &str {
        self.english_name.as_deref().unwrap_or(&self.name)
    }
}

/// DE drops the "2X" from "2X Forma Blueprint", which would collide with plain Forma.
fn alias(item: &RelicRewardItem) -> Option<PoolItem> {
    let localized = item
        .display_name
        .as_deref()
        .filter(|l| !l.is_empty() && *l != item.name)?;
    let count = &item.name[..item.name.len() - strip_quantity_prefix(&item.name).len()];
    let name = if count.is_empty() || has_quantity_prefix(localized) {
        localized.to_owned()
    } else {
        format!("{count}{localized}")
    };
    Some(PoolItem::new(name, Some(item.name.clone())))
}

/// English names first, then the localized aliases, longest name first (stable).
pub fn build_pool(items: &[RelicRewardItem]) -> Vec<PoolItem> {
    let mut pool: Vec<PoolItem> = items.iter().map(|i| PoolItem::new(&i.name, None)).collect();
    pool.extend(items.iter().filter_map(alias));
    pool.sort_by_key(|p| std::cmp::Reverse(p.name.chars().count()));
    pool
}

/// Pool cache. It follows the relic database, which is rebuilt when the game language changes
/// (and holds the `Arc`, so a freed database's address can never be mistaken for the current one).
#[derive(Default)]
pub struct RewardPool(Mutex<Option<Built>>);

struct Built {
    db: Arc<RelicDatabase>,
    pool: Arc<Vec<PoolItem>>,
    /// The rewards the pool was built from, by English name.
    items: Arc<HashMap<String, RelicRewardItem>>,
}

impl RewardPool {
    pub fn built(
        &self,
        db: &Arc<RelicDatabase>,
        lookup: &dyn RewardLookup,
    ) -> (Arc<Vec<PoolItem>>, Arc<HashMap<String, RelicRewardItem>>) {
        let mut slot = self.0.lock().unwrap();
        if let Some(b) = &*slot
            && Arc::ptr_eq(&b.db, db)
        {
            return (b.pool.clone(), b.items.clone());
        }
        let rewards = reward_items(db, lookup);
        let b = Built {
            db: db.clone(),
            pool: Arc::new(build_pool(&rewards)),
            items: Arc::new(rewards.into_iter().map(|r| (r.name.clone(), r)).collect()),
        };
        let out = (b.pool.clone(), b.items.clone());
        *slot = Some(b);
        out
    }

    pub fn get(&self, db: &Arc<RelicDatabase>, lookup: &dyn RewardLookup) -> Arc<Vec<PoolItem>> {
        self.built(db, lookup).0
    }
}
