//! 44bananas' per-weapon good-roll sheet: CSV parsing, weapon lookup and the cached fetch
//! (port of WFHelper's `rivenGoodRolls` + `rivenBestAttributes`).

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::data::RivenData;
use super::vocab::{stat_tag_display_name, tag_to_wfm_url_name};
use crate::db::Db;
use crate::services::cache::{kv_get, kv_put};
use crate::services::net::{self, Fetched};

pub const SHEET_URL: &str = "https://docs.google.com/spreadsheets/d";
const SHEET_ID: &str = "1LJ83e4x_xIVgjZg049PKrDgTex2MIuaiX0FFZi9mH-Q";
const KV_KEY: &str = "riven:good-rolls";
const MAX_CSV_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Affixes the sheet folds into the base row that the variant prefixes leave alone:
/// warframe.market keys those variants as riven families of their own.
const SHEET_ONLY_PREFIXES: &[&str] = &["coda ", "dex ", "mara ", "carmine ", "ceti ", "prime "];

static OR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\s+or\s+").unwrap());

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SheetClass {
    Primary,
    Secondary,
    Melee,
    Archgun,
    Robotic,
}

const TABS: [(u64, SheetClass); 5] = [
    (0, SheetClass::Primary),
    (1505239276, SheetClass::Secondary),
    (1413904270, SheetClass::Melee),
    (289737427, SheetClass::Archgun),
    (965095749, SheetClass::Robotic),
];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodRoll {
    pub mandatory: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoodRollData {
    pub good_attrs: Vec<GoodRoll>,
    pub accepted_bad_attrs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenGoodRollAttribute {
    pub tag: String,
    /// `None` when warframe.market has no auction attribute for this tag.
    pub wfm_url_name: Option<String>,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenGoodRollGroup {
    pub mandatory: Vec<RivenGoodRollAttribute>,
    pub optional: Vec<RivenGoodRollAttribute>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenGoodRollDetail {
    pub groups: Vec<RivenGoodRollGroup>,
    pub accepted_negatives: Vec<RivenGoodRollAttribute>,
    /// ISO time the sheet was last fetched; `None` when never cached.
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenBestAttributes {
    pub positives: Vec<String>,
    pub negatives: Vec<String>,
}

#[derive(Debug)]
pub struct SheetEntry {
    pub name: String,
    pub data: GoodRollData,
}

// ---- CSV ----

fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = vec![vec![]];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
        } else {
            match c {
                '"' => quoted = true,
                ',' => rows.last_mut().unwrap().push(std::mem::take(&mut field)),
                '\n' => {
                    rows.last_mut().unwrap().push(std::mem::take(&mut field));
                    rows.push(vec![]);
                }
                '\r' => {}
                _ => field.push(c),
            }
        }
    }
    if !field.is_empty() || !rows.last().unwrap().is_empty() {
        rows.last_mut().unwrap().push(field);
    }
    rows.retain(|r| r.len() > 1 || (r.len() == 1 && !r[0].is_empty()));
    rows
}

fn tags_for(abbrev: &str, class: SheetClass) -> Vec<&'static str> {
    let key = abbrev.trim().to_uppercase();
    let one = |t| vec![t];
    match key.as_str() {
        "" => vec![],
        "DMG" if class == SheetClass::Melee => one("WeaponMeleeDamageMod"),
        "DMG" => one("WeaponDamageAmountMod"),
        "ELEMENT" => vec![
            "WeaponFireDamageMod",
            "WeaponFreezeDamageMod",
            "WeaponElectricityDamageMod",
            "WeaponToxinDamageMod",
        ],
        "CC" => one("WeaponCritChanceMod"),
        "CD" => one("WeaponCritDamageMod"),
        "MS" => one("WeaponFireIterationsMod"),
        "FR" | "AS" => one("WeaponFireRateMod"),
        "RLS" => one("WeaponReloadSpeedMod"),
        "SC" => one("WeaponStunChanceMod"),
        "SD" => one("WeaponProcTimeMod"),
        "PT" => one("WeaponPunctureDepthMod"),
        "MAG" => one("WeaponClipMaxMod"),
        "AMMO" => one("WeaponAmmoMaxMod"),
        "REC" => one("WeaponRecoilReductionMod"),
        "ZOOM" => one("WeaponZoomFovMod"),
        "PFS" => one("WeaponProjectileSpeedMod"),
        "IMP" => one("WeaponImpactDamageMod"),
        "PUNC" => one("WeaponArmorPiercingDamageMod"),
        "SLASH" => one("WeaponSlashDamageMod"),
        "COLD" => one("WeaponFreezeDamageMod"),
        "HEAT" => one("WeaponFireDamageMod"),
        "ELEC" => one("WeaponElectricityDamageMod"),
        "TOX" => one("WeaponToxinDamageMod"),
        "DTG" => one("WeaponFactionDamageGrineer"),
        "DTC" => one("WeaponFactionDamageCorpus"),
        "DTI" => one("WeaponFactionDamageInfested"),
        "RANGE" => one("WeaponMeleeRangeIncMod"),
        "IC" => one("WeaponMeleeComboInitialBonusMod"),
        "EFF" => one("WeaponMeleeComboEfficiencyMod"),
        "SLIDE" => one("SlideAttackCritChanceMod"),
        "FIN" => one("WeaponMeleeFinisherDamageMod"),
        _ => vec![],
    }
}

fn push_unique(v: &mut Vec<String>, tag: &str) {
    if !v.iter().any(|t| t == tag) {
        v.push(tag.to_owned());
    }
}

fn parse_positives(cell: &str, class: SheetClass) -> Vec<GoodRoll> {
    let normalized = cell.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut rolls = vec![];
    for alt in OR.split(&normalized) {
        let (mut mandatory, mut optional) = (vec![], vec![]);
        for token in alt.split_whitespace() {
            let target = if token.contains('/') {
                &mut optional
            } else {
                &mut mandatory
            };
            for part in token.split('/') {
                for tag in tags_for(part, class) {
                    push_unique(target, tag);
                }
            }
        }
        optional.retain(|t| !mandatory.contains(t));
        if !mandatory.is_empty() || !optional.is_empty() {
            rolls.push(GoodRoll {
                mandatory,
                optional,
            });
        }
    }
    rolls
}

fn parse_negatives(cell: &str, class: SheetClass) -> Vec<String> {
    let mut out = vec![];
    for token in cell.split(|c: char| c == '/' || c.is_whitespace()) {
        for tag in tags_for(token, class) {
            push_unique(&mut out, tag);
        }
    }
    out
}

pub fn parse_good_roll_csv(csv: &str, class: SheetClass) -> Result<Vec<SheetEntry>, String> {
    let rows = parse_csv(csv.trim_start_matches('\u{feff}'));
    let header: Vec<String> = rows
        .first()
        .map(|r| r.iter().map(|h| h.trim().to_lowercase()).collect())
        .unwrap_or_default();
    let col = |prefix: &str| header.iter().position(|h| h.starts_with(prefix));
    let (Some(weapon), Some(positive), Some(negative)) =
        (col("weapon"), col("positive"), col("negative"))
    else {
        return Err(format!(
            "bad riven good-rolls header for {class:?}: {}",
            rows.first().map_or("<empty>".into(), |r| r.join(", "))
        ));
    };
    let cell = |r: &Vec<String>, i: usize| r.get(i).cloned().unwrap_or_default();
    Ok(rows
        .iter()
        .skip(1)
        .filter_map(|r| {
            let name = cell(r, weapon).trim().to_lowercase();
            if name.is_empty() {
                return None;
            }
            let good_attrs = parse_positives(&cell(r, positive), class);
            let accepted_bad_attrs = parse_negatives(&cell(r, negative), class);
            (!good_attrs.is_empty() || !accepted_bad_attrs.is_empty()).then_some(SheetEntry {
                name,
                data: GoodRollData {
                    good_attrs,
                    accepted_bad_attrs,
                },
            })
        })
        .collect())
}

// ---- store ----

#[derive(Serialize, Deserialize)]
struct Stored {
    updated_at: String,
    entries: Vec<(String, GoodRollData)>,
}

/// The parsed sheet, immutable once built: replace the whole value to refresh it.
#[derive(Default)]
pub struct GoodRolls {
    entries: Vec<(String, GoodRollData)>,
    by_name: HashMap<String, usize>,
    /// Family slug -> first row with that slug.
    by_family: HashMap<String, usize>,
    updated_at: Option<String>,
}

fn normalize_for_search(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Same fold as the slug used for WFM urls: apostrophes dropped, other punctuation to `_`.
fn sheet_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if matches!(c, '\'' | '\u{2019}' | '\u{2018}') {
            continue;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

impl GoodRolls {
    pub fn new(entries: Vec<(String, GoodRollData)>, updated_at: Option<String>) -> Self {
        let mut by_name = HashMap::new();
        let mut by_family = HashMap::new();
        for (i, (name, _)) in entries.iter().enumerate() {
            by_name.entry(name.clone()).or_insert(i);
            let slug = sheet_slug(&name.replace('&', " and "));
            if !slug.is_empty() {
                by_family.entry(slug).or_insert(i);
            }
        }
        Self {
            entries,
            by_name,
            by_family,
            updated_at,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }

    fn is_stale(&self) -> bool {
        let Some(at) = &self.updated_at else {
            return false;
        };
        DateTime::parse_from_rfc3339(at).map_or(true, |t| {
            Utc::now()
                .signed_duration_since(t)
                .to_std()
                .unwrap_or_default()
                > MAX_AGE
        })
    }

    /// Loaded and young enough that nothing needs fetching.
    pub fn is_current(&self) -> bool {
        !self.is_empty() && !self.is_stale()
    }

    /// The sheet rates one row per weapon family, so a variant only matches under the family
    /// name `RivenData` resolves for it.
    fn lookup(&self, data: &RivenData, weapon_name: &str) -> Option<&GoodRollData> {
        if weapon_name.is_empty() || self.is_empty() {
            return None;
        }
        let lc = normalize_for_search(weapon_name);
        let get = |k: &str| self.by_name.get(k).map(|&i| &self.entries[i].1);
        if let Some(d) = get(&lc) {
            return Some(d);
        }
        let folded = SHEET_ONLY_PREFIXES
            .iter()
            .find_map(|p| lc.strip_prefix(p))
            .map_or(lc.as_str(), str::trim);
        if folded.is_empty() {
            return None;
        }
        if folded != lc
            && let Some(d) = get(folded)
        {
            return Some(d);
        }
        let slug = data.family_slug(folded);
        if slug.is_empty() {
            return None;
        }
        self.by_family.get(&slug).map(|&i| &self.entries[i].1)
    }

    /// Exact sheet row by its lowercase name.
    #[cfg(test)]
    pub fn get_by_name(&self, name: &str) -> Option<&GoodRollData> {
        self.by_name.get(name).map(|&i| &self.entries[i].1)
    }

    pub fn get(&self, data: &RivenData, weapon_name: &str) -> Option<&GoodRollData> {
        self.lookup(data, weapon_name)
    }

    pub fn detail(
        &self,
        data: &RivenData,
        weapon_name: &str,
        melee: bool,
    ) -> Option<RivenGoodRollDetail> {
        let found = self.lookup(data, weapon_name)?;
        let attr = |tag: &String| RivenGoodRollAttribute {
            tag: tag.clone(),
            wfm_url_name: tag_to_wfm_url_name(tag).map(str::to_owned),
            display_name: stat_tag_display_name(tag, melee),
        };
        Some(RivenGoodRollDetail {
            groups: found
                .good_attrs
                .iter()
                .map(|g| RivenGoodRollGroup {
                    mandatory: g.mandatory.iter().map(attr).collect(),
                    optional: g.optional.iter().map(attr).collect(),
                })
                .collect(),
            accepted_negatives: found.accepted_bad_attrs.iter().map(attr).collect(),
            updated_at: self.updated_at.clone(),
        })
    }

    /// Mandatory stats first, then optional ones, each once; plus the tolerated curses.
    pub fn best_attributes(
        &self,
        data: &RivenData,
        weapon_name: &str,
        melee: bool,
    ) -> Option<RivenBestAttributes> {
        let found = self.lookup(data, weapon_name)?;
        let mut seen: Vec<&String> = vec![];
        let mut positives = vec![];
        for tag in found
            .good_attrs
            .iter()
            .flat_map(|g| &g.mandatory)
            .chain(found.good_attrs.iter().flat_map(|g| &g.optional))
        {
            if !seen.contains(&tag) {
                seen.push(tag);
                positives.push(stat_tag_display_name(tag, melee));
            }
        }
        Some(RivenBestAttributes {
            positives,
            negatives: found
                .accepted_bad_attrs
                .iter()
                .map(|t| stat_tag_display_name(t, melee))
                .collect(),
        })
    }

    pub fn load_cached(db: &Db) -> Option<Self> {
        let stored: Stored = serde_json::from_str(&kv_get(db, KV_KEY).ok()??).ok()?;
        let me = Self::new(stored.entries, Some(stored.updated_at));
        (!me.is_empty()).then_some(me)
    }

    fn save(&self, db: &Db) {
        let stored = Stored {
            updated_at: self.updated_at.clone().unwrap_or_default(),
            entries: self.entries.clone(),
        };
        if let Ok(json) = serde_json::to_string(&stored) {
            let _ = kv_put(db, KV_KEY, &json);
        }
    }
}

/// Downloads the five sheet tabs from `{base}/{sheet}/export?...`; the first row of a name wins.
/// A failed tab fails the whole fetch, so a half-loaded sheet never replaces a good one.
pub async fn fetch_sheet(client: &Client, base: &str) -> Result<GoodRolls, String> {
    let mut entries: Vec<(String, GoodRollData)> = vec![];
    for (gid, class) in TABS {
        let url = format!("{base}/{SHEET_ID}/export?format=csv&gid={gid}");
        let Fetched::Modified { body, .. } = net::fetch(client, &url, None, MAX_CSV_BYTES).await?
        else {
            return Err(format!("{url}: unexpected 304"));
        };
        for e in parse_good_roll_csv(&String::from_utf8_lossy(&body), class)? {
            if !entries.iter().any(|(n, _)| *n == e.name) {
                entries.push((e.name, e.data));
            }
        }
    }
    Ok(GoodRolls::new(
        entries,
        Some(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    ))
}

/// Fetches and persists; on failure answers the cached sheet (possibly stale) if there is one.
pub async fn refresh(db: &Db, client: &Client, base: &str) -> Result<GoodRolls, String> {
    match fetch_sheet(client, base).await {
        Ok(fresh) if !fresh.is_empty() => {
            fresh.save(db);
            Ok(fresh)
        }
        res => GoodRolls::load_cached(db)
            .ok_or_else(|| res.err().unwrap_or_else(|| "empty good-rolls sheet".into())),
    }
}
