//! Weapon dispositions and riven stat pools, indexed from the bundled `ExportWeapons` /
//! `ExportUpgrades` tables (port of WFHelper's `rivenData`, minus the OCR text matching).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use super::vocab::{canonical_tag_name, stat_name_to_tag};
use crate::services::bundled_data::BundledData;

#[derive(Clone, Debug)]
pub struct UpgradeEntry {
    pub tag: String,
    pub can_be_buff: bool,
    pub can_be_curse: bool,
    pub base_value: f64,
    pub display_name: String,
    /// Omega prefix syllable ("crita"); empty when the stat adds nothing to the name.
    pub prefix: String,
    pub suffix: String,
}

struct WeaponInfo {
    unique_name: String,
    disposition: f64,
    product_category: String,
    holster_category: String,
    compat_tags: Vec<String>,
    shotgun_family: bool,
}

#[derive(Default)]
pub struct RivenData {
    weapons: HashMap<String, WeaponInfo>,
    /// Lowercase name -> display name, in export order (first insertion keeps its slot).
    names: Vec<(String, String)>,
    name_slot: HashMap<String, usize>,
    by_unique_name: HashMap<String, String>,
    mods: HashMap<String, Vec<UpgradeEntry>>,
    tag_names: HashMap<String, String>,
}

const RIFLE: &str = "/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare";
const PISTOL: &str = "/Lotus/Upgrades/Mods/Randomized/LotusPistolRandomModRare";
const MELEE: &str = "/Lotus/Upgrades/Mods/Randomized/PlayerMeleeWeaponRandomModRare";
const SHOTGUN: &str = "/Lotus/Upgrades/Mods/Randomized/LotusShotgunRandomModRare";
const ARCHGUN: &str = "/Lotus/Upgrades/Mods/Randomized/LotusArchgunRandomModRare";
const KITGUN: &str = "/Lotus/Upgrades/Mods/Randomized/LotusModularPistolRandomModRare";
const ZAW: &str = "/Lotus/Upgrades/Mods/Randomized/LotusModularMeleeRandomModRare";

const SENTINEL_WEAPON_TAG: &str = "SENTINEL_WEAPON";
const VARIANT_SUFFIXES: &[&str] = &[" Prime", " Wraith", " Vandal"];
/// WFM runs no syndicate riven market, so those prefixes fold into the base family.
const VARIANT_PREFIXES: &[&str] = &[
    "MK1-", "Kuva ", "Tenet ", "Prisma ", "Vaykor ", "Rakta ", "Secura ", "Sancti ", "Synoid ",
    "Telos ",
];

/// Whether `name` carries a variant affix (Prime, Vaykor, …).
pub fn has_variant_affix(name: &str) -> bool {
    let lc = name.to_lowercase();
    VARIANT_SUFFIXES
        .iter()
        .any(|s| lc.ends_with(&s.to_lowercase()))
        || VARIANT_PREFIXES
            .iter()
            .any(|p| lc.starts_with(&p.to_lowercase()))
}

static MODULAR_MELEE: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)ModularMelee").unwrap());
static MODULAR_PISTOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("(?i)SUModularSecondary|InfKitGun").unwrap());
static PISTOL_END: LazyLock<Regex> = LazyLock::new(|| Regex::new("(?i)Pistol$").unwrap());
static LOC_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|[^|]*\|[%s]?\s*").unwrap());
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("<[^>]+>").unwrap());
static PARENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\(.*?\)\s*").unwrap());

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

/// Duviri exports a Drifter twin of six Tenno melee weapons under the same name; the riven
/// belongs to the Tenno entry, so the twin must never win the name key.
fn name_rank(product_category: &str) -> u8 {
    u8::from(product_category != "DrifterMelee")
}

fn clean_loc_tag(raw: &str) -> String {
    let s = LOC_PREFIX.replace_all(raw, "");
    let s = TAGS.replace_all(s.trim(), "");
    PARENS.replace_all(s.trim(), "").trim().to_owned()
}

