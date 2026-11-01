//! Mastery progress from an inventory snapshot (port of WFHelper's `masteryHelper`).
//!
//! Pure over `(inventory JSON, item DB, export tables)`; the command feeds it the live inventory.

mod classify;
pub mod ownership;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, OnceLock};

use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use tauri::Manager;

use crate::services::bundled_data::BundledData;
use crate::services::game_locale::GameLocale;
use crate::services::inventory::Inventory;
use crate::services::item_db::{DropInfo, ItemDb, ItemDbState};
use classify::{
    MASTERABLE_DB_CATEGORIES, VINQUIBUS_MELEE, VINQUIBUS_PRIMARY, display_category, exclude_reason,
    is_amp_prism, is_kitgun_chamber, is_venari, keywords,
};
use ownership::{EQUIPMENT_COLLECTIONS, MODULAR_COLLECTIONS, item_type, to_finite};
pub use ownership::{
    aggregate_component_ownership, mastery_rank_to_xp, mastery_xp_to_rank, owned_component_count,
    pending_recipe_counts, without_foundry_pending,
};

const MAX_ITEM_RANK: u32 = 30;
const WEAPON_AFFINITY: f64 = 500.0;
const SUIT_AFFINITY: f64 = 1_000.0;
const JUNCTION_MASTERY_XP: f64 = 1_000.0;
const INTRINSIC_MASTERY_XP: f64 = 1_500.0;
const MAX_INTRINSIC_RANK: f64 = 10.0;
const GILDED_FEATURE_MASK: i64 = 8;
const SUIT_INVENTORY_KEYS: [&str; 7] = [
    "Suits",
    "Sentinels",
    "SpaceSuits",
    "MechSuits",
    "KubrowPets",
    "MoaPets",
    "Hoverboards",
];
const MASTERED_FLAG_KEYS: [&str; 8] = [
    "Mastered",
    "mastered",
    "IsMastered",
    "isMastered",
    "Completed",
    "completed",
    "IsComplete",
    "isComplete",
];
const RAILJACK_HARNESS: &str = "/Lotus/Types/Game/CrewShip/RailjackHarness";

/// Hound suits are model-specific: credit them to the model head when an inventory carries
/// suit XP without ModularParts. Vinquibus' melee half shares its primary's mastery.
const ALIASES: [(&str, &str); 5] = [
    (
        RAILJACK_HARNESS,
        "/Lotus/Types/Game/CrewShip/RailJack/DefaultHarness",
    ),
    (
        "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetParts/ZanukaPetPartHeadA",
        "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetAPowerSuit",
    ),
    (
        "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetParts/ZanukaPetPartHeadB",
        "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetBPowerSuit",
    ),
    (
        "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetParts/ZanukaPetPartHeadC",
        "/Lotus/Types/Friendly/Pets/ZanukaPets/ZanukaPetCPowerSuit",
    ),
    (VINQUIBUS_PRIMARY, VINQUIBUS_MELEE),
];

fn re(p: &str) -> Regex {
    Regex::new(&format!("(?i){p}")).unwrap()
}
static MODULAR_COMPANION_MODEL: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"/Pets/(?:MoaPets/MoaPetParts/MoaPetHead[^/]*|ZanukaPets/ZanukaPetParts/ZanukaPetPartHead[ABC])$",
    )
});
static HOVERBOARD_PATH: LazyLock<Regex> = LazyLock::new(|| re(r"/Hoverboard"));
static K_DRIVE: LazyLock<Regex> = LazyLock::new(|| re("k-drive"));
static HARNESS: LazyLock<Regex> =
    LazyLock::new(|| re(r"/CrewShip/(?:RailJack/Default)?Harness$|RailjackHarness$"));
static OVERCAP_NAME: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?:^|[\s/])(?:kuva|tenet|coda)(?:[\s/]|$)"));
static SUIT_PATH: LazyLock<Regex> =
    LazyLock::new(|| re(r"/(?:SpaceSuits?|Powersuits/Archwing|Hoverboard)/"));

