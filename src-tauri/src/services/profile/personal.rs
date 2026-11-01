//! Personal stats from the public profile (career, per-weapon/enemy tables, equipped appearance)
//! and the saved loadouts in the local inventory. Port of `personalProfileParser.ts` and
//! `personalLoadouts.ts`; ability/mission display names are resolved by the UI from bundled data.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager, State};

use super::{fetch_profile_json, load_account, now_ms};
use crate::db::Db;
use crate::services::cache::{kv_get, kv_put};
use crate::services::inventory::Inventory;
use crate::services::inventory::payload::{has_inventory_shape, unwrap_inventory_payload};

const MAX_ROWS: usize = 10_000;
const MAX_CONFIGS: usize = 12;
const MAX_SKINS: usize = 128;
const REFRESH_MIN_MS: i64 = 60_000;
const CAREER_KEYS: [&str; 17] = [
    "TimePlayedSec",
    "Income",
    "ReviveCount",
    "HealCount",
    "Deaths",
    "MeleeKills",
    "MissionsCompleted",
    "MissionsFailed",
    "MissionsQuit",
    "MissionsInterrupted",
    "MissionsDumped",
    "CiphersSolved",
    "CiphersFailed",
    "CipherTime",
    "PickupCount",
    "DestroyCount",
    "FishCount",
];
const EQUIPMENT_FIELDS: [&str; 5] = ["equipTime", "kills", "headshots", "assists", "xp"];
const ENEMY_FIELDS: [&str; 6] = [
    "kills",
    "headshots",
    "assists",
    "finishers",
    "deaths",
    "scans",
];
const COLOR_GROUPS: [&str; 4] = ["pricol", "attcol", "syancol", "sigcol"];
const COLOR_CHANNELS: [&str; 8] = ["t0", "t1", "t2", "t3", "m0", "m1", "en", "e1"];
/// (category, preset key): DE abbreviates suit, long gun, pistol and melee slots.
const CATEGORIES: [(&str, &str); 4] = [
    ("Suits", "s"),
    ("LongGuns", "l"),
    ("Pistols", "p"),
    ("Melee", "m"),
];

/// (account, attempt time, failed) of the last manual refresh.
static ATTEMPT: Mutex<Option<(String, i64, bool)>> = Mutex::new(None);

/// One row of a cumulative stats table; `stats` holds the numeric columns that were present.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ProfileStatRow {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: Option<String>,
    pub stats: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ProfileSkin {
    pub slot: u32,
    #[serde(rename = "type")]
    pub kind: String,
}

