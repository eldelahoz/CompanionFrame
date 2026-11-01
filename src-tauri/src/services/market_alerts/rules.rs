//! Alert rules: the shapes, their validation and the pure auction/order matching (port of WFHelper's
//! `marketAlertTypes` + the matchers in `marketAlerts`). No I/O.
//!
//! `MarketAlertRule` is the shareable half an export carries; `MarketAlertBinding` is device-local.
//! Unknown JSON fields are rejected rather than dropped: a file we did not write deserves a hard error.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::services::rivens::endo::{dissolve_endo, endo_per_plat};
use crate::services::rivens::vocab::is_wfm_attribute;
use crate::services::wfm::orders::BookOrder;

pub const SCHEMA_VERSION: u32 = 1;
pub const IMPORT_MAX_BYTES: usize = 256 * 1024;
pub const MAX_RULES: usize = 100;
pub const MAX_NAME_CHARS: usize = 60;
pub const MAX_ATTRIBUTES: usize = 8;
pub const MAX_STAT_BOUNDS: usize = 8;
pub const HISTORY_MAX: usize = 500;
pub const MIN_COOLDOWN_MINUTES: u32 = 5;
pub const MAX_COOLDOWN_MINUTES: u32 = 24 * 60;
pub const DEFAULT_COOLDOWN_MINUTES: u32 = 60;
const RIVEN_MAX_MOD_RANK: f64 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum MarketAlertKind {
    Riven,
    Item,
    Baro,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum RivenPolarity {
    Madurai,
    Naramon,
    Vazarin,
}

impl RivenPolarity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Madurai => "madurai",
            Self::Naramon => "naramon",
            Self::Vazarin => "vazarin",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum MarketOrderSide {
    Sell,
    Buy,
}

impl MarketOrderSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sell => "sell",
            Self::Buy => "buy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum SellerStatus {
    Ingame,
    Online,
}