// ---- output ----

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MasteryComponent {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub unique_name: String,
    pub tradable: bool,
    pub item_count: u32,
    pub owned_count: u32,
    pub owned: bool,
    pub drops: Vec<DropInfo>,
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MasteryItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub unique_name: String,
    pub category: String,
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_image_url: Option<String>,
    pub is_prime: bool,
    pub mastery_req: u32,
    pub vaulted: bool,
    pub tradable: bool,
    pub keywords: Vec<String>,
    pub debug_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_buyable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_credits: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dojo_research: Option<bool>,
    pub status: MasteryStatus,
    pub rank: u32,
    pub max_rank: u32,
    pub currently_owned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_gilding: Option<bool>,
    pub mastery_xp: f64,
    /// Mastery still on the table: what maxing this item would add.
    pub mastery_xp_remaining: f64,
    pub components: Vec<MasteryComponent>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum MasteryStatus {
    Mastered,
    Progress,
    Missing,
}

#[derive(Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStats {
    pub total: u32,
    pub mastered: u32,
    pub in_progress: u32,
    pub missing: u32,
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMastery {
    pub rank: Option<f64>,
    pub percent_to_next: Option<f64>,
    pub total_xp: Option<f64>,
    pub xp_into_rank: Option<f64>,
    pub xp_for_next: Option<f64>,
    pub test_ready: bool,
}

#[derive(Clone, Default, Serialize, specta::Type)]
pub struct ProgressPair {
    pub done: u32,
    pub total: u32,
}

#[derive(Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StarChartCompletion {
    pub normal: ProgressPair,
    pub junctions: ProgressPair,
    pub steel_path: ProgressPair,
    pub steel_path_junctions: ProgressPair,
}

#[derive(Clone, Default, Serialize, specta::Type)]
pub struct IntrinsicCompletion {
    pub railjack: ProgressPair,
    pub drifter: ProgressPair,
}

#[derive(Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AccountCompletion {
    pub star_chart: StarChartCompletion,
    pub intrinsics: IntrinsicCompletion,
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MasteryStats {
    pub total: u32,
    pub mastered: u32,
    pub in_progress: u32,
    pub missing: u32,
    pub by_category: std::collections::BTreeMap<String, CategoryStats>,
    pub profile_mastery: Option<ProfileMastery>,
    pub completion: AccountCompletion,
}

#[derive(Clone, Serialize, specta::Type)]
pub struct MasteryProgress {
    pub items: Vec<MasteryItem>,
    pub stats: MasteryStats,
    /// Computed from the public profile (no inventory loaded): no resources or component counts.
    #[serde(rename = "fromProfile")]
    pub from_profile: bool,
}

// ---- export tables ----

/// What the calculation reads from DE's export beyond the item DB.
#[derive(Default)]
pub struct MasteryExports {
    /// `maxLevelCap` above rank 30 (authoritative overcapped ranks).
    caps: HashMap<String, f64>,
    /// Every uniqueName the export lists: no cap there means rank 30.
    known: HashSet<String>,
    /// Node tag → (mastery granted, is junction). Junctions export 0 but award a flat 1000.
    regions: HashMap<String, (f64, bool)>,
}

impl MasteryExports {
    pub fn load(bundled: &BundledData) -> Self {
        let mut out = Self::default();
        for name in bundled.list_exports() {
            let Some(table) = bundled.export(&name) else {
                continue;
            };
            for (unique_name, entry) in &table {
                out.known.insert(unique_name.clone());
                let cap = entry.get("maxLevelCap").and_then(to_finite);
                if let Some(cap) = cap.filter(|c| *c > f64::from(MAX_ITEM_RANK)) {
                    out.caps.insert(unique_name.clone(), cap);
                }
            }
        }
        for (tag, node) in bundled.export("ExportRegions").iter().flatten() {
            let junction = node.get("missionType").and_then(Value::as_str) == Some("MT_JUNCTION");
            let xp = if junction {
                JUNCTION_MASTERY_XP
            } else {
                node.get("masteryExp").and_then(to_finite).unwrap_or(0.0)
            };
            out.regions.insert(tag.clone(), (xp, junction));
        }
        out
    }
}

pub struct MasteryState {
    bundled: Option<Arc<BundledData>>,
    exports: OnceLock<Arc<MasteryExports>>,
}

impl MasteryState {
    fn exports(&self) -> Arc<MasteryExports> {
        self.exports
            .get_or_init(|| {
                Arc::new(
                    self.bundled
                        .as_deref()
                        .map(MasteryExports::load)
                        .unwrap_or_default(),
                )
            })
            .clone()
    }
}

// ---- calculation ----

pub struct Ctx<'a> {
    pub db: &'a ItemDb,
    pub exports: &'a MasteryExports,
    pub locale: &'a GameLocale,
}

