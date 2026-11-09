//! Item database: bundled `warframe-public-export-plus` + `@wfcd/items`, gap-filled from DE's live
//! PublicExport overlay. Port of WFHelper's `services/itemDatabase.ts`. English `name` is the join
//! key everywhere; `displayName` carries the game-language name for display only.
//!
//! shortcut: wiki card art (`wikiItemArt`/`wikiModArt`, 210 KB of tables) is not ported, so mods
//! keep DE's flat texture; port the tables when the framed art is wanted.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::services::bundled_data::BundledData;
use crate::services::game_locale::GameLocale;
use crate::services::public_export::Overlay;

#[cfg(test)]
mod tests;

pub(crate) const WFCD_CDN: &str = "https://cdn.warframestat.us/img/";
const BROWSE_WF: &str = "https://browse.wf";

const PEP_EXPORTS: [(&str, &str); 16] = [
    ("ExportWarframes", "Warframe"),
    ("ExportWeapons", "Weapon"),
    ("ExportSentinels", "Companion"),
    ("ExportResources", "Resource"),
    ("ExportKeys", "Key"),
    ("ExportRecipes", "Recipe"),
    ("ExportGear", "Gear"),
    ("ExportArcanes", "Arcane"),
    ("ExportUpgrades", "Mod"),
    ("ExportMisc", "Misc"),
    ("ExportRelics", "Relic"),
    ("ExportRailjackWeapons", "Railjack"),
    ("ExportFusionBundles", "Fusion"),
    ("ExportCustoms", "Cosmetic"),
    ("ExportFlavour", "Cosmetic"),
    ("ExportDrones", "Gear"),
];

const WFCD_CATEGORIES: [&str; 16] = [
    "Warframes",
    "Primary",
    "Secondary",
    "Melee",
    "Sentinels",
    "Pets",
    "Archwing",
    "Arch-Gun",
    "Arch-Melee",
    "Mods",
    "Resources",
    "Misc",
    "Relics",
    "Fish",
    "Gear",
    "Arcanes",
];

macro_rules! re {
    ($name:ident, $pat:literal) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}
re!(LEADING_TOKEN, r"^<[^>]{1,24}>\s*");
re!(LOWER_UPPER, r"([a-z])([A-Z])");
re!(ACRONYM, r"([A-Z]+)([A-Z][a-z])");
re!(VOID_PROJECTION, r"(?i)VoidProjection");
re!(
    BUILD_PATH,
    r"(?i)/Types/Recipes/|/WeaponParts?/|/WarframeParts?/|/LandingCraftRecipes/"
);
re!(
    BUILD_WORD,
    r"\b(blueprint|barrel|receiver|stock|blade|handle|hilt|chassis|systems|neuroptics|fuselage|engines|avionics|carapace|cerebrum|pod|wings|harness|link|disc|gauntlet|grip|ornament)\b"
);
re!(
    WEAPON_PART_RECIPE,
    r"(?i)/Types/Recipes/Weapons/WeaponParts?/"
);
re!(ENDS_COMPONENT, r"(?i)Component$");
re!(ENDS_BLUEPRINT, r"(?i)Blueprint$");
re!(ENDS_BLUEPRINT_WORD, r"(?i)\bblueprint$");
re!(RECIPE_PATH, r"(?i)/Types/Recipes/");
re!(MECH_PART, r"(?i)/InfestedMicroplanet/Resources/Mechs/");
// Rust's regex has no backreference, so the frame name is captured twice and compared.
re!(
    COMPANION_FRAME,
    r"^/Lotus/Powersuits/([^/]+)/Kavat/([^/]+?)(Prime)?KavatPowerSuit$"
);
re!(NON_ALNUM, r"[^a-z0-9]+");
re!(
    SLUG,
    r"^[\p{Ll}\p{N}](?:[\p{Ll}\p{M}\p{N}_()'’-]{0,118}[\p{Ll}\p{N})])?$"
);

// ---- shared helpers (config/shared/displayName.ts, relicRarity.ts, wfm.ts) ----

/// DE dict values carry a leading category marker (`<ARCHWING> Amesha`, `<ENDO>`).
pub fn sanitize_display_name(name: &str) -> String {
    let raw = name.trim();
    let stripped = LEADING_TOKEN.replace(raw, "").trim().to_owned();
    if stripped.is_empty() {
        raw.to_owned()
    } else {
        stripped
    }
}

pub fn fallback_name_from_unique_name(unique_name: &str) -> String {
    if unique_name.is_empty() {
        return "Unknown".into();
    }
    let mut last = unique_name
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown");
    if unique_name.to_lowercase().contains("/lotus/language/") {
        last = last.strip_suffix("Name").unwrap_or(last);
    }
    let spaced = LOWER_UPPER.replace_all(last, "${1} ${2}");
    sanitize_display_name(&ACRONYM.replace_all(&spaced, "${1} ${2}"))
}

pub(crate) fn normalize_for_slug(value: &str) -> Option<String> {
    let lowered = value
        .trim()
        .to_lowercase()
        .replace(['\'', '\u{2019}', '\u{2018}'], "");
    let slug = NON_ALNUM
        .replace_all(&lowered, "_")
        .trim_matches('_')
        .to_owned();
    (!slug.is_empty()).then_some(slug)
}

/// A slug warframe.market minted survives verbatim; a display name gets folded.
pub fn normalize_wfm_slug(value: &str) -> Option<String> {
    let v = value.trim();
    if SLUG.is_match(v) {
        Some(v.to_owned())
    } else {
        normalize_for_slug(v)
    }
}

pub(crate) fn relic_rarity(refinement: &str, chance: f64, fallback: &str) -> String {
    let table: &[(f64, &str)] = match refinement {
        "intact" => &[(25.33, "Common"), (11.0, "Uncommon"), (2.0, "Rare")],
        "exceptional" => &[(23.33, "Common"), (13.0, "Uncommon"), (4.0, "Rare")],
        "flawless" => &[(20.0, "Common"), (17.0, "Uncommon"), (6.0, "Rare")],
        "radiant" => &[(16.67, "Common"), (20.0, "Uncommon"), (10.0, "Rare")],
        _ => return fallback.to_owned(),
    };
    table
        .iter()
        .find(|(expected, _)| (chance - expected).abs() < 0.5)
        .map_or_else(|| fallback.to_owned(), |(_, r)| (*r).to_owned())
}

/// DE chance bands mislabel relic commons, so relic rows derive rarity from refinement and chance.
pub(crate) fn corrected_drop_rarity(location: &str, chance: f64, rarity: &str) -> String {
    let lower = location.to_lowercase();
    if !lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| w == "relic")
    {
        return rarity.to_owned();
    }
    let refinement = ["intact", "exceptional", "flawless", "radiant"]
        .into_iter()
        .find(|r| lower.contains(&format!("({r})")))
        .unwrap_or("intact");
    relic_rarity(refinement, chance, rarity)
}

