//! Relic database: groups of refinements built from `@wfcd/items` `Relics` (bundled, or the newer
//! copy `relic_update` downloaded). Port of WFHelper's `services/relicService.ts`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;

use crate::services::bundled_data::BundledData;
use crate::services::game_locale::GameLocale;
use crate::services::item_db::{
    ItemDb, ItemDbState, WFCD_CDN, normalize_wfm_slug, public_image_url, relic_rarity,
};
use crate::services::relic_update::RelicDoc;

const QUALITIES: [&str; 4] = ["Intact", "Exceptional", "Flawless", "Radiant"];
const TIERS: [&str; 6] = ["Lith", "Meso", "Neo", "Axi", "Requiem", "Vanguard"];
const RELIC_PROJECTION_PATH: &str = "/Lotus/Types/Game/Projections/";

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicReward {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub unique_name: Option<String>,
    pub image_url: Option<String>,
    pub rarity: String,
    pub chance: f64,
    pub url_name: Option<String>,
    pub wfm_id: Option<String>,
    pub ducats: Option<u32>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicQuality {
    pub unique_name: Option<String>,
    pub rewards: Vec<RelicReward>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicGroup {
    pub key: String,
    pub name: String,
    pub tier: String,
    pub code: String,
    pub vaulted: bool,
    pub image_url: Option<String>,
    /// Keyed by lowercase refinement: intact, exceptional, flawless, radiant.
    pub qualities: BTreeMap<String, RelicQuality>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicRef {
    pub group_key: String,
    pub quality: String,
}

#[derive(Clone, Debug, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicDatabase {
    pub groups: BTreeMap<String, RelicGroup>,
    pub by_unique_name: BTreeMap<String, RelicRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RelicDataSource {
    Bundled,
    Downloaded,
}

/// Which relic data the database was built from, for Settings > About.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicDataInfo {
    pub version: Option<String>,
    pub source: RelicDataSource,
    pub published_at: Option<String>,
    /// Why the last update check failed; the app keeps the data above meanwhile.
    pub update_error: Option<String>,
    /// `update_error` is only a network problem, not a bad download.
    pub update_offline: bool,
    /// Releases newer than `version` exist but have no usable relic data.
    pub newer_unusable: bool,
}

/// What the build needs from the item database; a trait so tests can use a fixture.
pub trait RewardLookup {
    /// uniqueName and ducats of the item a reward name or WFM slug stands for.
    fn by_name_or_slug(
        &self,
        name: Option<&str>,
        slug: Option<&str>,
    ) -> Option<(String, Option<u32>)>;
    fn ducats_of(&self, unique_name: &str) -> Option<u32>;
    fn display_name(&self, unique_name: &str, english: &str) -> Option<String>;
}

pub struct ItemLookup<'a> {
    pub db: &'a ItemDb,
    pub locale: &'a GameLocale,
}

impl RewardLookup for ItemLookup<'_> {
    fn by_name_or_slug(
        &self,
        name: Option<&str>,
        slug: Option<&str>,
    ) -> Option<(String, Option<u32>)> {
        let (u, item) = self.db.lookup_by_name_or_slug(name, slug)?;
        Some((u.to_owned(), item.ducats))
    }

    fn ducats_of(&self, unique_name: &str) -> Option<u32> {
        self.db.get(unique_name)?.ducats
    }

    fn display_name(&self, unique_name: &str, english: &str) -> Option<String> {
        self.db.localized_name(self.locale, unique_name, english)
    }
}

fn s<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn wfcd_image(image_name: Option<&str>) -> Option<String> {
    let trimmed = image_name?.trim();
    (!trimmed.is_empty())
        .then(|| public_image_url(Some(&format!("{WFCD_CDN}{trimmed}"))))
        .flatten()
}

/// Nearly every @wfcd reward row carries the containing relic's uniqueName instead of the
/// item's, so the item database resolves the reward by name or WFM slug.
fn reward_unique_name(
    item: &Value,
    slug: Option<&str>,
    lookup: &dyn RewardLookup,
) -> Option<String> {
    let own = s(item, "uniqueName").unwrap_or("");
    if !own.is_empty() && !own.starts_with(RELIC_PROJECTION_PATH) {
        return Some(own.to_owned());
    }
    lookup
        .by_name_or_slug(s(item, "name"), slug)
        .map(|(u, _)| u)
}