struct Masterable {
    name: String,
    display_name: Option<String>,
    unique_name: String,
    category: &'static str,
    image_url: Option<String>,
    fallback_image_url: Option<String>,
    is_prime: bool,
    mastery_req: u32,
    vaulted: bool,
    tradable: bool,
    keywords: Vec<String>,
    debug_reason: String,
    components: Vec<crate::services::item_db::Comp>,
    market_buyable: bool,
    market_credits: Option<f64>,
    dojo_research: bool,
}

#[derive(Clone)]
struct Owned {
    /// Current rank (post-forma this resets to 0): drives the level bar.
    rank: u32,
    /// Highest rank ever reached: mastery credit is banked here.
    mastery_rank: u32,
    max_rank: u32,
    owned: bool,
    mastered: bool,
    mastery_per_rank: f64,
    /// A modular build still ungilded: the game grants no mastery until it is.
    needs_gilding: bool,
}

fn xp_to_rank(xp: f64, max_rank: u32, affinity: f64) -> u32 {
    if xp <= 0.0 {
        return 0;
    }
    ((xp / affinity).sqrt().floor() as u32).min(max_rank)
}

fn pick_bool(entry: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter().find_map(|k| match entry.get(*k)? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => Some(n.as_f64()? != 0.0),
        Value::String(s) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

fn pick_number(obj: &Value, paths: &[&[&str]]) -> Option<f64> {
    paths.iter().find_map(|p| {
        let mut cur = obj;
        for k in *p {
            cur = cur.as_object()?.get(*k)?;
        }
        to_finite(cur)
    })
}

fn num(entry: &Value, key: &str) -> Option<f64> {
    entry.get(key).and_then(to_finite)
}

impl Ctx<'_> {
    fn max_rank(&self, unique_name: &str, fallback: u32) -> u32 {
        // The export knows every rank-40 family, so no cap there means 30. The heuristics
        // below are only for items it omits (they over-capped strays like Onos).
        if let Some(cap) = self.exports.caps.get(unique_name) {
            return (*cap as u32).max(fallback);
        }
        if self.exports.known.contains(unique_name) {
            return fallback;
        }
        let name = self.db.get(unique_name).map_or("", |i| i.name.as_str());
        let path = unique_name.to_lowercase();
        if fallback > MAX_ITEM_RANK
            || OVERCAP_NAME.is_match(&format!("{unique_name} {name}"))
            || path.contains("infestedlich")
            || name.to_lowercase() == "paracesis"
            || path.contains("ballassword")
            || path.contains("mechsuits")
            || path.contains("entrati")
        {
            return 40;
        }
        fallback
    }

    fn is_suit_rate_xp_info(&self, unique_name: &str) -> bool {
        let db = self.db.get(unique_name);
        db.is_some_and(|i| i.category == "Warframe" || i.category == "Companion")
            || MODULAR_COMPANION_MODEL.is_match(unique_name)
            || HOVERBOARD_PATH.is_match(unique_name)
            || db.is_some_and(|i| K_DRIVE.is_match(&i.kind))
            || HARNESS.is_match(unique_name)
    }

    fn read_owned(
        &self,
        entry: &Value,
        fallback_max: u32,
        owned: bool,
        affinity: f64,
    ) -> Option<Owned> {
        let t = item_type(entry);
        if t.is_empty() {
            return None;
        }
        let max_rank = self.max_rank(t, fallback_max);
        let unlocked = unlocked_max_rank(entry, max_rank, owned);
        let xp_rank = xp_to_rank(num(entry, "XP").unwrap_or(0.0), unlocked, affinity);
        let rank = unlocked.min(xp_rank.max(overcap_feature_rank(entry, max_rank).unwrap_or(0)));
        Some(Owned {
            rank,
            mastery_rank: rank,
            max_rank,
            owned,
            mastered: pick_bool(entry, &MASTERED_FLAG_KEYS) == Some(true) || rank >= max_rank,
            mastery_per_rank: affinity / 5.0,
            needs_gilding: false,
        })
    }

    fn masterable_items(&self) -> Vec<Masterable> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (unique_name, item) in self.db.iter() {
            let name = crate::services::item_db::sanitize_display_name(if item.name.is_empty() {
                "Unknown"
            } else {
                &item.name
            });
            if !MASTERABLE_DB_CATEGORIES.contains(&item.category.as_str()) {
                continue;
            }
            let prism = is_amp_prism(&item.name, unique_name);
            let chamber = is_kitgun_chamber(&item.name, unique_name);
            let venari = is_venari(unique_name);
            if item.masterable == Some(false) && !prism && !chamber && !venari {
                continue;
            }
            if exclude_reason(unique_name, &name, item).is_some() {
                continue;
            }
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            let display = display_category(item, unique_name);
            if display.category == "Railjack" {
                continue;
            }
            let source = if prism {
                "amp-prism-override"
            } else if chamber {
                "kitgun-chamber-override"
            } else if venari {
                "venari-override"
            } else if item.masterable == Some(true) {
                "wfcd-masterable:true"
            } else {
                "default"
            };
            let or_q = |s: &str| {
                if s.is_empty() {
                    "?".to_owned()
                } else {
                    s.to_owned()
                }
            };
            out.push(Masterable {
                display_name: self.db.localized_name(self.locale, unique_name, &name),
                keywords: keywords(unique_name, &name),
                debug_reason: format!(
                    "show:{source}; cat:{}; dbCat:{}; product:{}; type:{}",
                    display.source,
                    or_q(&item.category),
                    or_q(item.product_category.as_deref().unwrap_or("")),
                    or_q(&item.kind)
                ),
                name,
                unique_name: unique_name.clone(),
                category: display.category,
                image_url: item.image_url.clone(),
                fallback_image_url: item.fallback_image_url.clone(),
                is_prime: item.is_prime,
                mastery_req: item.mastery_req,
                vaulted: item.vaulted,
                tradable: item.tradable.unwrap_or(false) || item.is_prime,
                components: item.components.clone(),
                market_buyable: item.market_buyable,
                market_credits: item.market_credits,
                dojo_research: item.dojo_research,
            });
        }
        // Plexus has no catalog entry of its own.
        if seen.insert("plexus".into()) {
            out.push(Masterable {
                name: "Plexus".into(),
                display_name: None,
                unique_name: RAILJACK_HARNESS.into(),
                category: "Companions",
                image_url: None,
                fallback_image_url: None,
                is_prime: false,
                mastery_req: 0,
                vaulted: false,
                tradable: false,
                keywords: vec!["plexus".into(), "railjack".into()],
                debug_reason:
                    "show:synthetic; cat:profile-companions; dbCat:?; product:CrewShip; type:Plexus"
                        .into(),
                components: vec![],
                market_buyable: false,
                market_credits: None,
                dojo_research: false,
            });
        }
        // shortcut: byte order, not ICU localeCompare; punctuation-leading names may differ.
        out.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.name.cmp(&b.name))
        });
        out
    }
}