// ---- types ----

#[derive(Clone, Default, Serialize, specta::Type)]
pub struct DropInfo {
    pub location: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub chance: f64,
    pub rarity: String,
}

#[derive(Clone, Default, Serialize, specta::Type)]
pub struct RecipeIngredient {
    #[serde(rename = "uniqueName")]
    pub unique_name: String,
    pub count: u32,
}

#[derive(Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecipeData {
    pub build_price: u32,
    pub build_time: u32,
    pub num: u32,
    pub blueprint_unique_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reusable_blueprint: Option<bool>,
    pub ingredients: Vec<RecipeIngredient>,
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub unique_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradable: Option<bool>,
    pub item_count: u32,
    pub drops: Vec<DropInfo>,
}

/// One row of the renderer's item lookup (`getRendererLookup`).
#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemDbEntry {
    /// English. Every by-name lookup is built from this.
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_is_fallback: Option<bool>,
    /// Active game language, present only when it differs from `name`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_art: Option<bool>,
    pub category: String,
    pub image_url: Option<String>,
    /// @wfcd art, for when `image_url` (DE's icon path on browse.wf) is missing on the host.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_image_url: Option<String>,
    pub is_prime: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tradable: Option<bool>,
    pub mastery_req: u32,
    pub vaulted: bool,
    pub exalted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incarnon: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masterable: Option<bool>,
    #[serde(rename = "type")]
    pub kind: String,
    pub is_build_component: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component_of: Option<String>,
    pub description: String,
    pub product_category: Option<String>,
    pub ducats: Option<u32>,
    pub components: Vec<ComponentInfo>,
    pub drops: Vec<DropInfo>,
    pub wikia_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<RecipeData>,
    /// For blueprint entries: uniqueName of the item this blueprint crafts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builds_product: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reusable_blueprint: Option<bool>,
    /// Mods and arcanes: DE's `fusionLimit`, the rank cap (0 for mods that cannot be ranked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rank: Option<u32>,
}

#[derive(Clone, Default)]
pub struct Comp {
    pub unique_name: String,
    pub name: String,
    pub name_key: Option<String>,
    pub image_name: Option<String>,
    pub tradable: Option<bool>,
    pub ducats: Option<u32>,
    pub item_count: u32,
    pub drops: Vec<DropInfo>,
}

#[derive(Clone, Default)]
pub struct Item {
    pub name: String,
    pub name_is_fallback: bool,
    /// `/Lotus/Language/...` key `name` was resolved from, for game-language lookup.
    pub name_key: Option<String>,
    pub category: String,
    pub image_url: Option<String>,
    /// @wfcd art, for when `image_url` (DE's icon path on browse.wf) is missing on the host.
    pub fallback_image_url: Option<String>,
    pub browse_wf_url: Option<String>,
    pub is_prime: bool,
    pub mastery_req: u32,
    pub masterable: Option<bool>,
    pub tradable: Option<bool>,
    pub vaulted: bool,
    pub exalted: bool,
    pub incarnon: bool,
    pub description: String,
    pub product_category: Option<String>,
    pub ducats: Option<u32>,
    pub kind: String,
    pub wikia_url: Option<String>,
    pub components: Vec<Comp>,
    pub drops: Vec<DropInfo>,
    pub is_build_component: bool,
    pub component_of: Option<String>,
    /// DE offers the blueprint in the in-game Market for credits.
    pub market_buyable: bool,
    pub market_credits: Option<f64>,
    pub dojo_research: bool,
    pub max_rank: Option<u32>,
}

/// Everything a build reads, so tests can feed small fixtures instead of the 69 MB bundle.
#[derive(Default)]
pub struct Sources {
    /// Bundled `Export*` tables by name.
    pub exports: HashMap<String, Map<String, Value>>,
    pub dict: Map<String, Value>,
    /// `@wfcd/items` as `BundledData::wfcd_items` returns it.
    pub wfcd: Vec<Value>,
    /// DE live overlay tables by export name.
    pub overlay: BTreeMap<String, Map<String, Value>>,
}

impl Sources {
    pub fn load(bundled: &BundledData, overlay: Option<Overlay>) -> Self {
        let exports = PEP_EXPORTS
            .iter()
            .map(|(k, _)| *k)
            .chain(["ExportDojoRecipes"])
            .filter_map(|k| Some((k.to_owned(), bundled.export(k)?)))
            .collect();
        let wfcd = bundled.wfcd_items(&WFCD_CATEGORIES).unwrap_or_else(|e| {
            tracing::warn!("@wfcd/items not available: {e}");
            vec![]
        });
        Self {
            exports,
            dict: bundled.dict("en").unwrap_or_default(),
            wfcd,
            overlay: overlay.map(|o| o.exports).unwrap_or_default(),
        }
    }
}

// ---- value accessors (JS truthiness: empty strings count as absent) ----

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k)?.as_str().filter(|x| !x.is_empty())
}
fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k)?.as_bool()
}
fn n(v: &Value, k: &str) -> Option<f64> {
    v.get(k)?.as_f64().filter(|x| x.is_finite())
}
/// `normalizeDucats`: a non-negative number rounded, else none.
fn ducats(v: &Value, k: &str) -> Option<u32> {
    n(v, k).filter(|x| *x >= 0.0).map(|x| x.round() as u32)
}