impl SellerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ingame => "ingame",
            Self::Online => "online",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RivenStatBound {
    /// WFM auction attribute url_name, matched exactly, never by substring.
    pub attribute: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RivenAlertMatch {
    /// WFM riven family slug, resolved from the catalog, never a display name.
    pub weapon_url_name: String,
    #[serde(default)]
    pub require_positive: Vec<String>,
    /// The only curses the roll may carry; a clean roll still passes, `has_negative` decides that.
    pub allowed_negatives: Option<Vec<String>>,
    pub exclude_negatives: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_attributes: Vec<String>,
    /// true = the roll must carry a curse, false = must not, absent = either.
    pub has_negative: Option<bool>,
    pub positive_count: Option<u32>,
    /// Share of `require_positive` the roll must carry, 0-100; absent means all.
    pub min_similarity_pct: Option<u32>,
    /// Auctions with no buyout price. Off by default so a rule never quotes an opening bid as the ask.
    pub include_bid_only: Option<bool>,
    /// Compared against the attribute value normalised to mod rank 8.
    #[serde(default)]
    pub stat_bounds: Vec<RivenStatBound>,
    pub min_mastery_rank: Option<u32>,
    pub max_mastery_rank: Option<u32>,
    pub polarity: Option<RivenPolarity>,
    pub min_mod_rank: Option<u32>,
    pub max_mod_rank: Option<u32>,
    pub min_platinum: Option<u32>,
    pub max_platinum: Option<u32>,
    pub min_rerolls: Option<u32>,
    pub max_rerolls: Option<u32>,
    pub min_endo_per_plat: Option<f64>,
    /// Only auctions whose seller is in one of these; empty means any status.
    #[serde(default)]
    pub statuses: Vec<SellerStatus>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemAlertMatch {
    pub item_url_name: String,
    pub side: MarketOrderSide,
    pub max_platinum: Option<u32>,
    pub min_platinum: Option<u32>,
    pub min_quantity: Option<u32>,
    /// Only orders whose owner is in one of these; empty means any status.
    #[serde(default)]
    pub statuses: Vec<SellerStatus>,
    pub owned_below: Option<u32>,
    pub owned_above: Option<u32>,
}

/// Reserved. The schema accepts and round-trips these rules; nothing evaluates them, so a baro rule
/// can never fire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BaroAlertMatch {
    pub item_url_name: String,
    pub max_ducats: Option<u32>,
    pub max_credits: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarketAlertRule {
    /// Empty on a new rule; the store assigns one.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub kind: MarketAlertKind,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_cooldown")]
    pub cooldown_minutes: u32,
    /// No quiet window at all; every new listing pings. `cooldown_minutes` is kept so switching
    /// back restores the window the user had.
    #[serde(default)]
    pub no_cooldown: bool,
    pub riven: Option<RivenAlertMatch>,
    pub item: Option<ItemAlertMatch>,
    pub baro: Option<BaroAlertMatch>,
}

fn yes() -> bool {
    true
}

fn default_cooldown() -> u32 {
    DEFAULT_COOLDOWN_MINUTES
}

/// Device-local. Never exported, never shared, never part of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketAlertBinding {
    pub native: bool,
}

impl Default for MarketAlertBinding {
    fn default() -> Self {
        Self { native: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketAlertHit {
    pub id: String,
    pub rule_id: String,
    /// Copied at fire time so a renamed or deleted rule keeps its history.
    pub rule_name: String,
    pub at: String,
    pub kind: MarketAlertKind,
    /// English on purpose: a stored translated string freezes its language.
    pub title: String,
    pub detail: String,
    pub url: String,
    pub platinum: Option<f64>,
    pub seller: Option<String>,
    pub seller_status: Option<String>,
    pub endo: Option<u32>,
    pub endo_per_plat: Option<f64>,
}

// --- validation -------------------------------------------------------------------------------

/// WFM item and weapon slugs. Attribute slugs are checked against the closed vocabulary instead
/// because those legitimately contain a slash.
fn is_slug(s: &str) -> bool {
    let mut bytes = s.bytes();
    s.len() <= 64
        && bytes
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn attribute_list(key: &str, list: &mut Vec<String>) -> Result<(), String> {
    if list.len() > MAX_ATTRIBUTES {
        return Err(format!("{key} has too many entries"));
    }
    let mut kept: Vec<String> = Vec::new();
    for a in list.drain(..) {
        if !is_wfm_attribute(&a) {
            return Err(format!("{key} has an unknown attribute"));
        }
        if !kept.contains(&a) {
            kept.push(a);
        }
    }
    *list = kept;
    Ok(())
}

fn within(key: &str, v: Option<u32>, min: u32, max: u32) -> Result<(), String> {
    match v {
        Some(v) if v < min || v > max => Err(format!("{key} out of range")),
        _ => Ok(()),
    }
}

fn ordered(
    low_key: &str,
    high_key: &str,
    low: Option<u32>,
    high: Option<u32>,
) -> Result<(), String> {
    match (low, high) {
        (Some(l), Some(h)) if l > h => Err(format!("{low_key} is above {high_key}")),
        _ => Ok(()),
    }
}

fn dedup_statuses(list: &mut Vec<SellerStatus>) -> Result<(), String> {
    if list.len() > 2 {
        return Err("statuses has too many entries".into());
    }
    let mut seen = Vec::new();
    list.retain(|s| {
        let new = !seen.contains(s);
        seen.push(*s);
        new
    });
    Ok(())
}

fn finite(key: &str, v: Option<f64>, min: f64, max: f64) -> Result<(), String> {
    match v {
        Some(v) if !v.is_finite() || v < min || v > max => Err(format!("{key} out of range")),
        _ => Ok(()),
    }
}

impl RivenAlertMatch {
    fn validate(&mut self) -> Result<(), String> {
        let r = |e: String| format!("riven {e}");
        if !is_slug(&self.weapon_url_name) {
            return Err("riven weaponUrlName is not a slug".into());
        }
        attribute_list("requirePositive", &mut self.require_positive).map_err(r)?;
        attribute_list("excludeAttributes", &mut self.exclude_attributes).map_err(r)?;
        for (key, list) in [
            ("allowedNegatives", &mut self.allowed_negatives),
            ("excludeNegatives", &mut self.exclude_negatives),
        ] {
            if let Some(list) = list {
                attribute_list(key, list).map_err(r)?;
            }
        }
        dedup_statuses(&mut self.statuses).map_err(r)?;

        if self.stat_bounds.len() > MAX_STAT_BOUNDS {
            return Err("riven statBounds has too many entries".into());
        }
        let mut bounds: Vec<RivenStatBound> = Vec::new();
        for b in self.stat_bounds.drain(..) {
            if !is_wfm_attribute(&b.attribute) {
                return Err("riven statBounds has an unknown attribute".into());
            }
            finite("min", b.min, -100_000.0, 100_000.0)
                .map_err(|e| r(format!("statBounds {e}")))?;
            finite("max", b.max, -100_000.0, 100_000.0)
                .map_err(|e| r(format!("statBounds {e}")))?;
            match (b.min, b.max) {
                (None, None) => return Err("riven statBounds entry needs a min or a max".into()),
                (Some(lo), Some(hi)) if lo > hi => {
                    return Err("riven statBounds min is above max".into());
                }
                _ => {}
            }
            // Only the first bound per attribute counts.
            if !bounds.iter().any(|e| e.attribute == b.attribute) {
                bounds.push(b);
            }
        }
        self.stat_bounds = bounds;

        for (key, v, min, max) in [
            ("positiveCount", self.positive_count, 2, 3),
            ("minSimilarityPct", self.min_similarity_pct, 0, 100),
            ("minMasteryRank", self.min_mastery_rank, 0, 16),
            ("maxMasteryRank", self.max_mastery_rank, 0, 16),
            ("minModRank", self.min_mod_rank, 0, 8),
            ("maxModRank", self.max_mod_rank, 0, 8),
            ("minPlatinum", self.min_platinum, 0, 1_000_000),
            ("maxPlatinum", self.max_platinum, 0, 1_000_000),
            ("minRerolls", self.min_rerolls, 0, 10_000),
            ("maxRerolls", self.max_rerolls, 0, 10_000),
        ] {
            within(key, v, min, max).map_err(r)?;
        }
        finite("minEndoPerPlat", self.min_endo_per_plat, 0.0, 100_000.0).map_err(r)?;
        for (lk, hk, l, h) in [
            (
                "minMasteryRank",
                "maxMasteryRank",
                self.min_mastery_rank,
                self.max_mastery_rank,
            ),
            (
                "minModRank",
                "maxModRank",
                self.min_mod_rank,
                self.max_mod_rank,
            ),
            (
                "minPlatinum",
                "maxPlatinum",
                self.min_platinum,
                self.max_platinum,
            ),
            (
                "minRerolls",
                "maxRerolls",
                self.min_rerolls,
                self.max_rerolls,
            ),
        ] {
            ordered(lk, hk, l, h).map_err(r)?;
        }
        if self
            .exclude_attributes
            .iter()
            .any(|a| self.require_positive.contains(a))
        {
            return Err("riven excludeAttributes contradicts a required attribute".into());
        }
        Ok(())
    }
}

impl ItemAlertMatch {
    fn validate(&mut self) -> Result<(), String> {
        let r = |e: String| format!("item {e}");
        if !is_slug(&self.item_url_name) {
            return Err("item itemUrlName is not a slug".into());
        }
        dedup_statuses(&mut self.statuses).map_err(r)?;
        for (key, v, min) in [
            ("maxPlatinum", self.max_platinum, 0),
            ("minPlatinum", self.min_platinum, 0),
            ("minQuantity", self.min_quantity, 1),
            ("ownedBelow", self.owned_below, 0),
            ("ownedAbove", self.owned_above, 0),
        ] {
            within(key, v, min, 1_000_000).map_err(r)?;
        }
        if self.max_platinum.is_none() && self.min_platinum.is_none() {
            return Err("item needs a price threshold".into());
        }
        ordered(
            "minPlatinum",
            "maxPlatinum",
            self.min_platinum,
            self.max_platinum,
        )
        .map_err(r)
    }
}

impl MarketAlertRule {
    /// Normalises in place (trim, dedupe) and rejects anything out of bounds. The id is not checked
    /// beyond length; the store assigns one when empty.
    pub fn validate(&mut self) -> Result<(), String> {
        if self.id.len() > 64 {
            return Err("rule id is invalid".into());
        }
        self.name = self.name.trim().to_owned();
        if self.name.is_empty() || self.name.chars().count() > MAX_NAME_CHARS {
            return Err("rule name length is invalid".into());
        }
        if !(MIN_COOLDOWN_MINUTES..=MAX_COOLDOWN_MINUTES).contains(&self.cooldown_minutes) {
            return Err("rule cooldownMinutes out of range".into());
        }
        // Exactly the section its kind names, so a rule cannot carry a hidden second criteria block
        // that a later version would start evaluating.
        let present = [
            (MarketAlertKind::Riven, self.riven.is_some()),
            (MarketAlertKind::Item, self.item.is_some()),
            (MarketAlertKind::Baro, self.baro.is_some()),
        ];
        for (kind, has) in present {
            if has && kind != self.kind {
                return Err(
                    format!("rule kind {:?} cannot carry a {kind:?} section", self.kind)
                        .to_lowercase(),
                );
            }
        }
        match self.kind {
            MarketAlertKind::Riven => self
                .riven
                .as_mut()
                .ok_or("riven must be an object")?
                .validate(),
            MarketAlertKind::Item => self
                .item
                .as_mut()
                .ok_or("item must be an object")?
                .validate(),
            MarketAlertKind::Baro => {
                let b = self.baro.as_ref().ok_or("baro must be an object")?;
                if !is_slug(&b.item_url_name) {
                    return Err("baro itemUrlName is not a slug".into());
                }
                within("maxDucats", b.max_ducats, 0, 1_000_000).map_err(|e| format!("baro {e}"))?;
                within("maxCredits", b.max_credits, 0, 1_000_000_000)
                    .map_err(|e| format!("baro {e}"))
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportFile {
    schema: u32,
    #[allow(dead_code)]
    #[serde(rename = "exportedAt")]
    exported_at: Option<String>,
    rules: Vec<Value>,
}

/// Criteria only. Bindings, owned counts, hit history and anything naming this machine are left
/// behind by construction.
pub fn build_export(rules: &[MarketAlertRule]) -> String {
    serde_json::json!({
        "schema": SCHEMA_VERSION,
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "rules": rules,
    })
    .to_string()
}

/// `text` is the raw file so the byte cap applies before the JSON parser sees it. Imported ids are
/// dropped: a shared file must never collide with or overwrite a rule that already exists here.
pub fn parse_import(text: &str) -> Result<Vec<MarketAlertRule>, String> {
    if text.len() > IMPORT_MAX_BYTES {
        return Err("import is too large".into());
    }
    let file: ExportFile =
        serde_json::from_str(text).map_err(|e| format!("import is not valid: {e}"))?;
    if file.schema != SCHEMA_VERSION {
        return Err("import schema version mismatch".into());
    }
    if file.rules.len() > MAX_RULES {
        return Err("import has too many rules".into());
    }
    file.rules
        .into_iter()
        .enumerate()
        .map(|(i, mut raw)| {
            let n = i + 1;
            if let Some(o) = raw.as_object_mut() {
                o.remove("id");
            }
            let mut rule: MarketAlertRule =
                serde_json::from_value(raw).map_err(|e| format!("import rule {n}: {e}"))?;
            rule.validate()
                .map_err(|e| format!("import rule {n}: {e}"))?;
            Ok(rule)
        })
        .collect()
}

// --- matching ---------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct AuctionAttr {
    pub url_name: String,
    pub value: f64,
    pub positive: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Auction {
    pub id: String,
    pub seller: String,
    pub seller_status: String,
    pub platinum: f64,
    pub mastery_level: f64,
    pub mod_rank: f64,
    pub rerolls: f64,
    pub polarity: String,
    /// No buyout price: `platinum` is the opening bid, not an asking price.
    pub bid_only: bool,
    pub attributes: Vec<AuctionAttr>,
}

fn num(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .unwrap_or(0.0)
}

fn lower(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_lowercase()
}

/// Reads a v1 `/auctions/search` body (`payload.auctions`).
pub fn parse_auctions(raw: &Value) -> Vec<Auction> {
    let Some(list) = raw.pointer("/payload/auctions").and_then(Value::as_array) else {
        return vec![];
    };
    list.iter()
        .filter_map(|a| {
            let id = a.get("id")?.as_str()?.to_owned();
            let item = a.get("item");
            let owner = a.get("owner");
            let attributes = item
                .and_then(|i| i.get("attributes"))
                .and_then(Value::as_array)
                .map(|attrs| {
                    attrs
                        .iter()
                        .filter_map(|x| {
                            Some(AuctionAttr {
                                url_name: x.get("url_name")?.as_str()?.to_owned(),
                                value: num(x.get("value")),
                                positive: x.get("positive").and_then(Value::as_bool) != Some(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let buyout = a
                .get("buyout_price")
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite());
            Some(Auction {
                id,
                seller: owner
                    .and_then(|o| o.get("ingame_name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                seller_status: lower(owner.and_then(|o| o.get("status"))),
                platinum: buyout.unwrap_or_else(|| num(a.get("starting_price"))),
                mastery_level: num(item.and_then(|i| i.get("mastery_level"))),
                mod_rank: num(item.and_then(|i| i.get("mod_rank"))),
                rerolls: num(item.and_then(|i| i.get("re_rolls"))),
                polarity: lower(item.and_then(|i| i.get("polarity"))),
                bid_only: buyout.is_none(),
                attributes,
            })
        })
        .collect()
}

/// WFM's stat keys AND server-side and ignore `similarity`, so only an all-required rule may push
/// positives, and only a one-entry curse list is safe.
pub fn riven_search_path(m: &RivenAlertMatch) -> String {
    let mut path = format!(
        "/auctions/search?type=riven&weapon_url_name={}",
        enc(&m.weapon_url_name)
    );
    let push_positive = m.min_similarity_pct.unwrap_or(100) >= 100;
    let negatives = match (&m.allowed_negatives, m.has_negative) {
        (Some(a), Some(true)) if a.len() == 1 => a.as_slice(),
        _ => &[],
    };
    // A comma list in one key is the AND the picked stats mean; WFM honours only the first repeat.
    if push_positive && !m.require_positive.is_empty() {
        path += &format!("&positive_stats={}", enc(&m.require_positive.join(",")));
    }
    if !negatives.is_empty() {
        path += &format!("&negative_stats={}", enc(&negatives.join(",")));
    }
    if let Some(p) = m.polarity {
        path += &format!("&polarity={}", p.as_str());
    }
    for (key, v) in [
        ("mastery_rank_min", m.min_mastery_rank),
        ("mastery_rank_max", m.max_mastery_rank),
        ("re_rolls_min", m.min_rerolls),
        ("re_rolls_max", m.max_rerolls),
    ] {
        if let Some(v) = v {
            path += &format!("&{key}={v}");
        }
    }
    path + "&sort_by=price_asc"
}

fn status_ok(statuses: &[SellerStatus], status: &str) -> bool {
    statuses.is_empty() || statuses.iter().any(|s| s.as_str() == status)
}

fn in_bounds(v: f64, min: Option<f64>, max: Option<f64>) -> bool {
    min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m)
}

fn bounds(v: f64, min: Option<u32>, max: Option<u32>) -> bool {
    in_bounds(v, min.map(f64::from), max.map(f64::from))
}

/// WFM serves an attribute at the listing's own rank, and riven stats scale with (rank + 1).
fn value_at_max_rank(value: f64, mod_rank: f64) -> f64 {
    let rank = mod_rank.trunc().clamp(0.0, RIVEN_MAX_MOD_RANK);
    ((value * (RIVEN_MAX_MOD_RANK + 1.0) / (rank + 1.0)) * 10.0).round() / 10.0
}

/// Exact url_name equality only: critical_chance must never claim the slide slug.
pub fn match_riven(m: &RivenAlertMatch, a: &Auction) -> bool {
    if a.bid_only && m.include_bid_only != Some(true) {
        return false;
    }
    if !status_ok(&m.statuses, &a.seller_status) {
        return false;
    }
    let positives: HashSet<&str> = a
        .attributes
        .iter()
        .filter(|x| x.positive)
        .map(|x| x.url_name.as_str())
        .collect();
    let negatives: HashSet<&str> = a
        .attributes
        .iter()
        .filter(|x| !x.positive)
        .map(|x| x.url_name.as_str())
        .collect();

    let required = m.require_positive.len();
    let required_pct = if required > 0 {
        m.require_positive
            .iter()
            .filter(|s| positives.contains(s.as_str()))
            .count() as f64
            / required as f64
            * 100.0
    } else {
        100.0
    };
    if required_pct < f64::from(m.min_similarity_pct.unwrap_or(100)) {
        return false;
    }
    if m.exclude_attributes
        .iter()
        .any(|s| positives.contains(s.as_str()) || negatives.contains(s.as_str()))
    {
        return false;
    }
    if let Some(allowed) = m.allowed_negatives.as_ref().filter(|l| !l.is_empty())
        && negatives.iter().any(|n| !allowed.iter().any(|s| s == n))
    {
        return false;
    }
    if m.exclude_negatives
        .iter()
        .flatten()
        .any(|s| negatives.contains(s.as_str()))
    {
        return false;
    }
    if m.has_negative == Some(true) && negatives.is_empty() {
        return false;
    }
    if m.has_negative == Some(false) && !negatives.is_empty() {
        return false;
    }
    if m.positive_count
        .is_some_and(|c| positives.len() != c as usize)
    {
        return false;
    }
    for b in &m.stat_bounds {
        let Some(attr) = a.attributes.iter().find(|x| x.url_name == b.attribute) else {
            return false;
        };
        if !in_bounds(value_at_max_rank(attr.value, a.mod_rank), b.min, b.max) {
            return false;
        }
    }
    if !bounds(a.mastery_level, m.min_mastery_rank, m.max_mastery_rank)
        || !bounds(a.mod_rank, m.min_mod_rank, m.max_mod_rank)
        || !bounds(a.platinum, m.min_platinum, m.max_platinum)
        || !bounds(a.rerolls, m.min_rerolls, m.max_rerolls)
    {
        return false;
    }
    if m.polarity.is_some_and(|p| p.as_str() != a.polarity) {
        return false;
    }
    if let Some(min) = m.min_endo_per_plat {
        match endo_per_plat(a.mastery_level, a.mod_rank, a.rerolls, a.platinum) {
            Some(ratio) if ratio >= min => {}
            _ => return false,
        }
    }
    true
}

/// A seller listing's view of a v2 order: platform filtering is the caller's.
pub fn match_item_order(m: &ItemAlertMatch, o: &BookOrder, owned: Option<u32>) -> bool {
    if o.order_type != m.side.as_str() {
        return false;
    }
    let status = o
        .user
        .as_ref()
        .and_then(|u| u.status.as_deref())
        .unwrap_or("")
        .to_lowercase();
    if !status_ok(&m.statuses, &status) {
        return false;
    }
    if !bounds(f64::from(o.platinum), m.min_platinum, m.max_platinum) {
        return false;
    }
    if m.min_quantity.is_some_and(|q| o.quantity < q) {
        return false;
    }
    if let Some(owned) = owned
        && (m.owned_below.is_some_and(|b| owned >= b) || m.owned_above.is_some_and(|a| owned <= a))
    {
        return false;
    }
    true
}

fn title_case(slug: &str) -> String {
    slug.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn stat_text(a: &Auction) -> String {
    a.attributes
        .iter()
        .map(|x| {
            // WFM ships a negative as a negative value or as a magnitude; the flag decides.
            format!(
                "{}{}% {}",
                if x.positive { "+" } else { "-" },
                x.value.abs(),
                title_case(&x.url_name.replace(['/', ' '], "_")).replace('_', " ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn new_hit(
    rule: &MarketAlertRule,
    kind: MarketAlertKind,
    title: String,
    detail: String,
    url: String,
) -> MarketAlertHit {
    MarketAlertHit {
        id: uuid::Uuid::new_v4().simple().to_string(),
        rule_id: rule.id.clone(),
        rule_name: rule.name.clone(),
        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        kind,
        title,
        detail,
        url,
        platinum: None,
        seller: None,
        seller_status: None,
        endo: None,
        endo_per_plat: None,
    }
}

pub fn riven_hit(rule: &MarketAlertRule, a: &Auction) -> MarketAlertHit {
    let stats = stat_text(a);
    let endo = dissolve_endo(a.mastery_level, a.mod_rank, a.rerolls);
    let ratio = endo_per_plat(a.mastery_level, a.mod_rank, a.rerolls, a.platinum);
    let detail = format!(
        "{}{}p{} - MR{} r{} {} rerolls - {} endo{}",
        if stats.is_empty() {
            String::new()
        } else {
            format!("{stats} - ")
        },
        a.platinum,
        if a.bid_only { " starting bid" } else { "" },
        a.mastery_level,
        a.mod_rank,
        a.rerolls,
        endo,
        ratio.map_or(String::new(), |r| format!(" ({r:.1}/plat)")),
    );
    let mut hit = new_hit(
        rule,
        MarketAlertKind::Riven,
        format!("Riven: {}", rule.name),
        detail,
        format!("https://warframe.market/auction/{}", a.id),
    );
    hit.platinum = Some(a.platinum);
    hit.endo = Some(endo as u32);
    hit.endo_per_plat = ratio.map(|r| (r * 10.0).round() / 10.0);
    hit.seller = Some(a.seller.clone()).filter(|s| !s.is_empty());
    hit.seller_status = Some(a.seller_status.clone()).filter(|s| !s.is_empty());
    hit
}

pub fn item_hit(
    rule: &MarketAlertRule,
    m: &ItemAlertMatch,
    o: &BookOrder,
    item_name: &str,
) -> MarketAlertHit {
    let verb = if o.order_type == "sell" { "WTS" } else { "WTB" };
    let mut hit = new_hit(
        rule,
        MarketAlertKind::Item,
        format!("Item: {}", rule.name),
        format!("{verb} {item_name} {}p x{}", o.platinum, o.quantity),
        format!("https://warframe.market/items/{}", m.item_url_name),
    );
    hit.platinum = Some(f64::from(o.platinum));
    if let Some(u) = &o.user {
        hit.seller = u.ingame_name.clone().filter(|s| !s.is_empty());
        hit.seller_status = u
            .status
            .clone()
            .map(|s| s.to_lowercase())
            .filter(|s| !s.is_empty());
    }
    hit
}

pub fn item_display_name(slug: &str) -> String {
    title_case(slug)
}

/// Percent-encodes everything but RFC 3986 unreserved bytes (`,` and `/` in a stat list included).
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