fn unlocked_max_rank(entry: &Value, max_rank: u32, owned: bool) -> u32 {
    if !owned || max_rank <= MAX_ITEM_RANK {
        return max_rank;
    }
    let polarized = num(entry, "Polarized").unwrap_or(0.0).floor().max(0.0) as u32;
    max_rank.min(MAX_ITEM_RANK + polarized * 2)
}

fn overcap_feature_rank(entry: &Value, max_rank: u32) -> Option<u32> {
    if max_rank <= MAX_ITEM_RANK {
        return None;
    }
    let rank = num(entry, "Features")?.floor() as i64 + 1;
    (rank > i64::from(MAX_ITEM_RANK) && rank <= i64::from(max_rank)).then_some(rank as u32)
}

fn has_gilded_feature(entry: &Value) -> bool {
    num(entry, "Features").is_some_and(|f| (f.floor() as i64) & GILDED_FEATURE_MASK != 0)
}

fn requires_gilding(inv_key: &str, entry: &Value, part: Option<&str>) -> bool {
    match inv_key {
        "MoaPets" => true,
        "KubrowPets" => entry.get("ModularParts").is_some_and(Value::is_array),
        "OperatorAmps" => part.is_some(),
        "LongGuns" | "Pistols" | "Melee" => part.is_some(),
        _ => false,
    }
}

fn better(current: Option<Owned>, candidate: Option<&Owned>) -> Option<Owned> {
    let Some(cand) = candidate else {
        return current;
    };
    let Some(cur) = current else {
        return Some(cand.clone());
    };
    // Bank the highest rank either record saw; display fields pick a winner below.
    let mastery_rank = cur.mastery_rank.max(cand.mastery_rank);
    let cand_wins = if cand.mastered != cur.mastered {
        cand.mastered
    } else if cand.rank != cur.rank {
        cand.rank > cur.rank
    } else if cand.max_rank != cur.max_rank {
        cand.max_rank > cur.max_rank
    } else {
        cand.owned != cur.owned && cand.owned
    };
    let mut winner = if cand_wins { cand.clone() } else { cur };
    winner.mastery_rank = mastery_rank;
    Some(winner)
}