fn parse_drops(v: &Value) -> Vec<DropInfo> {
    v.get("drops")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|d| {
                    let location = s(d, "location").unwrap_or("");
                    let chance = n(d, "chance").unwrap_or(0.0);
                    let rarity = s(d, "rarity").unwrap_or("");
                    DropInfo {
                        location: location.into(),
                        kind: s(d, "type").unwrap_or("").into(),
                        chance,
                        rarity: corrected_drop_rarity(location, chance, rarity),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `@wfcd/items` row, drops rarity-corrected.
struct Wfcd {
    unique_name: String,
    name: Option<String>,
    category: Option<String>,
    image_name: Option<String>,
    thumbnail: Option<String>,
    mastery_req: u32,
    masterable: Option<bool>,
    tradable: Option<bool>,
    vaulted: bool,
    exalted: bool,
    incarnon: bool,
    description: String,
    product_category: Option<String>,
    kind: String,
    wikia_url: Option<String>,
    ducats: Option<u32>,
    components: Vec<Comp>,
    drops: Vec<DropInfo>,
}

fn parse_wfcd(v: &Value) -> Option<Wfcd> {
    let components = v
        .get("components")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|c| Comp {
                    unique_name: s(c, "uniqueName").unwrap_or("").into(),
                    name: s(c, "name").unwrap_or("").into(),
                    name_key: None,
                    image_name: s(c, "imageName").map(Into::into),
                    tradable: b(c, "tradable"),
                    ducats: ducats(c, "ducats"),
                    item_count: n(c, "itemCount")
                        .filter(|x| *x > 0.0)
                        .map_or(1, |x| x as u32),
                    drops: parse_drops(c),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Wfcd {
        unique_name: s(v, "uniqueName")?.into(),
        name: s(v, "name").map(Into::into),
        category: s(v, "category").map(Into::into),
        image_name: s(v, "imageName").map(Into::into),
        thumbnail: s(v, "wikiaThumbnail").map(Into::into),
        mastery_req: n(v, "masteryReq").map_or(0, |x| x as u32),
        masterable: b(v, "masterable"),
        tradable: b(v, "tradable"),
        vaulted: b(v, "vaulted").unwrap_or(false),
        exalted: b(v, "exalted").unwrap_or(false),
        incarnon: v
            .get("tags")
            .and_then(Value::as_array)
            .is_some_and(|t| t.iter().any(|x| x == "Incarnon")),
        description: s(v, "description").unwrap_or("").into(),
        product_category: s(v, "productCategory").map(Into::into),
        kind: s(v, "type").unwrap_or("").into(),
        wikia_url: s(v, "wikiaUrl").map(Into::into),
        ducats: ducats(v, "ducats"),
        components,
        drops: parse_drops(v),
    })
}

// ---- image urls ----

/// Icons load straight from the public host that has them (browse.wf, cdn.warframestat.us, DE's
/// content CDN, the wiki); only http(s) URLs reach the UI. The webview's HTTP cache keeps them.
pub fn public_image_url(source: Option<&str>) -> Option<String> {
    let trimmed = source?.trim();
    let parsed = url::Url::parse(trimmed).ok()?;
    matches!(parsed.scheme(), "http" | "https").then(|| trimmed.to_owned())
}

fn choose_image_url(urls: &[Option<&str>]) -> Option<String> {
    public_image_url(
        urls.iter()
            .flatten()
            .find(|u| !u.trim().is_empty())
            .copied(),
    )
}

fn wfcd_image_url(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| format!("{WFCD_CDN}{n}"))
}

// ---- component naming ----

fn is_likely_build_component(unique_name: &str, name: &str) -> bool {
    !unique_name.is_empty()
        && (BUILD_PATH.is_match(unique_name) || BUILD_WORD.is_match(&name.to_lowercase()))
}

fn resolve_component_tradable(
    component: Option<bool>,
    parent: Option<bool>,
    unique_name: &str,
) -> Option<bool> {
    match component.or(parent) {
        Some(false) if WEAPON_PART_RECIPE.is_match(unique_name) => None,
        other => other,
    }
}

fn build_component_display_name(parent: &str, component: &str, force_blueprint: bool) -> String {
    let parent = sanitize_display_name(parent);
    let component = sanitize_display_name(component);
    if parent.is_empty() && component.is_empty() {
        return "Unknown".into();
    }
    if component.is_empty() {
        return parent;
    }
    let mut full = component;
    if force_blueprint && !ENDS_BLUEPRINT_WORD.is_match(&full) {
        full = format!("{full} Blueprint");
    }
    if parent.is_empty() {
        return full;
    }
    // A component overlapping the parent's tail is already a full item name.
    let pw: Vec<&str> = parent.split_whitespace().collect();
    let cw: Vec<&str> = full.split_whitespace().collect();
    for k in (1..=pw.len().min(cw.len())).rev() {
        if pw[pw.len() - k..].join(" ").to_lowercase() == cw[..k].join(" ").to_lowercase() {
            return full;
        }
    }
    format!("{parent} {full}")
}

fn component_alias_unique_names(unique_name: &str) -> Vec<String> {
    if unique_name.is_empty() {
        vec![]
    } else if ENDS_COMPONENT.is_match(unique_name) {
        vec![
            ENDS_COMPONENT
                .replace(unique_name, "Blueprint")
                .into_owned(),
        ]
    } else if !ENDS_BLUEPRINT.is_match(unique_name) && RECIPE_PATH.is_match(unique_name) {
        vec![format!("{unique_name}Blueprint")]
    } else {
        vec![]
    }
}

// ---- the database ----

struct Bucket {
    strong: Option<usize>,
    first: usize,
}

#[derive(Default)]
pub struct ItemDb {
    /// Build order matters: it breaks ties in the name index.
    items: Vec<(String, Item)>,
    index: HashMap<String, usize>,
    wfcd_images: HashMap<String, String>,
    sentinel_weapons: Vec<(String, String)>,
    recipes: HashMap<String, RecipeData>,
    result_by_blueprint: HashMap<String, String>,
    reusable: HashSet<String>,
    market_credits: HashMap<String, Option<f64>>,
    dojo_results: HashSet<String>,
    by_name: HashMap<String, Bucket>,
    by_slug: HashMap<String, Bucket>,
}

/// Merges `{...overlay, ...base}`: base wins values, overlay fixes key order.
fn merge_tables<'a>(
    overlay: Option<&'a Map<String, Value>>,
    base: Option<&'a Map<String, Value>>,
) -> Vec<(&'a String, &'a Value)> {
    let mut out: Vec<(&String, &Value)> = overlay.into_iter().flatten().collect();
    let pos: HashMap<&String, usize> = out.iter().enumerate().map(|(i, (k, _))| (*k, i)).collect();
    for (k, v) in base.into_iter().flatten() {
        match pos.get(k) {
            Some(&i) => out[i].1 = v,
            None => out.push((k, v)),
        }
    }
    out
}

impl ItemDb {
    pub fn build(src: &Sources) -> Self {
        let mut db = Self::default();
        db.load_pep(src);
        db.build_recipe_index(src);
        db.load_wfcd(&src.wfcd);
        db.inherit_sentinel_weapon_vaulting();
        db.inherit_companion_frame_vaulting();
        db.apply_mech_part_tradability();
        db.link_blueprints_to_results();
        db.inherit_blueprint_display_from_results();
        db.apply_acquisition_sources();
        db.resolve_all_images();
        db.index_names();
        tracing::info!("[ItemDB] total: {} items", db.items.len());
        db
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get(&self, unique_name: &str) -> Option<&Item> {
        self.items
            .get(*self.index.get(unique_name)?)
            .map(|(_, i)| i)
    }

    /// Items in build order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Item)> {
        self.items.iter().map(|(k, i)| (k, i))
    }

    fn get_mut(&mut self, unique_name: &str) -> Option<&mut Item> {
        let i = *self.index.get(unique_name)?;
        Some(&mut self.items[i].1)
    }

    /// JS object assignment: replaces in place, so the original key keeps its position.
    fn set(&mut self, unique_name: &str, item: Item) {
        match self.index.get(unique_name) {
            Some(&i) => self.items[i].1 = item,
            None => {
                self.index.insert(unique_name.to_owned(), self.items.len());
                self.items.push((unique_name.to_owned(), item));
            }
        }
    }

    /// True when building this blueprint does not consume the owned copy.
    pub fn is_reusable_blueprint(&self, unique_name: &str) -> bool {
        self.reusable.contains(unique_name)
    }

    fn load_pep(&mut self, src: &Sources) {
        let resolve = |key: Option<&str>| -> Option<String> {
            let key = key?;
            if !key.starts_with('/') {
                return Some(key.to_owned());
            }
            src.dict
                .get(key)?
                .as_str()
                .filter(|s| !s.is_empty())
                .map(Into::into)
        };
        let mut count = 0;
        for (export, category) in PEP_EXPORTS {
            let base = src.exports.get(export);
            for (unique_name, item) in merge_tables(src.overlay.get(export), base) {
                if unique_name.is_empty() || unique_name == "default" {
                    continue;
                }
                // Relics have no name field - build from era + category (e.g. "Axi A2 Relic").
                let relic_name = (export == "ExportRelics")
                    .then(|| {
                        Some(format!(
                            "{} {} Relic",
                            s(item, "era")?,
                            s(item, "category")?
                        ))
                    })
                    .flatten();
                // Recipes have no name - resolve via resultType (e.g. "Sands of Inaros Blueprint").
                let mut recipe_name = None;
                let mut recipe_fallback = false;
                if export == "ExportRecipes"
                    && s(item, "name").is_none()
                    && let Some(r) = s(item, "resultType")
                        .and_then(|r| self.get(r))
                        .filter(|r| !r.name.is_empty())
                {
                    recipe_name = Some(format!("{} Blueprint", r.name));
                    recipe_fallback = r.name_is_fallback;
                }
                let source_name = relic_name
                    .clone()
                    .or(recipe_name.clone())
                    .or_else(|| resolve(s(item, "name")));
                let resolved = sanitize_display_name(
                    &source_name
                        .clone()
                        .unwrap_or_else(|| fallback_name_from_unique_name(unique_name)),
                );
                // For recipes without an icon, inherit from the result item.
                let recipe_icon = s(item, "icon").is_none().then(|| {
                    s(item, "resultType").and_then(|r| self.get(r)?.browse_wf_url.clone())
                });
                // Only a plain dict resolve is re-localizable: relic and recipe names are composed.
                let name_key = (relic_name.is_none() && recipe_name.is_none())
                    .then(|| {
                        s(item, "name")
                            .filter(|n| n.starts_with('/'))
                            .map(Into::into)
                    })
                    .flatten();
                // DE lists relics the bundled package lacks only under ExportResources.
                let category = if category == "Resource"
                    && !base.is_some_and(|b| b.contains_key(unique_name))
                    && VOID_PROJECTION.is_match(unique_name)
                {
                    "Relic"
                } else {
                    category
                };
                let entry = Item {
                    is_prime: resolved.contains("Prime"),
                    name: resolved,
                    name_is_fallback: source_name.is_none() || recipe_fallback,
                    name_key,
                    category: category.into(),
                    browse_wf_url: s(item, "icon")
                        .map(|i| format!("{BROWSE_WF}{i}"))
                        .or(recipe_icon.flatten()),
                    mastery_req: n(item, "masteryReq").map_or(0, |x| x as u32),
                    tradable: b(item, "tradable"),
                    vaulted: b(item, "vaulted").unwrap_or(false),
                    description: resolve(s(item, "description")).unwrap_or_default(),
                    product_category: s(item, "productCategory").map(Into::into),
                    ducats: n(item, "primeSellingPrice").map(|p| p.max(0.0).round() as u32),
                    max_rank: matches!(export, "ExportUpgrades" | "ExportArcanes")
                        .then(|| n(item, "fusionLimit").map(|x| x.max(0.0) as u32))
                        .flatten(),
                    ..Item::default()
                };
                self.set(unique_name, entry);
                count += 1;
                if export == "ExportSentinels"
                    && let Some(w) = s(item, "defaultWeapon")
                {
                    self.sentinel_weapons.push((w.into(), unique_name.clone()));
                }
            }
        }
        tracing::info!("[ItemDB] public-export-plus: {count} items indexed");
    }

    fn build_recipe_index(&mut self, src: &Sources) {
        let base = src.exports.get("ExportRecipes");
        for (key, item) in merge_tables(src.overlay.get("ExportRecipes"), base) {
            let (Some(result), Some(ingredients)) = (
                s(item, "resultType"),
                item.get("ingredients").and_then(Value::as_array),
            ) else {
                continue;
            };
            self.result_by_blueprint.insert(key.clone(), result.into());
            let reusable = b(item, "consumeOnUse") == Some(false);
            if reusable {
                self.reusable.insert(key.clone());
            }
            // DE's raw export has no market fields, so an overlay-only recipe cannot claim one.
            if base.is_some_and(|b| b.contains_key(key))
                && b(item, "excludeFromMarket") != Some(true)
            {
                let credits = n(item, "creditsCost");
                // Several recipes can build one item, so a listed price beats a missing one.
                let slot = self.market_credits.entry(result.into()).or_insert(None);
                if slot.is_none() {
                    *slot = credits;
                }
            }
            self.recipes.insert(
                result.into(),
                RecipeData {
                    build_price: n(item, "buildPrice").map_or(0, |x| x as u32),
                    build_time: n(item, "buildTime").map_or(0, |x| x as u32),
                    num: n(item, "num").filter(|x| *x > 0.0).map_or(1, |x| x as u32),
                    blueprint_unique_name: key.clone(),
                    reusable_blueprint: reusable.then_some(true),
                    ingredients: ingredients
                        .iter()
                        .map(|i| RecipeIngredient {
                            unique_name: s(i, "ItemType").unwrap_or("").into(),
                            count: n(i, "ItemCount")
                                .filter(|x| *x > 0.0)
                                .map_or(1, |x| x as u32),
                        })
                        .collect(),
                },
            );
        }
        // Dojo research is keyed by the recipe uniqueName, so it joins to an item via resultType.
        let research = src
            .exports
            .get("ExportDojoRecipes")
            .and_then(|d| d.get("research"))
            .and_then(Value::as_object);
        for key in research.into_iter().flat_map(|r| r.keys()) {
            if let Some(result) = self.result_by_blueprint.get(key) {
                self.dojo_results.insert(result.clone());
            }
        }
    }

    fn load_wfcd(&mut self, raw: &[Value]) {
        let mut items: Vec<Wfcd> = raw.iter().filter_map(parse_wfcd).collect();
        restore_weapon_ingredient_drops(&mut items);
        // Top-level WFCD components already carry their complete tradable name.
        let standalone: HashMap<&str, String> = items
            .iter()
            .filter_map(|i| {
                Some((
                    i.unique_name.as_str(),
                    sanitize_display_name(i.name.as_deref()?),
                ))
            })
            .collect();

        for item in &items {
            let item_name = item.name.as_deref().unwrap_or("");
            let wfcd_image = wfcd_image_url(item.image_name.as_deref());
            let entry = Item {
                name: sanitize_display_name(if item_name.is_empty() {
                    "Unknown"
                } else {
                    item_name
                }),
                category: item.category.clone().unwrap_or_else(|| "Misc".into()),
                image_url: choose_image_url(&[item.thumbnail.as_deref(), wfcd_image.as_deref()]),
                is_prime: sanitize_display_name(item_name).contains("Prime"),
                mastery_req: item.mastery_req,
                masterable: item.masterable,
                tradable: item.tradable,
                vaulted: item.vaulted,
                exalted: item.exalted,
                incarnon: item.incarnon,
                components: item.components.clone(),
                drops: item.drops.clone(),
                description: item.description.clone(),
                product_category: item.product_category.clone(),
                kind: item.kind.clone(),
                wikia_url: item.wikia_url.clone(),
                ducats: item.ducats,
                ..Item::default()
            };
            if let Some(u) = &entry.image_url {
                self.wfcd_images.insert(item.unique_name.clone(), u.clone());
            }

            for comp in item.components.iter().filter(|c| !c.unique_name.is_empty()) {
                self.merge_component(item, &entry, comp, wfcd_image.as_deref(), &standalone);
            }

            match self.get_mut(&item.unique_name) {
                None => self.set(&item.unique_name, entry),
                Some(existing) => {
                    // wikiCardArtUrl is not ported, so the card slot is empty.
                    existing.image_url = choose_image_url(&[
                        existing.browse_wf_url.as_deref(),
                        item.thumbnail.as_deref(),
                        wfcd_image.as_deref(),
                    ]);
                    if (existing.name_is_fallback || existing.name.starts_with("/Lotus/"))
                        && item.name.is_some()
                    {
                        existing.name = sanitize_display_name(item_name);
                        existing.name_is_fallback = false;
                        existing.is_prime = existing.name.contains("Prime");
                    }
                    existing.tradable = existing.tradable.or(item.tradable);
                    // DE's export has no vault status at all; @wfcd is the only source.
                    existing.vaulted |= entry.vaulted;
                    existing.drops = item.drops.clone();
                    existing.wikia_url = item.wikia_url.clone();
                    existing.exalted = item.exalted;
                    existing.incarnon |= entry.incarnon;
                    if item.masterable.is_some() {
                        existing.masterable = item.masterable;
                    }
                    existing.components = item.components.clone();
                    if existing.product_category.is_none() {
                        existing.product_category = item.product_category.clone();
                    }
                    if existing.kind.is_empty() {
                        existing.kind = item.kind.clone();
                    }
                    if existing.description.is_empty() {
                        existing.description = item.description.clone();
                    }
                    if item.ducats.is_some() {
                        existing.ducats = item.ducats;
                    }
                }
            }
        }
    }

    fn merge_component(
        &mut self,
        item: &Wfcd,
        parent: &Item,
        comp: &Comp,
        parent_image: Option<&str>,
        standalone: &HashMap<&str, String>,
    ) {
        let looks_like_part = is_likely_build_component(&comp.unique_name, &comp.name);
        let aliases = component_alias_unique_names(&comp.unique_name);
        let force_blueprint = ENDS_COMPONENT.is_match(&comp.unique_name);
        let is_standalone = standalone
            .get(comp.unique_name.as_str())
            .is_some_and(|n| n.to_lowercase() == sanitize_display_name(&comp.name).to_lowercase());
        let parent_name = item.name.as_deref().unwrap_or("");
        let name = build_component_display_name(
            if is_standalone { "" } else { parent_name },
            &comp.name,
            force_blueprint,
        );

        // "blueprint.png" is a generic placeholder that 404s on the WFCD CDN, so blueprint
        // components fall back to the parent item's image.
        let existing_browse = self
            .get(&comp.unique_name)
            .and_then(|e| e.browse_wf_url.clone());
        let comp_image = comp.image_name.as_deref().filter(|n| *n != "blueprint.png");
        let image_url = choose_image_url(&[
            existing_browse.as_deref(),
            parent.image_url.as_deref(),
            wfcd_image_url(comp_image).as_deref(),
        ]);
        let tradable = resolve_component_tradable(comp.tradable, item.tradable, &comp.unique_name);
        let kind = if comp.name.is_empty() {
            if parent.kind.is_empty() {
                "Part".into()
            } else {
                parent.kind.clone()
            }
        } else {
            format!("{} Part", comp.name)
        };
        let entry = Item {
            name,
            image_url: image_url.clone(),
            tradable,
            kind,
            components: vec![],
            drops: comp.drops.clone(),
            description: String::new(),
            is_build_component: looks_like_part,
            component_of: Some(item.unique_name.clone()),
            ducats: comp.ducats,
            ..parent.clone()
        };
        if let Some(u) = &image_url {
            self.wfcd_images.insert(comp.unique_name.clone(), u.clone());
        }

        match self.get_mut(&comp.unique_name) {
            None => self.set(&comp.unique_name, entry),
            Some(existing) => {
                if !entry.name.is_empty()
                    && (existing.name.is_empty()
                        || existing.name_is_fallback
                        || existing.name.starts_with("/Lotus/")
                        || looks_like_part)
                {
                    existing.name = entry.name.clone();
                    existing.name_is_fallback = false;
                }
                if existing.image_url.is_none() {
                    existing.image_url = entry.image_url.clone();
                }
                existing.vaulted |= entry.vaulted;
                existing.tradable = existing.tradable.or(entry.tradable);
                if existing.kind.is_empty() {
                    existing.kind = entry.kind.clone();
                }
                if existing.product_category.is_none() {
                    existing.product_category = entry.product_category.clone();
                }
                if looks_like_part {
                    existing.is_build_component = true;
                    existing
                        .component_of
                        .get_or_insert_with(|| item.unique_name.clone());
                }
            }
        }

        for alias in aliases {
            let alias_name = build_component_display_name(parent_name, &comp.name, true);
            let alias_image =
                wfcd_image_url(comp.image_name.as_deref()).or_else(|| parent_image.map(Into::into));
            let Some(bp) = self.get_mut(&alias) else {
                continue;
            };
            if !alias_name.is_empty() {
                bp.name = alias_name;
                bp.name_is_fallback = false;
            }
            let image = choose_image_url(&[
                bp.browse_wf_url.as_deref(),
                parent.image_url.as_deref(),
                alias_image.as_deref(),
            ]);
            if bp.image_url.is_none() {
                bp.image_url = image;
            }
            bp.vaulted |= parent.vaulted;
            bp.tradable = bp.tradable.or(tradable).or(item.tradable);
            bp.is_build_component = true;
            bp.component_of
                .get_or_insert_with(|| item.unique_name.clone());
            if bp.kind.is_empty() && !comp.name.is_empty() {
                bp.kind = format!("{} Part", comp.name);
            }
        }
    }

    /// Bundled sentinel weapons have no relic drops for WFCD to derive vaulting from.
    fn inherit_sentinel_weapon_vaulting(&mut self) {
        for (weapon, sentinel) in self.sentinel_weapons.clone() {
            if self.get(&sentinel).is_some_and(|s| s.vaulted)
                && let Some(w) = self.get_mut(&weapon)
            {
                w.vaulted = true;
            }
        }
    }

    /// Venari drops from no relic, so WFCD never vaults her.
    fn inherit_companion_frame_vaulting(&mut self) {
        let hits: Vec<(String, String)> = self
            .items
            .iter()
            .filter_map(|(u, _)| {
                let c = COMPANION_FRAME.captures(u)?;
                (c[1] == c[2]).then(|| {
                    let prime = c.get(3).map_or("", |m| m.as_str());
                    (
                        u.clone(),
                        format!("/Lotus/Powersuits/{0}/{0}{prime}", &c[1]),
                    )
                })
            })
            .collect();
        for (companion, frame) in hits {
            if self.get(&frame).is_some_and(|f| f.vaulted) {
                self.get_mut(&companion).unwrap().vaulted = true;
            }
        }
    }

    /// WFCD inherits false resource tradability for crafted mech parts.
    fn apply_mech_part_tradability(&mut self) {
        for (u, item) in &mut self.items {
            if MECH_PART.is_match(u) {
                item.tradable = Some(true);
            }
        }
    }

    /// Recipe aliases inherit their crafted item's mappings.
    fn link_blueprints_to_results(&mut self) {
        for (bp, result) in self.result_by_blueprint.clone() {
            let Some(parent) = self
                .get(&result)
                .filter(|r| r.is_build_component)
                .and_then(|r| r.component_of.clone())
            else {
                continue;
            };
            if let Some(b) = self.get_mut(&bp).filter(|b| b.component_of.is_none()) {
                b.is_build_component = true;
                b.component_of = Some(parent);
            }
        }
    }

    /// ExportRecipes loads before ExportGear/Customs/Misc, so a recipe whose result sits in a
    /// later export missed both its name and its inherited icon.
    fn inherit_blueprint_display_from_results(&mut self) {
        for (bp, result) in self.result_by_blueprint.clone() {
            let (Some(r), true) = (self.get(&result).cloned(), self.index.contains_key(&bp)) else {
                continue;
            };
            let blueprint = self.get_mut(&bp).unwrap();
            if blueprint.browse_wf_url.is_none() {
                blueprint.browse_wf_url = r.browse_wf_url.clone();
            }
            // Only rename what nothing else named, and only when the result itself resolved.
            if r.name.is_empty() || !blueprint.name_is_fallback || r.name_is_fallback {
                continue;
            }
            // A warframe part component is already named "... Chassis Blueprint".
            let derived = sanitize_display_name(&if ENDS_BLUEPRINT_WORD.is_match(&r.name) {
                r.name.clone()
            } else {
                format!("{} Blueprint", r.name)
            });
            blueprint.name_is_fallback = false;
            if derived != blueprint.name {
                blueprint.is_prime = derived.contains("Prime");
                blueprint.name = derived;
            }
        }
    }

    /// Market and dojo facts live on the recipe, not the item, so stamp them once every loader ran.
    fn apply_acquisition_sources(&mut self) {
        for (result, credits) in self.market_credits.clone() {
            if let Some(e) = self.get_mut(&result) {
                e.market_buyable = true;
                if credits.is_some() {
                    e.market_credits = credits;
                }
            }
        }
        for result in self.dojo_results.clone() {
            if let Some(e) = self.get_mut(&result) {
                e.dojo_research = true;
            }
        }
    }

    fn resolve_all_images(&mut self) {
        for (u, item) in &mut self.items {
            if item.image_url.is_some() {
                continue;
            }
            item.image_url = match &item.browse_wf_url {
                Some(url) => public_image_url(Some(url)),
                None => self.wfcd_images.get(u).cloned(),
            };
        }
        for (u, item) in &mut self.items {
            item.fallback_image_url = self
                .wfcd_images
                .get(u)
                .filter(|w| Some(*w) != item.image_url.as_ref())
                .cloned();
        }
    }

    fn index_names(&mut self) {
        fn put(map: &mut HashMap<String, Bucket>, key: String, order: usize, strong: bool) {
            let bucket = map.entry(key).or_insert(Bucket {
                strong: None,
                first: order,
            });
            if strong && bucket.strong.is_none() {
                bucket.strong = Some(order);
            }
        }
        for (order, (_, item)) in self.items.iter().enumerate() {
            let strong =
                item.component_of.is_some() || item.ducats.is_some() || item.is_build_component;
            let name = item.name.trim().to_lowercase();
            if !name.is_empty() {
                put(&mut self.by_name, name, order, strong);
            }
            if let Some(slug) = normalize_wfm_slug(&item.name) {
                put(&mut self.by_slug, slug, order, strong);
            }
        }
    }

    /// First match in build order, preferring entries that carry part metadata.
    pub fn lookup_by_name_or_slug(
        &self,
        name: Option<&str>,
        slug: Option<&str>,
    ) -> Option<(&str, &Item)> {
        let name = name
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty());
        let slug = slug.and_then(normalize_wfm_slug);
        let buckets: Vec<&Bucket> = name
            .and_then(|n| self.by_name.get(&n))
            .into_iter()
            .chain(slug.and_then(|s| self.by_slug.get(&s)))
            .collect();
        let best = buckets
            .iter()
            .filter_map(|b| b.strong)
            .min()
            .or_else(|| buckets.iter().map(|b| b.first).min())?;
        let (u, item) = &self.items[best];
        Some((u, item))
    }

    // ---- renderer projection ----

    /// Recipes carry no name of their own; theirs is composed from the item they build, using
    /// DE's word-order pattern (Spanish/Russian lead with it, Japanese does not).
    fn localize_item_name(
        &self,
        locale: &GameLocale,
        unique_name: &str,
        name_key: Option<&str>,
        english: &str,
    ) -> String {
        if name_key.is_some() {
            return locale.localize(name_key, english);
        }
        let Some(result) = self
            .result_by_blueprint
            .get(unique_name)
            .and_then(|r| self.get(r))
        else {
            return english.to_owned();
        };
        locale
            .localize(
                Some("/Lotus/Language/Items/BlueprintAndItem"),
                "|ITEM| Blueprint",
            )
            .replace(
                "|ITEM|",
                &locale.localize(result.name_key.as_deref(), &result.name),
            )
    }

    /// `name` stays English because the UI joins on it; `displayName` appears only when the game
    /// language actually moved the name, so English users pay nothing.
    fn localized_pair(
        &self,
        locale: &GameLocale,
        unique_name: &str,
        name_key: Option<&str>,
        english: &str,
    ) -> (String, Option<String>) {
        if !locale.is_localizing() {
            return (english.to_owned(), None);
        }
        let localized = self.localize_item_name(locale, unique_name, name_key, english);
        let display = (localized != english).then_some(localized);
        (english.to_owned(), display)
    }

    /// `displayName` for anything built outside the lookup, keyed by uniqueName.
    pub fn localized_name(
        &self,
        locale: &GameLocale,
        unique_name: &str,
        english: &str,
    ) -> Option<String> {
        let key = self.get(unique_name).and_then(|i| i.name_key.as_deref());
        self.localized_pair(locale, unique_name, key, english).1
    }

    /// Game-language name (lowercase) -> English name, for text the game printed in its own
    /// language (trade dialogs). Empty in English, where nothing needs translating.
    pub fn english_by_localized(&self, locale: &GameLocale) -> HashMap<String, String> {
        if !locale.is_localizing() {
            return HashMap::new();
        }
        self.items
            .iter()
            .filter_map(|(key, item)| {
                let local = self.localized_name(locale, key, &item.name)?;
                Some((local.to_lowercase(), item.name.clone()))
            })
            .collect()
    }

    pub fn renderer_lookup(&self, locale: &GameLocale) -> BTreeMap<String, ItemDbEntry> {
        self.items
            .iter()
            .map(|(key, item)| {
                let (name, display_name) =
                    self.localized_pair(locale, key, item.name_key.as_deref(), &item.name);
                let entry = ItemDbEntry {
                    name,
                    name_is_fallback: item.name_is_fallback.then_some(true),
                    display_name,
                    card_art: item
                        .image_url
                        .as_deref()
                        .is_some_and(|u| u.contains("/mod-art/") || u.contains("/item-art/"))
                        .then_some(true),
                    category: item.category.clone(),
                    image_url: item.image_url.clone(),
                    fallback_image_url: item.fallback_image_url.clone(),
                    is_prime: item.is_prime,
                    tradable: item.tradable,
                    mastery_req: item.mastery_req,
                    vaulted: item.vaulted,
                    exalted: item.exalted,
                    incarnon: item.incarnon.then_some(true),
                    masterable: item.masterable,
                    kind: item.kind.clone(),
                    is_build_component: item.is_build_component,
                    component_of: item.component_of.clone(),
                    description: item.description.clone(),
                    product_category: item.product_category.clone(),
                    ducats: item.ducats,
                    components: item
                        .components
                        .iter()
                        .map(|c| {
                            // Set parts carry no key of their own, so borrow the part item's.
                            let ckey = c
                                .name_key
                                .as_deref()
                                .or_else(|| self.get(&c.unique_name)?.name_key.as_deref());
                            let (name, display_name) =
                                self.localized_pair(locale, &c.unique_name, ckey, &c.name);
                            ComponentInfo {
                                name,
                                display_name,
                                unique_name: c.unique_name.clone(),
                                tradable: c.tradable,
                                item_count: c.item_count,
                                drops: c.drops.clone(),
                            }
                        })
                        .collect(),
                    drops: item.drops.iter().take(20).cloned().collect(),
                    wikia_url: item.wikia_url.clone(),
                    recipe: self.recipes.get(key).cloned(),
                    builds_product: self
                        .result_by_blueprint
                        .get(key)
                        .filter(|r| self.index.contains_key(*r))
                        .cloned(),
                    reusable_blueprint: self.reusable.contains(key).then_some(true),
                    max_rank: item.max_rank,
                };
                (key.clone(), entry)
            })
            .collect()
    }
}