/// Mod class comes from the inherited archetype, not holsterCategory: Drakgoon holsters WIDE_RIFLE.
fn has_shotgun_ancestor(unique_name: &str, weapons: &Map<String, Value>) -> bool {
    let mut seen = HashSet::new();
    let mut current = Some(unique_name);
    while let Some(c) = current {
        if !seen.insert(c) {
            break;
        }
        if c.to_lowercase().contains("/shotgun/") {
            return true;
        }
        current = weapons
            .get(c)
            .and_then(|w| w.get("parentName"))
            .and_then(Value::as_str);
    }
    false
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_owned()
}

impl RivenData {
    /// Empty when the bundle has no weapon table (every lookup then answers "unknown").
    pub fn load(bundled: &BundledData) -> Self {
        let dict = bundled.dict("en").unwrap_or_default();
        let weapons = bundled.export("ExportWeapons").unwrap_or_default();
        let upgrades = bundled.export("ExportUpgrades").unwrap_or_default();
        Self::build(&dict, &weapons, &upgrades)
    }

    pub fn build(
        dict: &Map<String, Value>,
        weapons: &Map<String, Value>,
        upgrades: &Map<String, Value>,
    ) -> Self {
        let mut me = Self::default();
        let lookup = |k: &str| dict.get(k).and_then(Value::as_str).unwrap_or("");

        for (unique, w) in weapons {
            if unique.is_empty() || unique == "default" {
                continue;
            }
            let name = match w.get("name").and_then(Value::as_str) {
                Some(n) if n.starts_with('/') => lookup(n),
                Some(n) => n,
                None => continue,
            };
            let Some(disposition) = w.get("omegaAttenuation").and_then(Value::as_f64) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let lc = name.to_lowercase();
            let cat = |k: &str| w.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
            let product_category = cat("productCategory");
            // Ties keep the later entry, so a name collision resolves by insertion order.
            let wins = me
                .weapons
                .get(&lc)
                .is_none_or(|e| name_rank(&product_category) >= name_rank(&e.product_category));
            if wins {
                me.weapons.insert(
                    lc.clone(),
                    WeaponInfo {
                        unique_name: unique.clone(),
                        disposition,
                        holster_category: cat("holsterCategory"),
                        compat_tags: w
                            .get("compatibilityTags")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|t| t.as_str().map(str::to_owned))
                            .collect(),
                        shotgun_family: has_shotgun_ancestor(unique, weapons),
                        product_category,
                    },
                );
            }
            match me.name_slot.get(&lc) {
                Some(&i) => me.names[i].1 = name.to_owned(),
                None => {
                    me.name_slot.insert(lc.clone(), me.names.len());
                    me.names.push((lc, name.to_owned()));
                }
            }
            me.by_unique_name.insert(unique.clone(), name.to_owned());
        }