/// @wfcd/items 1.1276.6 flags the 15 Citrine Prime relics vaulted while listing their mission drops.
fn is_vaulted(relic: &Value) -> bool {
    let drops = relic
        .get("dropCount")
        .and_then(Value::as_u64)
        .or_else(|| {
            relic
                .get("drops")
                .and_then(Value::as_array)
                .map(|d| d.len() as u64)
        })
        .unwrap_or(0);
    relic
        .get("vaulted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && drops == 0
}

fn build_reward(reward: &Value, quality: &str, lookup: &dyn RewardLookup) -> RelicReward {
    let item = reward.get("item").unwrap_or(&Value::Null);
    let market = item.get("warframeMarket");
    let raw_slug = market
        .and_then(|m| s(m, "urlName").or_else(|| s(m, "url_name")))
        .filter(|x| !x.is_empty());
    let unique_name = reward_unique_name(item, raw_slug, lookup);
    let name = s(item, "name")
        .filter(|n| !n.is_empty())
        .unwrap_or("Unknown");
    let chance = reward.get("chance").and_then(Value::as_f64).unwrap_or(0.0);
    RelicReward {
        name: name.to_owned(),
        display_name: unique_name
            .as_deref()
            .and_then(|u| lookup.display_name(u, name)),
        unique_name,
        image_url: wfcd_image(s(item, "imageName")),
        rarity: relic_rarity(
            &quality.to_lowercase(),
            chance,
            s(reward, "rarity")
                .filter(|r| !r.is_empty())
                .unwrap_or("Common"),
        ),
        chance,
        url_name: raw_slug.and_then(normalize_wfm_slug),
        wfm_id: market
            .and_then(|m| s(m, "id"))
            .filter(|i| !i.is_empty())
            .map(str::to_owned),
        ducats: item
            .get("ducats")
            .and_then(Value::as_f64)
            .filter(|d| d.is_finite() && *d >= 0.0)
            .map(|d| d.round() as u32),
    }
}

pub fn build(relics: &[Value], lookup: &dyn RewardLookup) -> RelicDatabase {
    let mut db = RelicDatabase::default();
    for relic in relics {
        let name = s(relic, "name").unwrap_or("");
        let parts: Vec<&str> = name.split(' ').collect();
        if parts.len() < 3 {
            continue;
        }
        let quality = parts[parts.len() - 1];
        let tier = parts[0];
        if !QUALITIES.contains(&quality) || !TIERS.contains(&tier) {
            continue;
        }
        let base = parts[..parts.len() - 1].join(" ");
        let lower = quality.to_lowercase();
        let vaulted = is_vaulted(relic);
        let group = db.groups.entry(base.clone()).or_insert_with(|| RelicGroup {
            key: base.clone(),
            name: base.clone(),
            tier: tier.to_owned(),
            code: parts[1..parts.len() - 1].join(" "),
            vaulted,
            image_url: None,
            qualities: BTreeMap::new(),
        });
        group.vaulted = group.vaulted && vaulted;
        if let Some(img) = s(relic, "imageName").filter(|i| !i.is_empty())
            && (quality == "Intact" || group.image_url.is_none())
        {
            group.image_url = wfcd_image(Some(img));
        }
        let unique_name = s(relic, "uniqueName")
            .filter(|u| !u.is_empty())
            .map(str::to_owned);
        group.qualities.insert(
            lower.clone(),
            RelicQuality {
                unique_name: unique_name.clone(),
                rewards: relic
                    .get("rewards")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|r| build_reward(r, quality, lookup))
                    .collect(),
            },
        );
        if let Some(u) = unique_name {
            db.by_unique_name.insert(
                u,
                RelicRef {
                    group_key: base,
                    quality: lower,
                },
            );
        }
    }
    db
}

/// One reward as the reward scanner wants it: once per name, vaulted only while every relic
/// dropping it is vaulted.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicRewardItem {
    pub name: String,
    /// Game-language name, when the active locale has one that differs from `name`.
    pub display_name: Option<String>,
    pub unique_name: Option<String>,
    pub url_name: Option<String>,
    pub rarity: String,
    pub ducats: Option<u32>,
    pub vaulted: bool,
}

pub fn reward_items(db: &RelicDatabase, lookup: &dyn RewardLookup) -> Vec<RelicRewardItem> {
    let mut seen: Vec<RelicRewardItem> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for group in db.groups.values() {
        for reward in group.qualities.values().flat_map(|q| &q.rewards) {
            if reward.name.is_empty() {
                continue;
            }
            if let Some(&i) = index.get(&reward.name) {
                seen[i].vaulted = seen[i].vaulted && group.vaulted;
                continue;
            }
            let resolved = lookup.by_name_or_slug(Some(&reward.name), reward.url_name.as_deref());
            let unique_name = resolved
                .as_ref()
                .map(|(u, _)| u.clone())
                .or_else(|| reward.unique_name.clone());
            let ducats = reward
                .ducats
                .or_else(|| resolved.and_then(|(_, d)| d))
                .or_else(|| {
                    reward
                        .unique_name
                        .as_deref()
                        .and_then(|u| lookup.ducats_of(u))
                });
            index.insert(reward.name.clone(), seen.len());
            seen.push(RelicRewardItem {
                name: reward.name.clone(),
                display_name: reward.display_name.clone(),
                unique_name,
                url_name: reward.url_name.clone(),
                rarity: if reward.rarity.is_empty() {
                    "Common".into()
                } else {
                    reward.rarity.clone()
                },
                ducats,
                vaulted: group.vaulted,
            });
        }
    }
    seen.sort_by(|a, b| a.name.cmp(&b.name));
    seen
}