/// Before 1.1276 @wfcd/items listed the drops of a weapon's own parts on that weapon wherever it
/// is an ingredient; the ref it has now resolves to the weapon, which has none.
fn restore_weapon_ingredient_drops(items: &mut [Wfcd]) {
    let by_name: HashMap<String, usize> = items
        .iter()
        .enumerate()
        .map(|(i, w)| (w.unique_name.clone(), i))
        .collect();
    for i in 0..items.len() {
        for j in 0..items[i].components.len() {
            let comp = &items[i].components[j];
            let un = &comp.unique_name;
            if !comp.drops.is_empty() || comp.name == "Blueprint" {
                continue;
            }
            if !un.contains("/Weapons/") || un.contains("/WeaponParts/") {
                continue;
            }
            let Some(&w) = by_name.get(un) else { continue };
            if !items[w].drops.is_empty() {
                continue;
            }
            let prefix = format!("{} ", items[w].name.as_deref().unwrap_or("").to_lowercase());
            let mut drops: Vec<DropInfo> = items[w]
                .components
                .iter()
                .flat_map(|p| &p.drops)
                .filter(|d| d.kind.to_lowercase().starts_with(&prefix))
                .cloned()
                .collect();
            if drops.is_empty() {
                continue;
            }
            // shortcut: byte order stands in for JS localeCompare("en") on the @wfcd drop key.
            drops.sort_by_key(|d| {
                format!("{}:{}::{}", d.chance, d.location, d.rarity).to_uppercase()
            });
            items[i].components[j].drops = drops;
        }
    }
}

