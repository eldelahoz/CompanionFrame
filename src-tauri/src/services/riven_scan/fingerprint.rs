//! Decodes the `UpgradeFingerprint` of inventory rivens into stats with their roll floats and
//! grades (port of WFHelper's `rivenFingerprint`). A fingerprint is the game's own identity of a
//! roll, so two reads of one riven decode identically and a reroll changes them.

use serde::Serialize;
use serde_json::Value;

use crate::services::rivens::best_attributes::GoodRolls;
use crate::services::rivens::data::RivenData;
use crate::services::rivens::grading::{RivenStatInput, compute_attribute_grade, float_to_grade};
use crate::services::rivens::vocab::{
    BASE_DRAIN, NUM_BUFFS_ATTEN, NUM_BUFFS_CURSE_ATTEN, SPECIFIC_FIT_ATTEN, is_non_percentage,
};

const MAX_RIVEN_RANK: usize = 8;
/// Fingerprint values encode rolls as `round(f * 0x3FFFFFFF)`, not IEEE floats.
const ROLL_INT_MAX: f64 = 1_073_741_823.0;

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DecodedRivenStat {
    pub tag: String,
    pub name: String,
    pub display_value: f64,
    /// Same roll recomputed at rank 8, for listing an unranked riven as maxed.
    pub max_rank_value: f64,
    /// Each rank recomputed from the fingerprint to avoid scaling rounded values.
    pub rank_values: Vec<f64>,
    pub roll_float: f64,
    pub grade: String,
    pub positive: bool,
    /// Faction-damage style stats, displayed as xN.NN.
    pub multiplier: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DecodedRiven {
    pub item_id: String,
    pub weapon_name: String,
    pub weapon_unique_name: String,
    pub riven_name: String,
    pub mastery_req: f64,
    pub current_rank: f64,
    pub max_rank: u8,
    pub rerolls: f64,
    pub polarity: String,
    pub disposition: f64,
    pub stats: Vec<DecodedRivenStat>,
    pub overall_grade: String,
    pub attribute_grade: String,
    /// Average roll float across all stats; higher is closer to perfect.
    pub stat_perfectness: f64,
    pub riven_type: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VeiledRivenEntry {
    pub item_type: String,
    pub label: String,
    pub challenge_type: Option<String>,
    pub challenge_desc: Option<String>,
    /// `challenge_desc` without its count or complication.
    pub challenge_group: Option<String>,
    pub challenge_progress: Option<f64>,
    pub challenge_required: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VeiledRivenGroup {
    pub item_type: String,
    pub label: String,
    pub count: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DecodedRivens {
    pub unveiled: Vec<DecodedRiven>,
    pub veiled: Vec<VeiledRivenEntry>,
    pub veiled_unseen: Vec<VeiledRivenGroup>,
}

const RIVEN_TYPE_LABELS: &[(&str, &str)] = &[
    // Lotus (unveiled / individual-veiled in Upgrades)
    ("LotusRifleRandomModRare", "Rifle"),
    ("LotusShotgunRandomModRare", "Shotgun"),
    ("LotusPistolRandomModRare", "Pistol"),
    ("PlayerMeleeWeaponRandomModRare", "Melee"),
    ("LotusArchgunRandomModRare", "Archgun"),
    ("LotusModularPistolRandomModRare", "Kitgun"),
    ("LotusModularMeleeRandomModRare", "Zaw"),
    // Raw (stackable veiled in RawUpgrades)
    ("RawRifleRandomMod", "Rifle"),
    ("RawShotgunRandomMod", "Shotgun"),
    ("RawPistolRandomMod", "Pistol"),
    ("RawMeleeRandomMod", "Melee"),
    ("RawArchgunRandomMod", "Archgun"),
    ("RawModularPistolRandomMod", "Kitgun"),
    ("RawModularMeleeRandomMod", "Zaw"),
];

/// Keys match exact challenge path suffixes; `{n}` is replaced with the Required count.
const CHALLENGE_DESCS: &[(&str, &str)] = &[
    ("RandomizedKill", "Kill {n} Enemies"),
    (
        "RandomizedKillPassengers",
        "Kill {n} Enemies that are on a Dropship",
    ),
    (
        "RandomizedKillFallingPilots",
        "Kill {n} Enemies with Headshots",
    ),
    ("RandomizedFinisherKill", "Kill {n} Enemies with Finishers"),
    ("RandomizedHeadshot", "Kill {n} Enemies with Headshots"),
    (
        "RandomizedHeadshotGlide",
        "Get {n} Headshot kills in a single Aim Glide",
    ),
    ("RandomizedStyleKill", "Kill {n} Enemies while Sliding"),
    (
        "RandomizedWallClingKillstreak",
        "Get {n} kills in a row while Wall Dashing or Wall Latching without touching the floor",
    ),
    (
        "RandomizedHeadshotUnawareBallistas",
        "Kill {n} unalerted Tusk Ballistas with a Headshot",
    ),
    (
        "RandomizedKillSentients",
        "Get the killing blow on {n} Sentients",
    ),
    (
        "RandomizedLongRangeSniper",
        "Kill {n} Enemies with Headshots from at least 75m away",
    ),
    (
        "RandomizedSkiffArcher",
        "Destroy {n} Dargyns in flight using a bow",
    ),
    (
        "RandomizedAntiAntiAir",
        "Destroy {n} Vruush Turrets while in Archwing without dying or becoming downed",
    ),
    (
        "RandomizedFlyingHeadshotSeries",
        "Land {n} consecutive headshots while in Archwing in the Plains of Eidolon",
    ),
    ("RandomizedFindCaches", "Find {n} Caches"),
    (
        "RandomizedFindRareMedallions",
        "Pick up {n} Syndicate Medallions",
    ),
    (
        "RandomizedFisherman",
        "Catch {n} fish without missing a throw",
    ),
    (
        "SustainMeleeComboThree",
        "Sustain a 6x Melee Combo Multiplier for 30s",
    ),
    (
        "HighSurvivalPacifist",
        "Complete a Survival mission with level 30 or higher enemies without killing anyone",
    ),
    (
        "HighExterminationUndetected",
        "Complete an Extermination mission with level 30 or higher enemies without being detected",
    ),
    (
        "HighPerfectDefense",
        "Complete a Defense mission with level 30 or higher enemies with the defense objective taking no damage",
    ),
    (
        "HighSoloInterceptionHobbled",
        "Complete a Solo Interception mission with level 30 or higher enemies and a Hobbled Dragon Key equipped",
    ),
    (
        "LimitedSynthesis",
        "Synthesize a Simaris target without using Traps or Abilities while having a Hobbled Dragon Key equipped",
    ),
    (
        "PlainsTimedVariety",
        "Catch one fish, mine one gem or metal, and kill one enemy in 30 seconds",
    ),
    ("KahlMissions", "Complete {n} Kahl missions"),
    // DJRandomized is the same challenge set from a different pool.
    ("DJRandomizedKill", "Kill {n} Enemies"),
    (
        "DJRandomizedFinisherKill",
        "Kill {n} Enemies with Finishers",
    ),
    ("DJRandomizedHeadshot", "Kill {n} Enemies with Headshots"),
    ("DJRandomizedStyleKill", "Kill {n} Enemies while Sliding"),
    (
        "DJRandomizedWallClingKillstreak",
        "Get {n} kills in a row while Wall Dashing or Wall Latching without touching the floor",
    ),
    (
        "DJRandomizedKillPassengers",
        "Kill {n} Enemies that are on a Dropship",
    ),
    (
        "DJRandomizedHeadshotUnawareBallistas",
        "Kill {n} unalerted Tusk Ballistas with a Headshot",
    ),
    ("DJRandomizedFindCaches", "Find {n} Caches"),
];

const COMPLICATION_DESCS: &[(&str, &str)] = &[
    ("ResetOnDamageTaken", "without taking damage"),
    ("ResetOnDowned", "without dying or becoming downed"),
    ("ResetOnMissionFailure", "without failing a mission"),
    ("ResetOnAlarmRaised", "without raising any alarms"),
    ("ResetOnAllyDowned", "without an ally becoming downed"),
    (
        "ResetOnDisrupt",
        "without being disrupted by a Magnetic Status Effect",
    ),
    ("ResetOnGearAirSupport", "without using air support"),
    ("ResetOnGearAmmoRestores", "without using ammo consumables"),
    ("ResetOnGearCipher", "without using ciphers"),
    (
        "ResetOnGearEnergyRestores",
        "without using energy consumables",
    ),
    (
        "ResetOnGearHealthRestores",
        "without using health consumables",
    ),
    (
        "ResetOnGearShieldRestores",
        "without using shield-restoring consumables",
    ),
    (
        "ResetOnProc",
        "without getting afflicted by a Status Effect",
    ),
    ("ResetOnNewDay", "in one day"),
    (
        "EquippedDamageDebuffKey",
        "with an Extinguished Dragon Key equipped",
    ),
    (
        "EquippedHealthDebuffKey",
        "with a Bleeding Dragon Key equipped",
    ),
    (
        "EquippedShieldDebuffKey",
        "with a Decaying Dragon Key equipped",
    ),
    (
        "EquippedSpeedDebuffKey",
        "with a Hobbled Dragon Key equipped",
    ),
    ("SoloPlayer", ", while alone or in Solo Mode"),
    ("PetPresent", "with an active pet present"),
    ("SentinelPresent", "with an active sentinel present"),
    ("Invisible", "while invisible"),
    ("AimGliding", "during Aim Glide"),
    ("Sliding", "while sliding"),
    ("Undetected", "while undetected"),
];

fn lookup(table: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn last_segment(path: &str) -> &str {
    match path.rsplit('/').next() {
        Some(s) if !s.is_empty() => s,
        _ => path,
    }
}

fn split_camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if c.is_ascii_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out.trim().to_owned()
}

/// JS `String(number)` for the integers a challenge count holds.
fn num_str(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn describe_challenge_type(
    challenge_type: &str,
    required: Option<f64>,
    complication: Option<&str>,
) -> String {
    let name = last_segment(challenge_type);
    let n = required.map_or_else(|| "?".to_owned(), num_str);
    let mut desc = match lookup(CHALLENGE_DESCS, name) {
        Some(template) => template.replace("{n}", &n),
        None => split_camel_case(name),
    };
    if let Some(text) =
        complication.and_then(|c| lookup(COMPLICATION_DESCS, c.rsplit('/').next().unwrap_or("")))
    {
        desc.push(' ');
        desc.push_str(text);
    }
    desc
}

fn describe_challenge_group(challenge_type: &str) -> String {
    let name = last_segment(challenge_type);
    match lookup(CHALLENGE_DESCS, name) {
        None => split_camel_case(name),
        Some(template) => {
            // `\s*{n}\s*` -> one space, then trim.
            let (head, tail) = template.split_once("{n}").unwrap_or((template, ""));
            format!("{} {}", head.trim_end(), tail.trim_start())
                .trim()
                .to_owned()
        }
    }
}

fn riven_type_label(item_type: &str) -> &'static str {
    RIVEN_TYPE_LABELS
        .iter()
        .find(|(key, _)| item_type.contains(key))
        .map_or("Riven", |(_, label)| label)
}

fn roll_int_to_float(i: f64) -> f64 {
    let f = i / ROLL_INT_MAX;
    if (0.0..=1.0).contains(&f) { f } else { 0.0 }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// JS `Math.round`: halves go toward +infinity.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn atten(table: &[f64; 6], n: usize) -> f64 {
    table[n.min(table.len() - 1)]
}

/// Faction-damage and initial-combo stats read as the final factor (x1.05), so the 1 is added.
fn is_multiplier_tag(tag: &str) -> bool {
    is_non_percentage(tag)
        && (tag.contains("FactionDamage") || tag == "WeaponMeleeComboInitialBonusMod")
}

struct StatCtx {
    base_value: f64,
    disposition: f64,
    roll_float: f64,
    num_buffs: usize,
    num_curses: usize,
    is_non_pct: bool,
    is_multiplier: bool,
}

fn round_stat_value(raw: f64, non_pct: bool) -> f64 {
    if non_pct {
        js_round(raw * 100.0) / 100.0
    } else {
        js_round(raw * 1000.0) / 10.0
    }
}

fn buff_value_at(ctx: &StatCtx, lvl: f64) -> f64 {
    if ctx.base_value == 0.0 {
        return if ctx.is_multiplier { 1.0 } else { 0.0 };
    }
    let attenuation = SPECIFIC_FIT_ATTEN * ctx.disposition * BASE_DRAIN;
    let raw = ctx.base_value
        * attenuation
        * 1.25f64.powi(ctx.num_curses as i32)
        * lerp(0.9, 1.1, ctx.roll_float)
        * atten(&NUM_BUFFS_ATTEN, ctx.num_buffs)
        * (lvl + 1.0);
    let value = round_stat_value(raw, ctx.is_non_pct);
    if ctx.is_multiplier {
        js_round((1.0 + value) * 100.0) / 100.0
    } else {
        value
    }
}

/// A curse runs opposite to the stat's buff direction: a damage curse shows -X%, a recoil curse
/// (negative base) shows +X%.
fn curse_value_at(ctx: &StatCtx, lvl: f64) -> f64 {
    if ctx.base_value == 0.0 {
        return 0.0;
    }
    let attenuation = SPECIFIC_FIT_ATTEN * ctx.disposition * BASE_DRAIN;
    let raw = ctx.base_value.abs()
        * attenuation
        * lerp(0.9, 1.1, ctx.roll_float)
        * atten(&NUM_BUFFS_CURSE_ATTEN, ctx.num_buffs)
        * atten(&NUM_BUFFS_ATTEN, ctx.num_curses)
        * (lvl + 1.0);
    let magnitude = round_stat_value(raw, ctx.is_non_pct);
    if ctx.is_multiplier {
        js_round((1.0 - magnitude) * 100.0) / 100.0
    } else if ctx.base_value > 0.0 {
        -magnitude
    } else {
        magnitude
    }
}

/// Some fingerprints are double-stringified.
fn parse_fingerprint(raw: &str) -> Option<Value> {
    let mut parsed: Value = serde_json::from_str(raw).ok()?;
    if let Value::String(inner) = &parsed {
        parsed = serde_json::from_str(inner).ok()?;
    }
    Some(parsed)
}

fn is_riven_item_type(item_type: &str) -> bool {
    item_type.contains("Randomized") || item_type.contains("RandomMod")
}

fn is_veiled_fingerprint(fp: &Value) -> bool {
    let compat = fp.get("compat").and_then(Value::as_str).unwrap_or("");
    fp.get("challenge").is_some_and(truthy) || compat.is_empty()
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

struct Roll<'a> {
    tag: &'a str,
    value: f64,
}

fn rolls_of<'a>(fp: &'a Value, key: &str) -> Vec<Roll<'a>> {
    fp.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|r| Roll {
                    tag: r.get("Tag").and_then(Value::as_str).unwrap_or(""),
                    value: r.get("Value").and_then(Value::as_f64).unwrap_or(0.0),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn decode_single(
    fp: &Value,
    item_type: &str,
    item_id: &str,
    data: &RivenData,
    good_rolls: &GoodRolls,
) -> Option<DecodedRiven> {
    let compat = fp.get("compat").and_then(Value::as_str)?;
    let weapon_name = data.weapon_name_by_unique_name(compat)?;
    let disposition = data.weapon_disposition(weapon_name)?;
    let riven_type_key = data.resolve_riven_type(weapon_name)?;
    let melee = data.is_melee_weapon(weapon_name);

    let lvl = fp.get("lvl").and_then(Value::as_f64).unwrap_or(0.0);
    let buffs = rolls_of(fp, "buffs");
    let curses = rolls_of(fp, "curses");
    let (num_buffs, num_curses) = (buffs.len(), curses.len());

    let mut stats: Vec<DecodedRivenStat> = Vec::with_capacity(num_buffs + num_curses);
    let mut roll_sum = 0.0;
    let mut scored = 0usize;

    for (rolls, is_curse) in [(&buffs, false), (&curses, true)] {
        for r in rolls.iter() {
            let roll_float = roll_int_to_float(r.value);
            let entry = data.find_upgrade_entry(riven_type_key, r.tag);
            let ctx = StatCtx {
                base_value: entry.map_or(0.0, |e| e.base_value),
                disposition,
                roll_float,
                num_buffs,
                num_curses,
                is_non_pct: is_non_percentage(r.tag),
                is_multiplier: is_multiplier_tag(r.tag),
            };
            let value_at = |lvl: f64| {
                if is_curse {
                    curse_value_at(&ctx, lvl)
                } else {
                    buff_value_at(&ctx, lvl)
                }
            };
            stats.push(DecodedRivenStat {
                tag: r.tag.to_owned(),
                name: data.stat_display_name(r.tag, melee),
                display_value: value_at(lvl),
                max_rank_value: value_at(MAX_RIVEN_RANK as f64),
                rank_values: (0..=MAX_RIVEN_RANK)
                    .map(|rank| value_at(rank as f64))
                    .collect(),
                roll_float,
                grade: float_to_grade(roll_float, is_curse).to_owned(),
                positive: !is_curse,
                multiplier: ctx.is_multiplier,
            });
            // For curses, a lower roll is better.
            roll_sum += if is_curse {
                1.0 - roll_float
            } else {
                roll_float
            };
            scored += 1;
        }
    }

    let avg = if scored > 0 {
        roll_sum / scored as f64
    } else {
        0.5
    };
    let overall_grade = float_to_grade(avg, false).to_owned();
    let inputs: Vec<RivenStatInput> = stats
        .iter()
        .map(|s| RivenStatInput {
            name: s.name.clone(),
            positive: s.positive,
            display_positive: None,
            value: None,
            multiplier: None,
        })
        .collect();
    let attribute_grade = compute_attribute_grade(data, good_rolls, &inputs, weapon_name);

    // Game rule: the suffix orders buffs by roll value, descending.
    let suffix = data.generate_suffix(
        riven_type_key,
        &buffs.iter().map(|b| (b.tag, b.value)).collect::<Vec<_>>(),
    );
    let riven_name = if suffix.is_empty() {
        weapon_name.to_owned()
    } else {
        format!("{weapon_name} {suffix}")
    };

    Some(DecodedRiven {
        item_id: item_id.to_owned(),
        weapon_name: weapon_name.to_owned(),
        weapon_unique_name: compat.to_owned(),
        riven_name,
        mastery_req: fp.get("lvlReq").and_then(Value::as_f64).unwrap_or(0.0),
        current_rank: lvl,
        max_rank: MAX_RIVEN_RANK as u8,
        rerolls: fp.get("rerolls").and_then(Value::as_f64).unwrap_or(0.0),
        polarity: fp
            .get("pol")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        disposition,
        stats,
        overall_grade,
        attribute_grade,
        stat_perfectness: avg_or_zero(roll_sum, scored),
        riven_type: riven_type_label(item_type).to_owned(),
    })
}

fn avg_or_zero(sum: f64, n: usize) -> f64 {
    if n > 0 { sum / n as f64 } else { 0.0 }
}

fn veiled_entry(item_type: &str, fp: &Value) -> VeiledRivenEntry {
    let mut entry = VeiledRivenEntry {
        item_type: item_type.to_owned(),
        label: riven_type_label(item_type).to_owned(),
        ..Default::default()
    };
    let Some(ch) = fp.get("challenge").filter(|c| c.is_object()) else {
        return entry;
    };
    entry.challenge_progress = ch.get("Progress").and_then(Value::as_f64);
    entry.challenge_required = ch.get("Required").and_then(Value::as_f64);
    if let Some(kind) = ch.get("Type").and_then(Value::as_str) {
        entry.challenge_type = Some(kind.to_owned());
        entry.challenge_desc = Some(describe_challenge_type(
            kind,
            entry.challenge_required,
            ch.get("Complication").and_then(Value::as_str),
        ));
        entry.challenge_group = Some(describe_challenge_group(kind));
    }
    entry
}

/// Every riven in an inventory: unveiled ones decoded, veiled ones with their challenge, and the
/// stackable unseen veiled mods counted per weapon class.
pub fn decode_all_rivens(
    inventory: &Value,
    data: &RivenData,
    good_rolls: &GoodRolls,
) -> DecodedRivens {
    let mut out = DecodedRivens::default();
    let mut unseen: Vec<(&'static str, f64)> = Vec::new();

    for u in inventory
        .get("Upgrades")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(item_type) = u
            .get("ItemType")
            .and_then(Value::as_str)
            .filter(|t| is_riven_item_type(t))
        else {
            continue;
        };
        let Some(raw) = u.get("UpgradeFingerprint").and_then(Value::as_str) else {
            continue;
        };
        let Some(fp) = parse_fingerprint(raw) else {
            continue;
        };
        if is_veiled_fingerprint(&fp) {
            out.veiled.push(veiled_entry(item_type, &fp));
            continue;
        }
        let item_id = u
            .pointer("/ItemId/$oid")
            .and_then(Value::as_str)
            .unwrap_or("");
        if let Some(decoded) = decode_single(&fp, item_type, item_id, data, good_rolls) {
            out.unveiled.push(decoded);
        }
    }

    for u in inventory
        .get("RawUpgrades")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(item_type) = u
            .get("ItemType")
            .and_then(Value::as_str)
            .filter(|t| is_riven_item_type(t))
        else {
            continue;
        };
        let count = u.get("ItemCount").and_then(Value::as_f64).unwrap_or(1.0);
        let label = riven_type_label(item_type);
        match unseen.iter_mut().find(|(l, _)| *l == label) {
            Some(slot) => slot.1 += count,
            None => unseen.push((label, count)),
        }
    }
    out.veiled_unseen = unseen
        .into_iter()
        .map(|(label, count)| VeiledRivenGroup {
            item_type: label.to_owned(),
            label: format!("{label} Riven Mod"),
            count,
        })
        .collect();
    out
}