// ---- state and commands ----

#[derive(Default)]
pub struct RelicState {
    downloaded: Mutex<Option<RelicDoc>>,
    /// Last check: error and whether it was only offline, and whether newer releases were unusable.
    update_error: Mutex<(Option<(String, bool)>, bool)>,
    /// Built database with what it was built from: item db, game language, downloaded version.
    built: Mutex<Option<(BuildKey, Arc<RelicDatabase>)>>,
    /// Same, in the game language: only the reward scanner's name pool reads it.
    scan: Mutex<Option<(BuildKey, Arc<RelicDatabase>)>>,
}

type BuildKey = (usize, &'static str, Option<String>);

impl RelicState {
    /// Rebuilds from this doc on the next read instead of the bundled package; `None` goes back to it.
    pub fn set_downloaded(&self, doc: Option<RelicDoc>) {
        *self.downloaded.lock().unwrap() = doc;
        *self.built.lock().unwrap() = None;
        *self.scan.lock().unwrap() = None;
    }

    /// True when the stored reason changed.
    pub fn set_update_error(
        &self,
        error: Option<&str>,
        offline: bool,
        newer_unusable: bool,
    ) -> bool {
        let mut cur = self.update_error.lock().unwrap();
        let next = (error.map(|e| (e.to_owned(), offline)), newer_unusable);
        let changed = *cur != next;
        *cur = next;
        changed
    }

    pub fn info(&self, bundled_version: Option<String>) -> RelicDataInfo {
        let (error, newer_unusable) = self.update_error.lock().unwrap().clone();
        let (update_error, update_offline) = match error {
            Some((e, offline)) => (Some(e), offline),
            None => (None, false),
        };
        match &*self.downloaded.lock().unwrap() {
            Some(d) => RelicDataInfo {
                version: Some(d.version.clone()),
                source: RelicDataSource::Downloaded,
                published_at: d.published_at.clone(),
                update_error,
                update_offline,
                newer_unusable,
            },
            None => RelicDataInfo {
                version: bundled_version,
                source: RelicDataSource::Bundled,
                published_at: None,
                update_error,
                update_offline,
                newer_unusable,
            },
        }
    }

    /// Names in the app language, for the UI.
    pub(crate) fn database(
        &self,
        bundled: &BundledData,
        items: &ItemDbState,
    ) -> Arc<RelicDatabase> {
        self.build_cached(&self.built, bundled, items.ui_snapshot())
    }

    /// Names in the game language, so the scanner can match what the card prints.
    pub(crate) fn scan_database(
        &self,
        bundled: &BundledData,
        items: &ItemDbState,
    ) -> Arc<RelicDatabase> {
        self.build_cached(&self.scan, bundled, items.snapshot())
    }

    fn build_cached(
        &self,
        slot: &Mutex<Option<(BuildKey, Arc<RelicDatabase>)>>,
        bundled: &BundledData,
        (db, locale): (Arc<ItemDb>, GameLocale),
    ) -> Arc<RelicDatabase> {
        let downloaded = self.downloaded.lock().unwrap().clone();
        let key = (
            Arc::as_ptr(&db) as usize,
            locale.code(),
            downloaded.as_ref().map(|d| d.version.clone()),
        );
        if let Some((k, built)) = &*slot.lock().unwrap()
            && *k == key
        {
            return built.clone();
        }
        let rows = match downloaded {
            Some(d) => d.relics,
            None => bundled.wfcd_items(&["Relics"]).unwrap_or_else(|e| {
                tracing::error!("[RelicDB] @wfcd/items not available: {e}");
                vec![]
            }),
        };
        let built = Arc::new(build(
            &rows,
            &ItemLookup {
                db: &db,
                locale: &locale,
            },
        ));
        tracing::info!("[RelicDB] {} relic groups indexed", built.groups.len());
        *slot.lock().unwrap() = Some((key, built.clone()));
        built
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_relic_database(
    app: tauri::AppHandle,
    state: tauri::State<RelicState>,
    items: tauri::State<ItemDbState>,
) -> Result<RelicDatabase, String> {
    Ok((*state.database(&BundledData::from_app(&app)?, &items)).clone())
}

#[tauri::command]
#[specta::specta]
pub fn get_relic_data_info(
    app: tauri::AppHandle,
    state: tauri::State<RelicState>,
) -> RelicDataInfo {
    state.info(
        BundledData::from_app(&app)
            .ok()
            .and_then(|b| b.wfcd_version()),
    )
}

pub fn setup(app: &tauri::App) {
    use tauri::Manager;
    app.manage(RelicState::default());
    crate::services::relic_update::start(app.handle());
}

#[cfg(test)]
mod tests;