// ---- runtime state, commands, events ----

pub type ItemLookup = BTreeMap<String, ItemDbEntry>;

/// Managed state: the built database, the game language (reads the screen), the app language
/// (names shown in the UI), and the projection cache.
#[derive(Default)]
pub struct ItemDbState {
    db: Mutex<Arc<ItemDb>>,
    locale: Mutex<GameLocale>,
    ui: Mutex<GameLocale>,
    lookup: Mutex<Option<(&'static str, Arc<ItemLookup>)>>,
}

impl ItemDbState {
    pub fn replace(&self, db: ItemDb) {
        *self.db.lock().unwrap() = Arc::new(db);
        *self.lookup.lock().unwrap() = None;
    }

    /// The current database and game language, for services that read items.
    pub fn snapshot(&self) -> (Arc<ItemDb>, GameLocale) {
        (
            self.db.lock().unwrap().clone(),
            self.locale.lock().unwrap().clone(),
        )
    }

    /// The game language's table code, for what matches on-screen text.
    pub fn locale_code(&self) -> &'static str {
        self.locale.lock().unwrap().code()
    }

    /// The current database and app language, for services whose names reach the UI.
    pub fn ui_snapshot(&self) -> (Arc<ItemDb>, GameLocale) {
        (
            self.db.lock().unwrap().clone(),
            self.ui.lock().unwrap().clone(),
        )
    }