/// Owned items carry their own rate; missing ones fall back to their type.
fn item_mastery_per_rank(category: &str, unique_name: &str) -> f64 {
    let suit = matches!(category, "Warframes" | "Companions" | "Necramech")
        || SUIT_PATH.is_match(unique_name);
    (if suit { SUIT_AFFINITY } else { WEAPON_AFFINITY }) / 5.0
}

/// Node mastery is granted once normally and once again on Steel Path tier 1.
fn mission_mastery_xp(inv: &Value, regions: &HashMap<String, (f64, bool)>) -> f64 {
    let mut xp = 0.0;
    for entry in inv
        .get("Missions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some((grant, _)) = entry
            .get("Tag")
            .and_then(Value::as_str)
            .and_then(|t| regions.get(t))
        else {
            continue;
        };
        let steel = num(entry, "Tier") == Some(1.0);
        if num(entry, "Completes").unwrap_or(0.0) <= 0.0 && !steel {
            continue;
        }
        xp += grant;
        if steel {
            xp += grant;
        }
    }
    xp
}

fn sum_ranks(
    skills: Option<&serde_json::Map<String, Value>>,
    keep: impl Fn(&str) -> bool,
) -> (u32, u32) {
    let (mut done, mut total) = (0.0, 0u32);
    for (_, value) in skills.into_iter().flatten().filter(|(k, _)| keep(k)) {
        total += MAX_INTRINSIC_RANK as u32;
        if let Some(r) = to_finite(value).filter(|r| *r > 0.0) {
            done += r.min(MAX_INTRINSIC_RANK);
        }
    }
    (done as u32, total)
}

/// Star chart + intrinsic completion. Display only; totals from ExportRegions.
fn account_completion(inv: &Value, regions: &HashMap<String, (f64, bool)>) -> AccountCompletion {
    let (mut normal_total, mut junction_total) = (0, 0);
    for (xp, junction) in regions.values().filter(|(xp, _)| *xp > 0.0) {
        let _ = xp;
        if *junction {
            junction_total += 1;
        } else {
            normal_total += 1;
        }
    }
    let (mut n, mut j, mut s, mut sj) = (0, 0, 0, 0);
    for entry in inv
        .get("Missions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some((xp, junction)) = entry
            .get("Tag")
            .and_then(Value::as_str)
            .and_then(|t| regions.get(t))
        else {
            continue;
        };
        let steel = num(entry, "Tier").unwrap_or(0.0) >= 1.0;
        if *xp <= 0.0 || (num(entry, "Completes").unwrap_or(0.0) <= 0.0 && !steel) {
            continue;
        }
        match (*junction, steel) {
            (true, true) => (j, sj) = (j + 1, sj + 1),
            (true, false) => j += 1,
            (false, true) => (n, s) = (n + 1, s + 1),
            (false, false) => n += 1,
        }
    }
    let skills = inv.get("PlayerSkills").and_then(Value::as_object);
    let pair = |(done, total)| ProgressPair { done, total };
    AccountCompletion {
        star_chart: StarChartCompletion {
            normal: ProgressPair {
                done: n,
                total: normal_total,
            },
            junctions: ProgressPair {
                done: j,
                total: junction_total,
            },
            steel_path: ProgressPair {
                done: s,
                total: normal_total,
            },
            steel_path_junctions: ProgressPair {
                done: sj,
                total: junction_total,
            },
        },
        intrinsics: IntrinsicCompletion {
            railjack: pair(sum_ranks(skills, |k| {
                k.starts_with("LPS_") && !k.starts_with("LPS_DRIFT_")
            })),
            drifter: pair(sum_ranks(skills, |k| k.starts_with("LPS_DRIFT_"))),
        },
    }
}

/// LPS_* entries are intrinsic ranks (railjack + drifter), 1500 mastery each; LPP_* pools grant nothing.
fn intrinsic_mastery_xp(inv: &Value) -> f64 {
    let skills = inv.get("PlayerSkills").and_then(Value::as_object);
    let (ranks, _) = sum_ranks(skills, |k| k.starts_with("LPS_"));
    f64::from(ranks) * INTRINSIC_MASTERY_XP
}