        for (key, m) in upgrades {
            let Some(entries) = m
                .get("upgradeEntries")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
            else {
                continue;
            };
            if !key.contains("Randomized") {
                continue;
            }
            let mut out = Vec::with_capacity(entries.len());
            for ue in entries {
                let tag = ue
                    .get("tag")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let val0 = ue.pointer("/upgradeValues/0");
                let base_value = val0
                    .and_then(|v| v.get("value"))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let mut display_name = val0
                    .and_then(|v| v.get("locTag"))
                    .and_then(Value::as_str)
                    .map(|l| clean_loc_tag(lookup(l)))
                    .unwrap_or_default();
                if display_name.is_empty() {
                    display_name = canonical_tag_name(&tag).unwrap_or_else(|| tag.clone());
                }
                let syllable = |k: &str| {
                    ue.get(k)
                        .and_then(Value::as_str)
                        .map(|t| lookup(t).to_owned())
                        .unwrap_or_default()
                };
                me.tag_names
                    .entry(tag.clone())
                    .or_insert_with(|| display_name.clone());
                out.push(UpgradeEntry {
                    can_be_buff: truthy(ue.get("canBeBuff")),
                    can_be_curse: truthy(ue.get("canBeCurse")),
                    prefix: syllable("prefixTag"),
                    suffix: syllable("suffixTag"),
                    tag,
                    base_value,
                    display_name,
                });
            }
            me.mods.insert(key.clone(), out);
        }
        me
    }

    pub fn is_empty(&self) -> bool {
        self.weapons.is_empty()
    }

    fn weapon(&self, name: &str) -> Option<&WeaponInfo> {
        self.weapons.get(&name.to_lowercase())
    }

    /// Resolves a riven fingerprint compat path to a weapon display name.
    pub fn weapon_name_by_unique_name(&self, unique_name: &str) -> Option<&str> {
        self.by_unique_name.get(unique_name).map(String::as_str)
    }

    pub fn weapon_disposition(&self, name: &str) -> Option<f64> {
        self.weapon(name).map(|w| w.disposition)
    }

    /// True when the card reads "Melee Damage" and "Attack Speed" for this weapon.
    pub fn is_melee_weapon(&self, name: &str) -> bool {
        let Some(w) = self.weapon(name) else {
            return false;
        };
        MODULAR_MELEE.is_match(&w.unique_name)
            || matches!(
                w.product_category.as_str(),
                "Melee" | "SpaceMelee" | "DrifterMelee"
            )
            || w.holster_category == "MELEE"
    }

    /// The riven mod uniqueName for a weapon; `None` when it cannot roll one.
    pub fn resolve_riven_type(&self, name: &str) -> Option<&'static str> {
        let w = self.weapon(name)?;
        let cat = w.product_category.as_str();
        if cat == "LongGuns"
            && (w.holster_category == "SHOTGUN"
                || w.compat_tags.iter().any(|t| t == "SHOTGUN")
                || w.shotgun_family)
        {
            return Some(SHOTGUN);
        }
        if MODULAR_MELEE.is_match(&w.unique_name) {
            return Some(ZAW);
        }
        if MODULAR_PISTOL.is_match(&w.unique_name) {
            return Some(KITGUN);
        }
        match cat {
            "LongGuns" => return Some(RIFLE),
            "Pistols" => return Some(PISTOL),
            "Melee" | "DrifterMelee" => return Some(MELEE),
            "SpaceGuns" => return Some(ARCHGUN),
            _ => {}
        }
        // Companion weapons roll the pool of the class they holster as. A leftover
        // omegaAttenuation proves nothing: exalted, hound and amp weapons carry one too.
        if !w.compat_tags.iter().any(|t| t == SENTINEL_WEAPON_TAG) {
            return None;
        }
        // Burst Laser and its variants carry no holsterCategory; the class is in the path.
        let holster = if w.holster_category.is_empty() {
            if PISTOL_END.is_match(&w.unique_name) {
                "PISTOL"
            } else {
                ""
            }
        } else {
            w.holster_category.as_str()
        };
        match holster {
            "RIFLE" | "SNIPER" | "BOW" => Some(RIFLE),
            "SHOTGUN" => Some(SHOTGUN),
            "PISTOL" => Some(PISTOL),
            "MELEE" => Some(MELEE),
            _ => None,
        }
    }

    fn entries(&self, riven_type: &str) -> &[UpgradeEntry] {
        self.mods.get(riven_type).map_or(&[], Vec::as_slice)
    }

    /// Exact tag, then the melee faction twin, then melee damage for plain damage.
    pub fn find_upgrade_entry(&self, riven_type: &str, tag: &str) -> Option<&UpgradeEntry> {
        let entries = self.entries(riven_type);
        let by_tag = |t: &str| entries.iter().find(|e| e.tag == t);
        by_tag(tag)
            .or_else(|| by_tag(&tag.replace("WeaponFaction", "WeaponMeleeFaction")))
            .or_else(|| {
                (tag == "WeaponDamageAmountMod")
                    .then(|| by_tag("WeaponMeleeDamageMod"))
                    .flatten()
            })
    }

    /// Uses "Attack Speed" for the fire-rate tag on melee cards.
    pub fn stat_display_name(&self, tag: &str, melee: bool) -> String {
        if melee && tag == "WeaponFireRateMod" {
            return "Attack Speed".into();
        }
        self.tag_names
            .get(tag)
            .cloned()
            .or_else(|| canonical_tag_name(tag))
            .unwrap_or_else(|| tag.to_owned())
    }

    pub fn stat_name_to_tag(&self, name: &str) -> Option<&'static str> {
        stat_name_to_tag(name)
    }

    /// Riven name from the buffs, sorted by descending roll then base value. Curses add nothing.
    pub fn generate_suffix(&self, riven_type: &str, buffs: &[(&str, f64)]) -> String {
        if self.entries(riven_type).is_empty() || buffs.is_empty() {
            return String::new();
        }
        let find = |tag: &str| self.find_upgrade_entry(riven_type, tag);
        let base = |tag: &str| find(tag).map_or(0.0, |e| e.base_value);
        let title = |s: &str| -> String {
            let mut c = s.chars();
            c.next()
                .map(|f| {
                    f.to_uppercase()
                        .chain(c.flat_map(char::to_lowercase))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut sorted = buffs.to_vec();
        sorted.sort_by(|a, b| {
            if a.1 == b.1 {
                base(a.0).total_cmp(&base(b.0))
            } else {
                b.1.total_cmp(&a.1)
            }
        });
        if sorted.len() == 1 {
            return match find(sorted[0].0) {
                Some(e) if !e.prefix.is_empty() => title(&e.prefix) + &e.suffix.to_lowercase(),
                _ => String::new(),
            };
        }
        let mut name = String::new();
        for (i, (tag, _)) in sorted.iter().enumerate() {
            let Some(e) = find(tag) else { continue };
            if i == sorted.len() - 1 {
                name += &e.suffix.to_lowercase();
            } else if name.is_empty() {
                if !e.prefix.is_empty() {
                    name += &title(&e.prefix);
                }
            } else if !e.prefix.is_empty() {
                name += &format!("-{}", e.prefix.to_lowercase());
            }
        }
        name
    }

    /// WFM riven family slug: "Boar Prime" -> "boar". An affix only marks a variant when the
    /// plain weapon exists (Gotva Prime has no base form, so it is its own family).
    pub fn family_slug(&self, weapon_name: &str) -> String {
        let base_if_known = |candidate: &str, fallback: &str| {
            if self.weapons.contains_key(&candidate.to_lowercase()) {
                candidate.to_owned()
            } else {
                fallback.to_owned()
            }
        };
        let mut name = weapon_name.trim().to_owned();
        for suffix in VARIANT_SUFFIXES {
            if name.to_lowercase().ends_with(&suffix.to_lowercase()) {
                let head = name.get(..name.len() - suffix.len()).unwrap_or(&name);
                name = base_if_known(head, &name);
                break;
            }
        }
        for prefix in VARIANT_PREFIXES {
            if name.to_lowercase().starts_with(&prefix.to_lowercase()) {
                let tail = name.get(prefix.len()..).unwrap_or(&name);
                name = base_if_known(tail, &name);
                break;
            }
        }
        // WFM spells the ampersand out: silva_and_aegis, not silva_aegis.
        slugify(&name.replace('&', " and "))
    }

    /// Every variant and disposition in a weapon's riven family.
    pub fn family_variants(&self, weapon_name: &str) -> Vec<(String, f64)> {
        let slug = self.family_slug(weapon_name);
        if slug.is_empty() {
            return vec![];
        }
        self.names
            .iter()
            .filter(|(_, display)| self.family_slug(display) == slug)
            .filter_map(|(lc, display)| Some((display.clone(), self.weapons.get(lc)?.disposition)))
            .collect()
    }

    /// Sorted display names of every weapon with a disposition.
    pub fn all_weapon_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.names.iter().map(|(_, d)| d.clone()).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }
}