    /// The app language's name table code.
    pub fn ui_code(&self) -> &'static str {
        self.ui.lock().unwrap().code()
    }

    /// Returns the new code, or `None` when the language did not change.
    pub fn set_locale(&self, locale: GameLocale) -> Option<&'static str> {
        let mut cur = self.locale.lock().unwrap();
        if cur.code() == locale.code() {
            return None;
        }
        *cur = locale;
        Some(cur.code())
    }

    /// Like `set_locale`, for the app language; it also drops the projection cache.
    pub fn set_ui_locale(&self, locale: GameLocale) -> Option<&'static str> {
        let mut cur = self.ui.lock().unwrap();
        if cur.code() == locale.code() {
            return None;
        }
        *cur = locale;
        *self.lookup.lock().unwrap() = None;
        Some(cur.code())
    }

    /// Built once per database build and app language, then shared read-only.
    pub fn lookup(&self) -> Arc<BTreeMap<String, ItemDbEntry>> {
        let locale = self.ui.lock().unwrap().clone();
        let mut cache = self.lookup.lock().unwrap();
        if let Some((code, l)) = &*cache
            && *code == locale.code()
        {
            return l.clone();
        }
        let l = Arc::new(self.db.lock().unwrap().renderer_lookup(&locale));
        *cache = Some((locale.code(), l.clone()));
        l
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_item_database(state: tauri::State<ItemDbState>) -> BTreeMap<String, ItemDbEntry> {
    (*state.lookup()).clone()
}

/// The language the game was started with, from EE.log's `-language:`.
static LOG_LANGUAGE: Mutex<Option<&'static str>> = Mutex::new(None);

/// Records EE.log's `-language:` (codes the app does not know are ignored) and applies it when
/// the `game_language` setting is `auto`.
pub fn set_log_language(app: &tauri::AppHandle, code: &str) {
    use tauri::{Emitter, Manager};
    let Some(code) = crate::services::game_locale::GAME_LOCALES
        .iter()
        .find(|c| **c == code)
    else {
        return;
    };
    if LOG_LANGUAGE.lock().unwrap().replace(code) == Some(code) {
        return;
    }
    tracing::info!("game language from EE.log: {code}");
    // The "Auto (detected: …)" label re-reads on this event.
    let _ = app.emit("game-locale-updated", ());
    let setting = app.state::<crate::store::Store>().get().game_language;
    if let Err(e) = set_game_locale(setting, app.clone()) {
        tracing::warn!("game language from EE.log not applied: {e}");
    }
}

/// The game language EE.log reported, if any.
#[tauri::command]
#[specta::specta]
pub fn game_language_detected() -> Option<String> {
    LOG_LANGUAGE.lock().unwrap().map(str::to_owned)
}

/// Applies the `game_language` setting: a code, or `auto` for EE.log's language (else English).
#[tauri::command]
#[specta::specta]
pub fn set_game_locale(code: String, app: tauri::AppHandle) -> Result<(), String> {
    let detected = *LOG_LANGUAGE.lock().unwrap();
    apply_game_locale(
        &app,
        crate::services::game_locale::effective(&code, detected),
    )
}

fn apply_game_locale(app: &tauri::AppHandle, code: &str) -> Result<(), String> {
    use tauri::{Emitter, Manager};
    let locale = GameLocale::load(code, &BundledData::from_app(app)?);
    crate::services::reward_scan::era::set_words(
        crate::services::reward_scan::era::EraWords::build(|k| {
            Some(locale.localize(Some(k), "")).filter(|s| !s.is_empty())
        }),
    );
    crate::services::ocr_onnx::set_language(locale.code());
    #[cfg(windows)]
    crate::platform::windows::ocr::set_language(locale.code());
    let changed = app.state::<ItemDbState>().set_locale(locale).is_some();
    // The app language reaches the backend with the settings; this covers the first start.
    let ui = app.state::<crate::store::Store>().get().language;
    if apply_ui_language(app, &ui)? || changed {
        app.emit("game-locale-updated", ())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Item names for the UI follow the app `language`; true when that changed the name table.
pub fn apply_ui_language(app: &tauri::AppHandle, language: &str) -> Result<bool, String> {
    use tauri::Manager;
    let code = crate::services::game_locale::ui_code(language);
    let state = app.state::<ItemDbState>();
    if state.ui_code() == code {
        return Ok(false);
    }
    let locale = GameLocale::load(code, &BundledData::from_app(app)?);
    Ok(state.set_ui_locale(locale).is_some())
}

/// Whether Windows has the OCR pack for the game language; reward cards are read in it.
#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OcrLanguageStatus {
    pub game_language: String,
    pub installed: bool,
    /// Windows has an OCR pack for this language, so the install button can offer it.
    pub installable: bool,
}

#[tauri::command]
#[specta::specta]
pub fn ocr_language_status() -> OcrLanguageStatus {
    #[cfg(windows)]
    let (code, installed, installable) = {
        use crate::platform::windows::ocr;
        let (code, tag) = ocr::language_status();
        (code, tag.is_some(), ocr::capability_tag(code).is_some())
    };
    #[cfg(not(windows))]
    let (code, installed, installable) = ("en", true, false);
    OcrLanguageStatus {
        game_language: code.into(),
        installed,
        installable,
    }
}

/// Windows' answer to installing the game language's OCR pack. Defined on every platform so the
/// generated bindings are the same everywhere; off Windows only `Unsupported` is ever returned.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OcrInstallOutcome {
    Installed,
    /// The user said no at the UAC prompt.
    Cancelled,
    Failed,
    /// The command finished but the recognizer never showed up.
    Timeout,
    /// No Windows OCR pack exists for the game language.
    Unsupported,
}

/// Installs the game language's Windows OCR pack (one UAC prompt). Only runs on the user's click.
#[tauri::command]
#[specta::specta]
pub async fn ocr_language_install() -> Result<OcrInstallOutcome, String> {
    #[cfg(windows)]
    let outcome = tauri::async_runtime::spawn_blocking(crate::platform::windows::ocr::install_pack)
        .await
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let outcome = OcrInstallOutcome::Unsupported;
    Ok(outcome)
}

/// Builds from the bundle plus the cached overlay, then refreshes the overlay from DE and
/// rebuilds if it changed.
pub fn setup(app: &tauri::App) {
    use crate::services::public_export::{self, Source};
    use tauri::{Emitter, Manager};

    app.manage(ItemDbState::default());
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let bundled = match BundledData::from_app(&handle) {
            Ok(b) => Arc::new(b),
            Err(e) => return tracing::warn!("item db: no bundled data: {e}"),
        };
        match bundled.list_exports().len() {
            0 => tracing::warn!("item db: bundled data not found"),
            n => tracing::info!("item db: bundled data found ({n} exports)"),
        }
        let rebuild = |overlay: Option<Overlay>| {
            let (h, b) = (handle.clone(), bundled.clone());
            async move {
                let built = tauri::async_runtime::spawn_blocking(move || {
                    ItemDb::build(&Sources::load(&b, overlay))
                })
                .await;
                if let Ok(db) = built {
                    h.state::<ItemDbState>().replace(db);
                    let _ = h.emit("item-db-updated", ());
                }
            }
        };
        let db = handle.state::<crate::db::Db>();
        rebuild(public_export::load_cached(&db)).await;
        let Ok(client) = crate::services::net::client() else {
            return;
        };
        let fresh =
            public_export::refresh(&db, &client, &Source::default(), &|e| bundled.export(e)).await;
        if fresh.changed {
            rebuild(fresh.overlay).await;
        }
    });
}