fn one_decimal(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn profile_mastery(inv: &Value, total_xp: Option<f64>) -> Option<ProfileMastery> {
    let rank = pick_number(
        inv,
        &[
            &["MasteryRank"],
            &["MasteryLevel"],
            &["PlayerLevel"],
            &["PlayerRank"],
            &["LevelInfo", "MasteryRank"],
            &["LevelInfo", "PlayerLevel"],
        ],
    )
    .or_else(|| total_xp.map(|x| mastery_xp_to_rank(x) as f64));

    // Preferred: progress from the mastery XP we computed ourselves.
    if let (Some(rank), Some(total)) = (rank, total_xp) {
        let current = mastery_rank_to_xp(rank as i64);
        let next = mastery_rank_to_xp(rank as i64 + 1);
        let for_next = next - current;
        let base = ProfileMastery {
            rank: Some(rank),
            percent_to_next: None,
            total_xp: Some(total),
            xp_into_rank: None,
            xp_for_next: None,
            test_ready: false,
        };
        // Test not taken yet: keep the overflowing figure, don't clamp to the bar.
        if total >= next {
            return Some(ProfileMastery {
                percent_to_next: Some(100.0),
                xp_into_rank: Some(total - current),
                xp_for_next: Some(for_next),
                test_ready: true,
                ..base
            });
        }
        // Reconstruction undercounts big accounts: below the rank floor, trust the game rank.
        if total < current {
            return Some(base);
        }
        let into = total - current;
        return Some(ProfileMastery {
            percent_to_next: Some(one_decimal((into / for_next * 100.0).min(100.0))),
            xp_into_rank: Some(into),
            xp_for_next: Some(for_next),
            ..base
        });
    }

    // Fallback: percent fields some helper exports include.
    let mut percent = pick_number(
        inv,
        &[
            &["MasteryPercent"],
            &["MasteryProgressPercent"],
            &["PlayerLevelProgressPercent"],
            &["LevelInfo", "MasteryPercent"],
            &["LevelInfo", "ProgressPercent"],
        ],
    );
    if percent.is_none() {
        let cur = pick_number(
            inv,
            &[
                &["MasteryXP"],
                &["MasteryXp"],
                &["PlayerXP"],
                &["PlayerXp"],
                &["LevelInfo", "MasteryXP"],
                &["LevelInfo", "CurrentXP"],
            ],
        );
        let next = pick_number(
            inv,
            &[
                &["NextMasteryXP"],
                &["NextLevelXP"],
                &["MasteryXPForNextRank"],
                &["LevelInfo", "NextXP"],
                &["LevelInfo", "NextLevelXP"],
            ],
        );
        if let (Some(c), Some(n)) = (cur, next)
            && n > 0.0
        {
            percent = Some(c / n * 100.0);
        }
    }
    if rank.is_none() && percent.is_none() {
        return None;
    }
    Some(ProfileMastery {
        rank,
        percent_to_next: percent.map(|p| one_decimal(p.clamp(0.0, 100.0))),
        total_xp: None,
        xp_into_rank: None,
        xp_for_next: None,
        test_ready: false,
    })
}

/// Doubled ingredients arrive as two rows of one; sum them so one copy cannot satisfy both.
fn merge_duplicate_ingredients(
    rows: &[crate::services::item_db::Comp],
) -> Vec<crate::services::item_db::Comp> {
    let mut merged: Vec<crate::services::item_db::Comp> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for row in rows {
        let count = row.item_count.max(1);
        match (!row.unique_name.is_empty())
            .then(|| index.get(row.unique_name.as_str()))
            .flatten()
        {
            Some(&i) => merged[i].item_count = merged[i].item_count.max(1) + count,
            None => {
                if !row.unique_name.is_empty() {
                    index.insert(&row.unique_name, merged.len());
                }
                let mut r = row.clone();
                r.item_count = count;
                merged.push(r);
            }
        }
    }
    merged
}

pub fn compute_mastery_progress(inv: &Value, ctx: &Ctx) -> MasteryProgress {
    let all = ctx.masterable_items();
    let usable = without_foundry_pending(inv, &|u| ctx.db.is_reusable_blueprint(u));
    let ownership = aggregate_component_ownership(&usable);
    let masterable_names: HashSet<&str> = all.iter().map(|m| m.unique_name.as_str()).collect();

    // `order` mirrors a JS Map: first insertion fixes the position.
    let mut owned_map: HashMap<String, Owned> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut put = |map: &mut HashMap<String, Owned>, key: &str, rec: Owned| {
        if map.insert(key.to_owned(), rec).is_none() {
            order.push(key.to_owned());
        }
    };

    // A kit masters through one part (strike/chamber/prism/deck) while the build carries a
    // generic ItemType; that part is the masterable one in the catalog.
    let modular_part = |entry: &Value| -> Option<String> {
        entry
            .get("ModularParts")?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .find(|p| masterable_names.contains(p))
            .map(str::to_owned)
    };

    for inv_key in EQUIPMENT_COLLECTIONS
        .iter()
        .chain(MODULAR_COLLECTIONS.iter())
    {
        let affinity = if SUIT_INVENTORY_KEYS.contains(inv_key) {
            SUIT_AFFINITY
        } else {
            WEAPON_AFFINITY
        };
        for entry in inv
            .get(*inv_key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(mut rec) = ctx.read_owned(entry, MAX_ITEM_RANK, true, affinity) else {
                continue;
            };
            let part = modular_part(entry);
            if requires_gilding(inv_key, entry, part.as_deref()) && !has_gilded_feature(entry) {
                rec.rank = 0;
                rec.mastery_rank = 0;
                rec.mastered = false;
                rec.needs_gilding = true;
            }
            let key = part.as_deref().unwrap_or_else(|| item_type(entry));
            if key.is_empty() {
                continue;
            }
            // Several builds can share one part; mastery keeps the best rank reached.
            if owned_map.get(key).is_some_and(|e| e.rank > rec.rank) {
                continue;
            }
            put(&mut owned_map, key, rec);
        }
    }

    // XPInfo: items sold but XP still counts.
    for entry in inv
        .get("XPInfo")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let t = item_type(entry);
        if t.is_empty() {
            continue;
        }
        let existing = owned_map.get(t).cloned();
        let affinity = if ctx.is_suit_rate_xp_info(t) {
            SUIT_AFFINITY
        } else {
            WEAPON_AFFINITY
        };
        let fallback = existing.as_ref().map_or(MAX_ITEM_RANK, |e| e.max_rank);
        let Some(rec) = ctx.read_owned(entry, fallback, false, affinity) else {
            continue;
        };
        let merged = match existing {
            Some(e) => Owned {
                max_rank: e.max_rank.max(rec.max_rank),
                mastered: e.mastered || rec.mastered,
                // Forma resets owned rank to 0; keep credit at the highest rank.
                mastery_rank: e.mastery_rank.max(rec.mastery_rank),
                ..e
            },
            None => Owned {
                owned: false,
                ..rec
            },
        };
        put(&mut owned_map, t, merged);
    }

    // Name-based fallback matching; the last entry per name wins.
    let mut owned_by_name: HashMap<String, Owned> = HashMap::new();
    for key in &order {
        if let Some(db_item) = ctx.db.get(key) {
            owned_by_name.insert(db_item.name.to_lowercase(), owned_map[key].clone());
        }
    }

    let mut items = Vec::with_capacity(all.len());
    for m in all {
        let mut owned = owned_map.get(&m.unique_name).cloned();
        owned = better(owned, owned_by_name.get(&m.name.to_lowercase()));
        for (_, alias) in ALIASES.iter().filter(|(u, _)| *u == m.unique_name) {
            owned = better(owned, owned_map.get(*alias));
        }

        let mut max_rank = ctx.max_rank(&m.unique_name, MAX_ITEM_RANK);
        let (mut status, mut rank, mut currently_owned, mut mastery_xp) =
            (MasteryStatus::Missing, 0, false, 0.0);
        if let Some(o) = &owned {
            rank = o.rank;
            max_rank = o.max_rank;
            currently_owned = o.owned;
            status = if o.mastered || rank >= max_rank {
                MasteryStatus::Mastered
            } else {
                MasteryStatus::Progress
            };
            // Credit the highest historical rank; a mastered flag banks the full max.
            let credit = if o.mastered { max_rank } else { o.mastery_rank };
            mastery_xp = f64::from(credit.min(max_rank)) * o.mastery_per_rank;
        }

        let components = merge_duplicate_ingredients(&m.components)
            .into_iter()
            .map(|c| {
                let owned_count = owned_component_count(&c.unique_name, &ownership);
                MasteryComponent {
                    display_name: ctx.db.localized_name(ctx.locale, &c.unique_name, &c.name),
                    owned: owned_count >= c.item_count,
                    name: c.name,
                    unique_name: c.unique_name,
                    tradable: c.tradable.unwrap_or(false),
                    item_count: c.item_count,
                    owned_count,
                    drops: c.drops,
                }
            })
            .collect();

        let per_rank = owned.as_ref().map_or_else(
            || item_mastery_per_rank(m.category, &m.unique_name),
            |o| o.mastery_per_rank,
        );
        items.push(MasteryItem {
            name: m.name,
            display_name: m.display_name,
            unique_name: m.unique_name,
            category: m.category.to_owned(),
            image_url: m.image_url,
            fallback_image_url: m.fallback_image_url,
            is_prime: m.is_prime,
            mastery_req: m.mastery_req,
            vaulted: m.vaulted,
            tradable: m.tradable,
            keywords: m.keywords,
            debug_reason: m.debug_reason,
            market_buyable: m.market_buyable.then_some(true),
            market_credits: m.market_credits,
            dojo_research: m.dojo_research.then_some(true),
            status,
            rank,
            max_rank,
            currently_owned,
            needs_gilding: owned
                .as_ref()
                .is_some_and(|o| o.needs_gilding)
                .then_some(true),
            mastery_xp,
            mastery_xp_remaining: (f64::from(max_rank) * per_rank - mastery_xp).max(0.0),
            components,
        });
    }

    // Account mastery XP only makes sense when the inventory actually has XP data.
    let has_xp = inv
        .get("XPInfo")
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty());
    let total_xp = has_xp.then(|| {
        items.iter().map(|i| i.mastery_xp).sum::<f64>()
            + mission_mastery_xp(inv, &ctx.exports.regions)
            + intrinsic_mastery_xp(inv)
    });

    let mut by_category: std::collections::BTreeMap<String, CategoryStats> = Default::default();
    let mut totals = CategoryStats::default();
    for item in &items {
        for s in [
            &mut totals,
            by_category.entry(item.category.clone()).or_default(),
        ] {
            s.total += 1;
            match item.status {
                MasteryStatus::Mastered => s.mastered += 1,
                MasteryStatus::Progress => s.in_progress += 1,
                MasteryStatus::Missing => s.missing += 1,
            }
        }
    }
    MasteryProgress {
        stats: MasteryStats {
            total: totals.total,
            mastered: totals.mastered,
            in_progress: totals.in_progress,
            missing: totals.missing,
            by_category,
            profile_mastery: profile_mastery(inv, total_xp),
            completion: account_completion(inv, &ctx.exports.regions),
        },
        items,
        from_profile: false,
    }
}