/// `colors[group][channel]` = `#rrggbbaa`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct ProfileAppearanceConfig {
    pub name: Option<String>,
    pub skins: Vec<ProfileSkin>,
    pub colors: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileAppearanceItem {
    /// `Suits`, `LongGuns`, `Pistols` or `Melee`.
    pub category: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub active_config: Option<u32>,
    pub hidden_when_holstered: Option<bool>,
    pub configs: Vec<ProfileAppearanceConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PersonalProfile {
    pub display_name: Option<String>,
    pub mastery_rank: Option<u32>,
    /// Unix ms.
    pub registered_at: Option<f64>,
    pub career: BTreeMap<String, f64>,
    pub equipment: Option<Vec<ProfileStatRow>>,
    pub enemies: Option<Vec<ProfileStatRow>>,
    pub abilities: Option<Vec<ProfileStatRow>>,
    pub missions: Option<Vec<ProfileStatRow>>,
    pub appearance: Vec<ProfileAppearanceItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LoadoutModUpgrade {
    pub slot: u32,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub rank: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LoadoutModConfig {
    pub name: Option<String>,
    pub upgrades: Option<Vec<LoadoutModUpgrade>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoadoutItem {
    pub category: String,
    #[serde(rename = "type")]
    pub kind: String,
    /// Appearance config chosen for this slot (`cus`).
    pub active_config: Option<u32>,
    pub hidden_when_holstered: Option<bool>,
    pub configs: Vec<ProfileAppearanceConfig>,
    /// Build config chosen for this slot (`mod`).
    pub active_mod_config: Option<u32>,
    pub mod_configs: Vec<LoadoutModConfig>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct LoadoutSlot {
    pub category: String,
    /// `None` when the preset points at equipment that is no longer in the inventory.
    pub item: Option<LoadoutItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct SavedLoadout {
    pub name: Option<String>,
    pub slots: Vec<LoadoutSlot>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PersonalProfileResult {
    pub profile: Option<PersonalProfile>,
    pub fetched_at: Option<f64>,
    /// `ready`, `no-data`, `no-account` or `fetch-failed`.
    pub status: String,
    pub next_refresh_at: f64,
}

fn text(v: Option<&Value>, max: usize) -> Option<String> {
    let t = v?.as_str()?.trim();
    (!t.is_empty() && t.chars().count() <= max).then(|| t.to_owned())
}

fn num(v: Option<&Value>) -> Option<f64> {
    v?.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

fn oid(v: Option<&Value>) -> Option<String> {
    text(v?.get("$oid"), 512)
}

fn index(v: Option<&Value>, len: usize) -> Option<u32> {
    v?.as_u64()
        .filter(|i| (*i as usize) < len)
        .map(|i| i as u32)
}

fn lotus_path(v: Option<&Value>) -> Option<String> {
    text(v, 512).filter(|t| t.starts_with("/Lotus/"))
}

fn numeric_fields(src: &Value, keys: &[&str]) -> BTreeMap<String, f64> {
    keys.iter()
        .filter_map(|k| Some(((*k).to_owned(), num(src.get(*k))?)))
        .collect()
}

/// Duplicate cumulative counters overlap, so each column keeps its maximum rather than a sum.
fn table(v: Option<&Value>, keys: &[&str], with_names: bool) -> Option<Vec<ProfileStatRow>> {
    let mut rows: Vec<ProfileStatRow> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for e in v?.as_array()?.iter().take(MAX_ROWS) {
        let Some(kind) = text(e.get("type"), 512) else {
            continue;
        };
        let mut row = ProfileStatRow {
            kind: kind.clone(),
            name: with_names.then(|| text(e.get("name"), 240)).flatten(),
            stats: numeric_fields(e, keys),
        };
        if let Some(&i) = at.get(&kind) {
            let prev = &rows[i];
            for (k, old) in &prev.stats {
                let slot = row.stats.entry(k.clone()).or_insert(*old);
                *slot = slot.max(*old);
            }
            row.name = row.name.or_else(|| prev.name.clone());
            rows[i] = row;
        } else {
            at.insert(kind, rows.len());
            rows.push(row);
        }
    }
    Some(rows)
}

/// A packed ARGB int to `#rrggbbaa`.
fn color(v: &Value) -> Option<String> {
    let n = v.as_i64()?;
    if !(-0x8000_0000..=0xffff_ffff).contains(&n) {
        return None;
    }
    let packed = format!("{:08x}", n as u32);
    Some(format!("#{}{}", &packed[2..], &packed[..2]))
}

fn appearance_config(
    cfg: &Value,
    resolve_skin: &dyn Fn(&str) -> Option<String>,
) -> ProfileAppearanceConfig {
    let mut out = ProfileAppearanceConfig {
        name: text(cfg.get("Name"), 120),
        ..Default::default()
    };
    if let Some(skins) = cfg.get("Skins").and_then(Value::as_array) {
        for (slot, s) in skins.iter().take(MAX_SKINS).enumerate() {
            let Some(raw) = text(Some(s), 512) else {
                continue;
            };
            let Some(kind) = resolve_skin(&raw) else {
                continue;
            };
            if !kind.ends_with("EmptyCustomization") {
                out.skins.push(ProfileSkin {
                    slot: slot as u32,
                    kind,
                });
            }
        }
    }
    for group in COLOR_GROUPS {
        let Some(input) = cfg.get(group).filter(|g| g.is_object()) else {
            continue;
        };
        let channels: BTreeMap<String, String> = COLOR_CHANNELS
            .iter()
            .filter_map(|c| Some(((*c).to_owned(), color(input.get(*c)?)?)))
            .collect();
        if !channels.is_empty() {
            out.colors.insert(group.to_owned(), channels);
        }
    }
    out
}

fn profile_appearance(player: &Value) -> Vec<ProfileAppearanceItem> {
    let mut out = Vec::new();
    for (category, key) in CATEGORIES {
        let Some(entries) = player
            .pointer(&format!("/LoadOutInventory/{category}"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let selection = player.pointer(&format!("/LoadOutPreset/{key}"));
        let selected_id = oid(selection.and_then(|s| s.get("ItemId")));
        for item in entries.iter().take(16) {
            let Some(kind) = text(item.get("ItemType"), 512) else {
                continue;
            };
            let configs: Vec<_> = item
                .get("Configs")
                .and_then(Value::as_array)
                .map(|c| {
                    c.iter()
                        .take(MAX_CONFIGS)
                        .map(|c| appearance_config(c, &|s| Some(s.to_owned())))
                        .collect()
                })
                .unwrap_or_default();
            let selected = match &selected_id {
                Some(id) => oid(item.get("ItemId")).as_ref() == Some(id),
                None => entries.len() == 1,
            };
            out.push(ProfileAppearanceItem {
                category: category.to_owned(),
                kind,
                active_config: selected
                    .then(|| index(selection.and_then(|s| s.get("cus")), configs.len()))
                    .flatten(),
                hidden_when_holstered: selected
                    .then(|| selection.and_then(|s| s.get("hide")?.as_bool()))
                    .flatten(),
                configs,
            });
        }
    }
    out
}

/// `Created.$date` is a millisecond number, or `{"$numberLong": "<ms>"}`.
fn created_at(v: Option<&Value>) -> Option<f64> {
    let date = v?.get("$date")?;
    let raw = date.get("$numberLong").unwrap_or(date);
    let ms = match raw {
        Value::String(s) if s.len() <= 16 && s.bytes().all(|b| b.is_ascii_digit()) => {
            s.parse::<f64>().ok()?
        }
        other => num(Some(other))?,
    };
    (ms <= 8.64e15 && ms.fract() == 0.0).then_some(ms)
}

/// `None` when the payload carries no stats at all (private or empty profile).
pub fn parse_personal_profile(payload: &Value) -> Option<PersonalProfile> {
    let root = payload.as_object()?;
    let player = payload
        .pointer("/Results/0")
        .filter(|p| p.is_object())
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let root_stats = root.get("Stats").filter(|s| s.is_object());
    let player_stats = player.get("Stats").filter(|s| s.is_object());
    if root_stats.is_none() && player_stats.is_none() {
        return None;
    }
    // The root block wins over the nested one, key by key.
    let mut merged = player_stats
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    merged.extend(
        root_stats
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
    );
    let stats = Value::Object(merged);
    let mut enemies = table(stats.get("Enemies"), &ENEMY_FIELDS, false);
    let scans: HashMap<String, f64> = table(stats.get("Scans"), &["scans"], false)
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r.kind, *r.stats.get("scans")?)))
        .collect();
    for e in enemies.iter_mut().flatten() {
        if let (false, Some(n)) = (e.stats.contains_key("scans"), scans.get(&e.kind)) {
            e.stats.insert("scans".into(), *n);
        }
    }
    Some(PersonalProfile {
        display_name: text(player.get("displayName"), 120)
            .or_else(|| text(player.get("DisplayName"), 120)),
        mastery_rank: num(player.get("PlayerLevel"))
            .filter(|n| n.fract() == 0.0 && *n <= f64::from(u32::MAX))
            .map(|n| n as u32),
        registered_at: created_at(player.get("Created")),
        career: numeric_fields(&stats, &CAREER_KEYS),
        equipment: table(stats.get("Weapons"), &EQUIPMENT_FIELDS, false),
        enemies,
        abilities: table(stats.get("Abilities"), &["used"], true),
        missions: table(stats.get("Missions"), &["highScore"], true),
        appearance: profile_appearance(&player),
    })
}

fn upgrade_rank(v: Option<&Value>) -> Option<u32> {
    let s = v?.as_str().filter(|s| s.len() <= 16_384)?;
    let lvl = serde_json::from_str::<Value>(s)
        .ok()?
        .get("lvl")?
        .as_u64()?;
    (lvl <= 100).then_some(lvl as u32)
}

fn is_owned_skin_id(s: &str) -> bool {
    s.len() == 24 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The saved NORMAL loadouts, with each slot's equipment, appearance configs and mod configs.
/// Empty when the inventory has no presets.
pub fn parse_personal_loadouts(inventory: &Value) -> Vec<SavedLoadout> {
    let Some(presets) = inventory
        .pointer("/LoadOutPresets/NORMAL")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let list = |key: &str, cap: usize| -> Vec<&Value> {
        inventory
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().take(cap).collect())
            .unwrap_or_default()
    };
    let upgrades: HashMap<String, (Option<String>, Option<u32>)> = list("Upgrades", 100_000)
        .into_iter()
        .filter_map(|u| {
            Some((
                oid(u.get("ItemId"))?,
                (
                    lotus_path(u.get("ItemType")),
                    upgrade_rank(u.get("UpgradeFingerprint")),
                ),
            ))
        })
        .collect();
    // Configs reference owned cosmetics by their WeaponSkins id, not by type.
    let skins: HashMap<String, String> = list("WeaponSkins", 50_000)
        .into_iter()
        .filter_map(|s| Some((oid(s.get("ItemId"))?, lotus_path(s.get("ItemType"))?)))
        .collect();
    let resolve_skin = |t: &str| {
        if is_owned_skin_id(t) {
            skins.get(t).cloned()
        } else {
            Some(t.to_owned())
        }
    };
    let mut equipment: HashMap<(&str, String), &Value> = HashMap::new();
    for (category, _) in CATEGORIES {
        for item in list(category, 10_000) {
            if let Some(id) = oid(item.get("ItemId")) {
                equipment.insert((category, id), item);
            }
        }
    }
    presets
        .iter()
        .take(128)
        .filter(|p| p.is_object())
        .map(|preset| {
            let mut loadout = SavedLoadout {
                name: text(preset.get("n"), 120),
                slots: Vec::new(),
            };
            for (category, key) in CATEGORIES {
                let selection = preset.get(key);
                let Some(id) = oid(selection.and_then(|s| s.get("ItemId"))) else {
                    continue;
                };
                let found = equipment
                    .get(&(category, id))
                    .and_then(|raw| Some((*raw, lotus_path(raw.get("ItemType"))?)));
                let Some((raw, kind)) = found else {
                    loadout.slots.push(LoadoutSlot {
                        category: category.to_owned(),
                        item: None,
                    });
                    continue;
                };
                let configs: Vec<&Value> = raw
                    .get("Configs")
                    .and_then(Value::as_array)
                    .map(|c| c.iter().take(MAX_CONFIGS).collect())
                    .unwrap_or_default();
                let sel = |k: &str| index(selection.and_then(|s| s.get(k)), configs.len());
                loadout.slots.push(LoadoutSlot {
                    category: category.to_owned(),
                    item: Some(LoadoutItem {
                        category: category.to_owned(),
                        kind,
                        active_config: sel("cus"),
                        hidden_when_holstered: selection.and_then(|s| s.get("hide")?.as_bool()),
                        configs: configs
                            .iter()
                            .map(|c| appearance_config(c, &resolve_skin))
                            .collect(),
                        active_mod_config: sel("mod"),
                        mod_configs: configs
                            .iter()
                            .map(|c| LoadoutModConfig {
                                name: text(c.get("Name"), 120),
                                upgrades: c.get("Upgrades").and_then(Value::as_array).map(|refs| {
                                    refs.iter()
                                        .take(32)
                                        .enumerate()
                                        .filter(|(_, r)| r.as_str() != Some(""))
                                        .map(|(slot, r)| {
                                            let (kind, rank) = r
                                                .as_str()
                                                .and_then(|s| upgrades.get(s))
                                                .cloned()
                                                .unwrap_or_else(|| (lotus_path(Some(r)), None));
                                            LoadoutModUpgrade {
                                                slot: slot as u32,
                                                kind,
                                                rank,
                                            }
                                        })
                                        .collect()
                                }),
                            })
                            .collect(),
                    }),
                });
            }
            loadout
        })
        .collect()
}

fn profile_key(account: &str) -> String {
    format!("personal_profile:{account}")
}

fn profile_at_key(account: &str) -> String {
    format!("personal_profile_at:{account}")
}

fn read_cache(db: &Db, account: &str) -> Option<(i64, PersonalProfile)> {
    let at = kv_get(db, &profile_at_key(account)).ok()??.parse().ok()?;
    let profile = serde_json::from_str(&kv_get(db, &profile_key(account)).ok()??).ok()?;
    Some((at, profile))
}

fn write_cache(db: &Db, account: &str, at: i64, p: &PersonalProfile) -> Result<(), String> {
    let body = serde_json::to_string(p).map_err(|e| e.to_string())?;
    kv_put(db, &profile_key(account), &body).map_err(|e| e.to_string())?;
    kv_put(db, &profile_at_key(account), &at.to_string()).map_err(|e| e.to_string())
}

fn result(account: &str, cached: Option<&(i64, PersonalProfile)>) -> PersonalProfileResult {
    let attempt = ATTEMPT
        .lock()
        .unwrap()
        .clone()
        .filter(|(a, _, _)| a == account);
    let failed = attempt
        .as_ref()
        .is_some_and(|(_, at, f)| *f && cached.is_none_or(|(c, _)| at >= c));
    let next = cached
        .map_or(0, |(c, _)| c + REFRESH_MIN_MS)
        .max(attempt.map_or(0, |(_, at, _)| at + REFRESH_MIN_MS));
    PersonalProfileResult {
        profile: cached.map(|(_, p)| p.clone()),
        fetched_at: cached.map(|(c, _)| *c as f64),
        status: if failed {
            "fetch-failed"
        } else if cached.is_none() {
            "no-data"
        } else {
            "ready"
        }
        .into(),
        next_refresh_at: next as f64,
    }
}

/// Cached personal profile; `refresh` (manual, rate-limited) re-fetches it from the public profile.
#[tauri::command]
#[specta::specta]
pub async fn get_personal_profile(
    app: AppHandle,
    refresh: bool,
) -> Result<PersonalProfileResult, String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // File and SQLite reads stay off the async workers, where they queue behind the price and image loads.
    let reader = app.clone();
    let read = tauri::async_runtime::spawn_blocking(move || {
        let account = load_account(&data)?;
        let cached = read_cache(&reader.state::<Db>(), &account);
        Some((account, cached))
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some((account, cached)) = read else {
        return Ok(PersonalProfileResult {
            status: "no-account".into(),
            ..Default::default()
        });
    };
    let db = app.state::<Db>();
    if !refresh || now_ms() < result(&account, cached.as_ref()).next_refresh_at as i64 {
        return Ok(result(&account, cached.as_ref()));
    }
    let at = now_ms();
    let outcome = match fetch_profile_json(&account).await {
        Ok(json) => {
            super::remember_profile_inventory(&app, &account, &json);
            parse_personal_profile(&json)
                .ok_or_else(|| "no stats in profile payload".to_owned())
                .and_then(|p| write_cache(&db, &account, at, &p))
        }
        Err(e) => Err(e),
    };
    if let Err(e) = &outcome {
        tracing::warn!(error = %e, "personal profile fetch failed");
    }
    *ATTEMPT.lock().unwrap() = Some((account.clone(), at, outcome.is_err()));
    Ok(result(&account, read_cache(&db, &account).as_ref()))
}

/// Saved loadouts from the loaded inventory (empty when none is loaded).
#[tauri::command(async)]
#[specta::specta]
pub fn get_personal_loadouts(inv: State<Arc<Inventory>>) -> Vec<SavedLoadout> {
    let Some(v) = inv.current() else {
        return Vec::new();
    };
    if has_inventory_shape(&v) {
        parse_personal_loadouts(&v)
    } else {
        parse_personal_loadouts(&unwrap_inventory_payload((*v).clone(), None))
    }
}

#[cfg(test)]
mod tests;