/// Test/diagnostic helper: the masterable catalog without any inventory.
pub fn masterable_names(ctx: &Ctx) -> Vec<String> {
    ctx.masterable_items().into_iter().map(|m| m.name).collect()
}

// ---- runtime ----

/// Registers the state; the inventory itself comes from the inventory service.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    app.manage(MasteryState {
        bundled: BundledData::from_app(app.handle()).ok().map(Arc::new),
        exports: OnceLock::new(),
    });
    Ok(())
}

/// From the inventory snapshot; without one, from the cached public profile (a full snapshot
/// always wins). `None` when there is neither.
pub fn current_progress(app: &tauri::AppHandle) -> Option<MasteryProgress> {
    let (inv, from_profile) = match app.state::<Arc<Inventory>>().get() {
        Some(inv) => (inv, false),
        None => (
            Arc::new(crate::services::profile::profile_inventory(app)?),
            true,
        ),
    };
    let (db, locale) = app.state::<ItemDbState>().ui_snapshot();
    let exports = app.state::<MasteryState>().exports();
    let mut progress = compute_mastery_progress(
        &inv,
        &Ctx {
            db: &db,
            exports: &exports,
            locale: &locale,
        },
    );
    progress.from_profile = from_profile;
    Some(progress)
}

#[tauri::command]
#[specta::specta]
pub async fn get_mastery_progress(
    app: tauri::AppHandle,
) -> Result<Option<MasteryProgress>, String> {
    tauri::async_runtime::spawn_blocking(move || current_progress(&app))
        .await
        .map_err(|e| e.to_string())
}
