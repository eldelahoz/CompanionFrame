//! Raw `worldState.php` (or the community oracle's copy of it) to [`WorldState`].
//! Input stays `serde_json::Value` on purpose: DE has shipped scalars where arrays belong
//! before, and one bad field must not cost the whole world state.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::regions::{Ctx, faction_label};
use super::{
    Alert, AlertItem, ArchonHunt, ArchonMission, BountyJob, CalendarDay, CalendarEvent,
    CalendarSeason, CircuitChoices, CountedItem, DailyDeal, DuviriCycle, Fissure, Invasion,
    InvasionReward, InvasionSide, Nightwave, NightwaveChallenge, Sortie, SortieMission,
    SyndicateBounty, TimeWindow, Trader, TraderItem, WorldState, steel_path,
};
use crate::services::item_db::public_image_url;

const BROWSE_WF: &str = "https://browse.wf";

const HUB_NODE: &[(&str, &str)] = &[
    ("SaturnHUB", "Kronia Relay (Saturn)"),
    ("MarsHUB", "Strata Relay (Mars)"),
    ("CerberusHUB", "Orcus Relay (Pluto)"),
    ("PlutoHUB", "Orcus Relay (Pluto)"),
    ("EarthHUB", "Larunda Relay (Earth)"),
    ("VenusHUB", "Vesper Relay (Venus)"),
    ("EuropaHUB", "Leonov Relay (Europa)"),
    ("NeptuneHUB", "Maroo's Bazaar (Mars)"),
    ("Relay Node 0", "Larunda Relay (Earth)"),
    ("Relay Node 4", "Strata Relay (Mars)"),
    ("Relay Node 9", "Vesper Relay (Venus)"),
    ("Relay Node 12", "Kronia Relay (Saturn)"),
    ("Relay Node 17", "Orcus Relay (Pluto)"),
    ("Relay Node 20", "Leonov Relay (Europa)"),
];

/// Browse.wf icon overrides for items missing from the public exports.
const BARO_ICON_OVERRIDES: [(&str, &str); 1] = [(
    "/Lotus/Types/Items/ShipDecos/Plushies/PlushyNecraLoid",
    "/Lotus/Interface/Icons/StoreIcons/ShipDecos/Decorations/NecraloidFloof.png",
)];

/// Fallback `MT_` labels for the enums DE's ExportMissionTypes leaves unnamed.
const MISSION_TYPE_LABELS: &[(&str, &str)] = &[
    ("MT_ALCHEMY", "Alchemy"),
    ("MT_ARENA", "Arena"),
    ("MT_ARMAGEDDON", "Void Armageddon"),
    ("MT_ARTIFACT", "Disruption"),
    ("MT_ASSASSINATION", "Assassination"),
    ("MT_ASSAULT", "Assault"),
    ("MT_CAPTURE", "Capture"),
    ("MT_CORRUPTION", "Void Flood"),
    ("MT_COUNTER_INTEL", "Deception"),
    ("MT_DEFENSE", "Defense"),
    ("MT_ENDLESS_CAPTURE", "Legacyte Harvest"),
    ("MT_ENDLESS_EXTERMINATION", "Sanctuary Onslaught"),
    ("MT_EVACUATION", "Defection"),
    ("MT_EXCAVATE", "Excavation"),
    ("MT_EXTERMINATION", "Exterminate"),
    ("MT_HIVE", "Hive"),
    ("MT_INTEL", "Spy"),
    ("MT_JUNCTION", "Solar Rail Junction"),
    ("MT_LANDSCAPE", "Free Roam"),
    ("MT_MOBILE_DEFENSE", "Mobile Defense"),
    ("MT_PURIFY", "Infested Salvage"),
    ("MT_PURSUIT", "Pursuit"),
    ("MT_PVP", "Conclave"),
    ("MT_RACE", "Rush"),
    ("MT_RAILJACK", "Railjack"),
    ("MT_RESCUE", "Rescue"),
    ("MT_RETRIEVAL", "Hijack"),
    ("MT_SABOTAGE", "Sabotage"),
    ("MT_SURVIVAL", "Survival"),
    ("MT_TERRITORY", "Interception"),
    ("MT_VAULTS", "Netracell"),
    ("MT_VOID_CASCADE", "Void Cascade"),
];

/// Sortie modifiers have no dictionary entry of any language, so these stay the English
/// community labels; anything unmapped degrades to its title-cased tail.
const SORTIE_MODIFIER: &[(&str, &str)] = &[
    ("SORTIE_MODIFIER_ARMOR", "Enhanced Enemy Armor"),
    ("SORTIE_MODIFIER_SHIELDS", "Augmented Enemy Shields"),
    ("SORTIE_MODIFIER_HEALTH", "Enhanced Enemy Health"),
    ("SORTIE_MODIFIER_EXIMUS", "Eximus Stronghold"),
    ("SORTIE_MODIFIER_LOW_ENERGY", "Energy Reduction"),
    ("SORTIE_MODIFIER_LOW_GRAVITY", "Low Gravity"),
    ("SORTIE_MODIFIER_HIGH_GRAVITY", "High Gravity"),
    (
        "SORTIE_MODIFIER_RADIATION",
        "Enemy Elemental Enhancement: Radiation",
    ),
    (
        "SORTIE_MODIFIER_VIRAL",
        "Enemy Elemental Enhancement: Viral",
    ),
    (
        "SORTIE_MODIFIER_CORROSIVE",
        "Enemy Elemental Enhancement: Corrosive",
    ),
    (
        "SORTIE_MODIFIER_MAGNETIC",
        "Enemy Elemental Enhancement: Magnetic",
    ),
    ("SORTIE_MODIFIER_GAS", "Enemy Elemental Enhancement: Gas"),
    (
        "SORTIE_MODIFIER_EXPLOSION",
        "Enemy Elemental Enhancement: Blast",
    ),
    ("SORTIE_MODIFIER_FIRE", "Enemy Elemental Enhancement: Heat"),
    (
        "SORTIE_MODIFIER_FREEZE",
        "Enemy Elemental Enhancement: Cold",
    ),
    (
        "SORTIE_MODIFIER_ELECTRICITY",
        "Enemy Elemental Enhancement: Electricity",
    ),
    (
        "SORTIE_MODIFIER_POISON",
        "Enemy Elemental Enhancement: Toxin",
    ),
    (
        "SORTIE_MODIFIER_IMPACT",
        "Enemy Physical Enhancement: Impact",
    ),
    (
        "SORTIE_MODIFIER_PUNCTURE",
        "Enemy Physical Enhancement: Puncture",
    ),
    ("SORTIE_MODIFIER_SLASH", "Enemy Physical Enhancement: Slash"),
    ("SORTIE_MODIFIER_SECONDARY_ONLY", "Secondary Only"),
    ("SORTIE_MODIFIER_RIFLE_ONLY", "Assault Rifle Only"),
    ("SORTIE_MODIFIER_SHOTGUN_ONLY", "Shotgun Only"),
    ("SORTIE_MODIFIER_SNIPER_ONLY", "Sniper Only"),
    ("SORTIE_MODIFIER_BOW_ONLY", "Bow Only"),
    ("SORTIE_MODIFIER_MELEE_ONLY", "Melee Only"),
    ("SORTIE_MODIFIER_HAZARD_RADIATION", "Radiation Hazard"),
    ("SORTIE_MODIFIER_HAZARD_MAGNETIC", "Magnetic Anomaly"),
    ("SORTIE_MODIFIER_HAZARD_FIRE", "Fire Hazard"),
    ("SORTIE_MODIFIER_HAZARD_ICE", "Cryogenic Leakage"),
    ("SORTIE_MODIFIER_HAZARD_COLD", "Cryogenic Leakage"),
    (
        "SORTIE_MODIFIER_HAZARD_ELECTRICITY",
        "Electromagnetic Anomalies",
    ),
    ("SORTIE_MODIFIER_HAZARD_FOG", "Dense Fog"),
];

/// Raw syndicate missions that ship their jobs in the world state itself.
const RAW_BOUNTY_SYNDICATES: [(&str, &str); 3] = [
    ("CetusSyndicate", "Ostrons"),
    ("SolarisSyndicate", "Solaris United"),
    ("EntratiSyndicate", "Entrati"),
];

/// Display names whose warframestat.us syndicate missions are kept as bounties.
pub const BOUNTY_SYNDICATES: [&str; 6] = [
    "Ostrons",
    "Solaris United",
    "Entrati",
    "The Holdfasts",
    "Cavia",
    "The Hex",
];

/// Display name for a raw-payload syndicate tag.
pub fn raw_bounty_name(key: &str) -> Option<&'static str> {
    RAW_BOUNTY_SYNDICATES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, n)| *n)
}

const STANDING_TIERS: [[u32; 2]; 7] = [
    [1000, 1500],
    [2000, 3000],
    [3000, 4500],
    [4000, 6000],
    [5000, 7500],
    [6000, 9000],
    [7500, 11250],
];

/// Seed-generated syndicates the oracle resolves: key, display name, standing tiers used,
/// and in-game level tiers per job index. The Hex levels run 10 above DE's drop-table labels.
const BOUNTY_CYCLE_SYNDICATES: [(&str, &str, usize, [[u32; 2]; 7]); 3] = [
    (
        "ZarimanSyndicate",
        "The Holdfasts",
        5,
        [
            [50, 55],
            [60, 65],
            [70, 75],
            [90, 95],
            [110, 115],
            [0, 0],
            [0, 0],
        ],
    ),
    (
        "EntratiLabSyndicate",
        "Cavia",
        5,
        [
            [55, 60],
            [65, 70],
            [75, 80],
            [95, 100],
            [115, 120],
            [0, 0],
            [0, 0],
        ],
    ),
    (
        "HexSyndicate",
        "The Hex",
        7,
        [
            [65, 70],
            [75, 80],
            [85, 90],
            [95, 100],
            [105, 110],
            [115, 120],
            [125, 130],
        ],
    ),
];

const CHALLENGE_DESC_PREFIXES: [&str; 3] = [
    "/Lotus/Language/Challenges/Challenge_",
    "/Lotus/Language/EntratiLab/EntratiGeneral/Challenge_",
    "/Lotus/Language/1999Bounties/Challenge_",
];

/// Difficulty suffixes the oracle appends that do not exist in the dict.
const DIFFICULTY_SUFFIXES: [&str; 4] = ["VeryHard", "Hard", "Normal", "Easy"];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

static CAMEL: LazyLock<Regex> = LazyLock::new(|| re(r"([a-z0-9])([A-Z])"));
static LETTER_DIGIT: LazyLock<Regex> = LazyLock::new(|| re(r"([A-Za-z])(\d)"));
static DIGIT_LETTER: LazyLock<Regex> = LazyLock::new(|| re(r"(\d)([A-Za-z])"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s{2,}"));
static DICT_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"<[A-Z0-9_]+>"));
static COLOR: LazyLock<Regex> = LazyLock::new(|| re(r"\|OPEN_COLOR\|[^|]*\|CLOSE_COLOR\|\s*"));
static DT_TAG: LazyLock<Regex> = LazyLock::new(|| re(r"<DT_[A-Z_]+>"));

// ---- value helpers ----

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// JS `Number(x || 0)` for the numeric shapes DE ships.
fn num(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn uint(v: Option<&Value>) -> u32 {
    num(v).max(0.0) as u32
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn ms(v: Option<&Value>) -> i64 {
    num(v
        .and_then(|d| d.get("$date"))
        .and_then(|d| d.get("$numberLong"))) as i64
}

fn iso_ms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn de_date(v: Option<&Value>) -> Option<String> {
    Some(ms(v)).filter(|m| *m != 0).map(iso_ms)
}

fn oid(v: &Value) -> String {
    text(get(v, "_id").unwrap_or(&Value::Null), "$oid").to_owned()
}

/// DE ships these singly or as an array: take the live entry, else the first.
fn active_entry(v: Option<&Value>, now: i64) -> Option<&Value> {
    let entries: &[Value] = match v {
        Some(Value::Array(a)) => a,
        Some(o @ Value::Object(_)) => std::slice::from_ref(o),
        _ => &[],
    };
    entries
        .iter()
        .find(|e| ms(e.get("Expiry")) > now)
        .or_else(|| entries.first())
}

fn first_or_self(v: Option<&Value>) -> Option<&Value> {
    match v {
        Some(Value::Array(a)) => a.first(),
        Some(o @ Value::Object(_)) => Some(o),
        _ => None,
    }
}

// ---- text helpers ----

/// Lowercase, then capitalise each word start (after whitespace or a hyphen).
pub fn title_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut at_start = true;
    for c in value.to_lowercase().chars() {
        if at_start && !c.is_whitespace() {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        at_start = c.is_whitespace() || c == '-';
    }
    out
}

/// Readable tail of a Lotus path: ".../SeasonDailyAimGlide" -> "Season Daily Aim Glide".
/// DE glues durations onto the noun ("Booster3Day"), so digits split off the letters.
pub fn pretty_path_slug(value: &str) -> String {
    let tail = value.rsplit('/').next().unwrap_or(value);
    let tail = tail
        .strip_suffix("StoreItems")
        .or_else(|| tail.strip_suffix("StoreItem"))
        .unwrap_or(tail);
    let s = CAMEL.replace_all(tail, "$1 $2");
    let s = LETTER_DIGIT.replace_all(&s, "$1 $2");
    let s = DIGIT_LETTER.replace_all(&s, "$1 $2");
    SPACES.replace_all(&s, " ").trim().to_owned()
}

/// Store paths mirror inventory identities under an extra StoreItems segment.
pub fn store_item_path(path: &str) -> String {
    path.strip_prefix("/Lotus/StoreItems/")
        .map_or_else(|| path.to_owned(), |rest| format!("/Lotus/{rest}"))
}

fn enum_tail_label(value: &str, prefix: &str) -> String {
    let tail = value.strip_prefix(prefix).unwrap_or(value);
    if tail.is_empty() {
        "Unknown".into()
    } else {
        title_case(&tail.replace('_', " "))
    }
}

fn clean_challenge_text(text: &str, ally: Option<&str>, count: Option<u32>) -> String {
    let count = count.map_or_else(|| "X".to_owned(), |c| c.to_string());
    let s = text
        .replace("|COUNT|", &count)
        .replace("|ALLY|", ally.unwrap_or("Ally"));
    let s = COLOR.replace_all(&s, "");
    DT_TAG
        .replace_all(&s, "")
        .replace('\n', " ")
        .trim()
        .to_owned()
}

/// Dict values embed icon tags such as "<SHARD_BLUE_SIMPLE>" that have no glyph outside the game.
fn strip_dict_tags(value: &str) -> String {
    let s = DICT_TAG.replace_all(value, " ");
    SPACES.replace_all(&s, " ").trim().to_owned()
}

// ---- name resolution ----

impl Ctx {
    /// Resolve a Lotus item path to a display name.
    pub fn item_name(&self, path: &str) -> String {
        let entry = self.items.get(path);
        if let Some(name) = entry.and_then(|e| self.resolve(e.get("name"))) {
            return strip_dict_tags(&name);
        }
        // Recipe fallback: MummyQuestKeyBlueprint -> "Sands of Inaros Blueprint".
        if let Some(result) = entry
            .and_then(|e| e.get("resultType")?.as_str())
            .and_then(|r| self.items.get(r))
            .and_then(|r| self.resolve(r.get("name")))
        {
            return format!("{} Blueprint", strip_dict_tags(&result));
        }
        // Relic fallback: ExportRelics entries have era + category but no name.
        if let Some(e) = entry
            && let (Some(era), Some(cat)) = (
                e.get("era")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty()),
                e.get("category")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty()),
            )
        {
            return format!("{era} {cat} Relic");
        }
        let readable = pretty_path_slug(path);
        // Glyphs are stored as "AvatarImage..." in data.
        match readable.strip_prefix("Avatar Image") {
            Some(rest) => format!("Glyph{rest}").trim().to_owned(),
            None => readable,
        }
    }

    fn baro_icon(&self, path: &str) -> Option<String> {
        let icon = BARO_ICON_OVERRIDES
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, i)| *i)
            .or_else(|| self.items.get(path)?.get("icon")?.as_str())?
            .trim();
        if icon.is_empty() {
            return None;
        }
        let url = if icon.starts_with("http") {
            icon.to_owned()
        } else {
            format!("{BROWSE_WF}{icon}")
        };
        public_image_url(Some(&url))
    }

    /// "Node, System" as fissures and invasions show it.
    fn fissure_node(&self, node_id: &str) -> String {
        if node_id.is_empty() {
            return "Unknown".into();
        }
        let Some(region) = self.region(node_id) else {
            return node_id.to_owned();
        };
        let name = self
            .resolve(region.get("name"))
            .unwrap_or_else(|| node_id.to_owned());
        match self.resolve(region.get("systemName")) {
            Some(system) => format!("{name}, {system}"),
            None => name,
        }
    }

    /// Fissure, sortie, alert and bounty labels all resolve in this order so their casing and
    /// wording never diverge: DE's own MT_ table, the node's mission, the fallback map, the enum tail.
    fn mission_label(
        &self,
        mission_type: &str,
        node_id: &str,
        dict: impl Fn(Option<&Value>) -> Option<String>,
    ) -> String {
        let export = self
            .mission_types
            .get(mission_type)
            .and_then(|m| m.get("name"));
        if let Some(label) = dict(export) {
            return title_case(&label);
        }
        if let Some(label) = dict(self.region(node_id).and_then(|r| r.get("missionName"))) {
            return title_case(&label);
        }
        MISSION_TYPE_LABELS
            .iter()
            .find(|(k, _)| *k == mission_type)
            .map_or_else(
                || enum_tail_label(mission_type, "MT_"),
                |(_, l)| (*l).into(),
            )
    }

    /// English on purpose: saved fissure-alert rules are matched against these.
    fn english_mission(&self, mission_type: &str, node_id: &str) -> String {
        self.mission_label(mission_type, node_id, |v| self.resolve(v))
    }

    /// Sorties, archon hunts and alerts are display-only, so they follow the game language.
    fn localized_mission(&self, mission_type: &str, node_id: &str) -> String {
        self.mission_label(mission_type, node_id, |v| self.localized(v))
    }

    /// Void storms omit MissionType, so use the node's own (railjack) mission name.
    fn railjack_mission(&self, node_id: &str) -> String {
        self.resolve(self.region(node_id).and_then(|r| r.get("missionName")))
            .map_or_else(|| "Railjack".into(), |n| title_case(&n))
    }
}

fn sortie_modifier(modifier: &str) -> String {
    SORTIE_MODIFIER
        .iter()
        .find(|(k, _)| *k == modifier)
        .map_or_else(
            || enum_tail_label(modifier, "SORTIE_MODIFIER_"),
            |(_, l)| (*l).into(),
        )
}

/// Empty rather than "Unknown" so a boss-less payload hides the line.
fn boss_label(boss: &str) -> String {
    if boss.is_empty() {
        String::new()
    } else {
        enum_tail_label(boss, "SORTIE_BOSS_")
    }
}

fn void_tier(modifier: &str) -> Option<&'static str> {
    match modifier.strip_suffix("Hard").unwrap_or(modifier) {
        "VoidT1" => Some("Lith"),
        "VoidT2" => Some("Meso"),
        "VoidT3" => Some("Neo"),
        "VoidT4" => Some("Axi"),
        "VoidT5" => Some("Requiem"),
        "VoidT6" => Some("Omnia"),
        _ => None,
    }
}

// ---- parsing ----

/// Display names of the Circuit's current picks, per category.
pub fn parse_circuit_choices(raw: &Value, now: i64) -> Vec<CircuitChoices> {
    let active = list(raw, "EndlessXpSchedule").iter().find(|e| {
        let (from, to) = (ms(e.get("Activation")), ms(e.get("Expiry")));
        from <= now && (to == 0 || to > now)
    });
    let categories = active
        .map(|a| list(a, "CategoryChoices"))
        .unwrap_or_else(|| list(raw, "EndlessXpChoices"));
    [("EXC_NORMAL", "normal"), ("EXC_HARD", "hard")]
        .into_iter()
        .map(|(tag, category)| CircuitChoices {
            category: category.into(),
            choices: categories
                .iter()
                .find(|e| text(e, "Category") == tag)
                .map(|e| list(e, "Choices"))
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_str)
                .map(|n| CAMEL.replace_all(n, "$1 $2").into_owned())
                .collect(),
        })
        .filter(|c| !c.choices.is_empty())
        .collect()
}

fn parse_nightwave_challenge(raw: &Value, ctx: &Ctx) -> NightwaveChallenge {
    let path = text(raw, "Challenge");
    let entry = ctx.challenges.get(path);
    let required = entry.map_or(0, |e| uint(e.get("requiredCount")));
    let name_key = entry.and_then(|e| e.get("name"));
    let title = ctx
        .localized(name_key)
        .or_else(|| Some(pretty_path_slug(path)).filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "Unknown".into());
    let description = ctx.localized(description_key(name_key).as_ref());
    NightwaveChallenge {
        id: oid(raw),
        name: path.rsplit('/').next().unwrap_or("").to_owned(),
        description: match description {
            Some(d) => clean_challenge_text(&d, None, Some(required)),
            None if entry.is_some() => title.clone(),
            None => String::new(),
        },
        title,
        standing: entry
            .and_then(|e| e.get("standing")?.as_f64())
            .filter(|s| *s >= 0.0)
            .map(|s| s as u32),
        required_count: required,
        is_daily: raw.get("Daily") == Some(&Value::Bool(true)),
        is_elite: path.contains("/WeeklyHard/"),
        activation: de_date(raw.get("Activation")),
        expiry: de_date(raw.get("Expiry")),
    }
}

/// The description key is the name key with its `_Name` tail swapped.
fn description_key(name: Option<&Value>) -> Option<Value> {
    let key = name?.as_str()?;
    Some(Value::String(key.strip_suffix("_Name").map_or_else(
        || key.to_owned(),
        |k| format!("{k}_Description"),
    )))
}

/// A few calendar dict values are shouted; only this row rewrites them, since item names
/// elsewhere are matched against the game and must stay verbatim.
fn calendar_label(value: &str) -> String {
    if value.chars().any(|c| c.is_ascii_alphabetic()) && value == value.to_uppercase() {
        title_case(value)
    } else {
        value.to_owned()
    }
}

fn parse_calendar_event(event: &Value, ctx: &Ctx) -> Option<CalendarEvent> {
    if let Some(challenge) = event
        .get("challenge")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let entry = ctx.challenges.get(challenge);
        let count = entry
            .map(|e| uint(e.get("requiredCount")))
            .filter(|c| *c != 0);
        let name_key = entry.and_then(|e| e.get("name"));
        let Some(name) = ctx.localized(name_key) else {
            return Some(CalendarEvent {
                kind: "challenge".into(),
                label: pretty_path_slug(challenge),
                description: None,
                unique_name: None,
            });
        };
        return Some(CalendarEvent {
            kind: "challenge".into(),
            label: calendar_label(&clean_challenge_text(&name, None, count)),
            description: ctx
                .localized(description_key(name_key).as_ref())
                .map(|d| clean_challenge_text(&d, None, count)),
            unique_name: None,
        });
    }
    if let Some(reward) = event
        .get("reward")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        let unique_name = store_item_path(reward);
        return Some(CalendarEvent {
            kind: "reward".into(),
            label: calendar_label(&ctx.item_name(&unique_name)),
            description: None,
            unique_name: Some(unique_name),
        });
    }
    let upgrade = event
        .get("upgrade")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())?;
    Some(CalendarEvent {
        kind: "upgrade".into(),
        label: pretty_path_slug(upgrade),
        description: None,
        unique_name: None,
    })
}

/// Days with no resolvable event carry nothing worth showing, so they are dropped.
fn parse_calendar_days(days: &[Value], ctx: &Ctx) -> Vec<CalendarDay> {
    days.iter()
        .map(|d| CalendarDay {
            day: uint(d.get("day")),
            events: list(d, "events")
                .iter()
                .filter_map(|e| parse_calendar_event(e, ctx))
                .filter(|e| !e.label.is_empty())
                .collect(),
        })
        .filter(|d| !d.events.is_empty())
        .collect()
}

fn parse_alert(raw: &Value, ctx: &Ctx) -> Alert {
    let empty = Value::Null;
    let info = get(raw, "MissionInfo").unwrap_or(&empty);
    let reward = get(info, "missionReward").unwrap_or(&empty);
    let node_id = text(info, "location");
    let counted = list(reward, "countedItems").iter().map(|c| AlertItem {
        name: ctx.item_name(&store_item_path(text(c, "ItemType"))),
        count: Some(uint(c.get("ItemCount")))
            .filter(|n| *n != 0)
            .unwrap_or(1),
    });
    let plain = list(reward, "items")
        .iter()
        .filter_map(Value::as_str)
        .map(|p| AlertItem {
            name: ctx.item_name(&store_item_path(p)),
            count: 1,
        });
    Alert {
        id: oid(raw),
        activation: de_date(raw.get("Activation")),
        expiry: de_date(raw.get("Expiry")),
        node: ctx.node_label(node_id),
        mission: ctx.localized_mission(text(info, "missionType"), node_id),
        faction: faction_label(info.get("faction").and_then(Value::as_str)),
        min_level: uint(info.get("minEnemyLevel")),
        max_level: uint(info.get("maxEnemyLevel")),
        credits: uint(reward.get("credits")),
        items: counted.chain(plain).collect(),
    }
}

fn parse_fissures(raw: &Value, now: i64, ctx: &Ctx) -> Vec<Fissure> {
    let normal = list(raw, "ActiveMissions").iter().filter_map(|m| {
        let modifier = text(m, "Modifier");
        if !modifier.starts_with("VoidT") {
            return None;
        }
        let tier = void_tier(modifier)?;
        let node_id = Some(text(m, "Node"))
            .filter(|n| !n.is_empty())
            .unwrap_or("Unknown");
        let exp = ms(m.get("Expiry"));
        Some(Fissure {
            expiry: Some(exp).filter(|e| *e != 0).map(iso_ms),
            tier: tier.into(),
            mission_type: ctx.english_mission(text(m, "MissionType"), node_id),
            node: ctx.fissure_node(node_id),
            node_id: node_id.into(),
            is_hard: m.get("Hard") == Some(&Value::Bool(true)) || modifier.ends_with("Hard"),
            is_storm: false,
            expired: exp < now,
        })
    });
    // Void storms (Railjack) use their own array with `ActiveMissionTier` instead of a Modifier.
    let storms = list(raw, "VoidStorms").iter().filter_map(|vs| {
        let tier_raw = text(vs, "ActiveMissionTier");
        let tier = void_tier(tier_raw)?;
        let node_id = Some(text(vs, "Node"))
            .filter(|n| !n.is_empty())
            .unwrap_or("Unknown");
        let exp = ms(vs.get("Expiry"));
        Some(Fissure {
            expiry: Some(exp).filter(|e| *e != 0).map(iso_ms),
            tier: tier.into(),
            mission_type: ctx.railjack_mission(node_id),
            node: ctx.fissure_node(node_id),
            node_id: node_id.into(),
            is_hard: tier_raw.ends_with("Hard"),
            is_storm: true,
            expired: exp < now,
        })
    });
    normal.chain(storms).filter(|f| !f.expired).collect()
}

fn price(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_f64)
        .filter(|p| p.is_finite() && *p >= 0.0)
        .map(|p| p as u32)
}

fn hub_location(node: &str, fallback: &str) -> String {
    HUB_NODE
        .iter()
        .find(|(k, _)| *k == node)
        .map(|(_, v)| (*v).to_owned())
        .or_else(|| Some(node.to_owned()).filter(|n| !n.is_empty()))
        .unwrap_or_else(|| fallback.into())
}

fn parse_void_trader(raw: &Value, ctx: &Ctx) -> Option<Trader> {
    let baro = first_or_self(raw.get("VoidTraders"))?;
    Some(Trader {
        activation: de_date(baro.get("Activation")),
        expiry: de_date(baro.get("Expiry")),
        location: hub_location(text(baro, "Node"), "Unknown"),
        inventory: list(baro, "Manifest")
            .iter()
            .filter(|i| !text(i, "ItemType").contains("BaroTreasureBox"))
            .map(|i| {
                let unique_name = store_item_path(text(i, "ItemType"));
                TraderItem {
                    item: ctx.item_name(&unique_name),
                    ducats: price(i.get("PrimePrice")),
                    credits: price(i.get("RegularPrice")),
                    image_override: ctx.baro_icon(&unique_name),
                    unique_name,
                }
            })
            .collect(),
    })
}

fn parse_vault_trader(raw: &Value) -> Option<Trader> {
    let varzia = first_or_self(raw.get("PrimeVaultTraders"))?;
    Some(Trader {
        activation: de_date(varzia.get("Activation")),
        expiry: de_date(varzia.get("Expiry")),
        location: hub_location(text(varzia, "Node"), "Varzia"),
        inventory: list(varzia, "Manifest")
            .iter()
            .map(|i| TraderItem {
                unique_name: store_item_path(text(i, "ItemType")),
                item: text(i, "ItemType").rsplit('/').next().unwrap_or("").into(),
                ducats: None,
                credits: None,
                image_override: None,
            })
            .collect(),
    })
}

fn parse_raw_bounties(raw: &Value, now: i64) -> Vec<SyndicateBounty> {
    list(raw, "SyndicateMissions")
        .iter()
        .filter_map(|sm| {
            let key = text(sm, "Tag");
            let (_, name) = RAW_BOUNTY_SYNDICATES.iter().find(|(k, _)| *k == key)?;
            let jobs = list(sm, "Jobs");
            if ms(sm.get("Expiry")) <= now || jobs.is_empty() {
                return None;
            }
            Some(SyndicateBounty {
                syndicate: (*name).into(),
                syndicate_key: key.into(),
                expiry: de_date(sm.get("Expiry")),
                jobs: jobs
                    .iter()
                    .filter(|j| !text(j, "jobType").is_empty())
                    .map(|j| {
                        // ".../AttritionBountyExt" -> "Attrition Bounty"
                        let slug = text(j, "jobType").rsplit('/').next().unwrap_or("");
                        let slug = match slug.find("Bounty") {
                            Some(at) => format!("{} Bounty", &slug[..at]),
                            None => slug.to_owned(),
                        };
                        let kind = CAMEL.replace_all(&slug, "$1 $2").trim().to_owned();
                        BountyJob {
                            kind: if kind.is_empty() {
                                "Unknown".into()
                            } else {
                                kind
                            },
                            enemy_levels: [
                                uint(j.get("minEnemyLevel")),
                                uint(j.get("maxEnemyLevel")),
                            ],
                            tier_index: None,
                            standing_stages: list(j, "xpAmounts")
                                .iter()
                                .map(|x| uint(Some(x)))
                                .collect(),
                            min_mr: uint(j.get("masteryReq")),
                            challenge_desc: None,
                        }
                    })
                    .collect(),
            })
        })
        .collect()
}

fn faction_name(faction: &str) -> String {
    match faction {
        "FC_GRINEER" => "Grineer",
        "FC_CORPUS" => "Corpus",
        "FC_INFESTATION" => "Infested",
        "FC_OROKIN" => "Orokin",
        "FC_SENTIENT" => "Sentient",
        other => other,
    }
    .into()
}

fn parse_invasions(raw: &Value, ctx: &Ctx) -> Vec<Invasion> {
    let side = |reward: Option<&Value>, faction: &str| InvasionSide {
        reward: reward.map_or_else(InvasionReward::default, |r| InvasionReward {
            items: vec![],
            counted_items: list(r, "countedItems")
                .iter()
                .map(|c| CountedItem {
                    count: Some(uint(c.get("ItemCount")))
                        .filter(|n| *n != 0)
                        .unwrap_or(1),
                    kind: ctx.item_name(text(c, "ItemType")),
                })
                .collect(),
            credits: uint(r.get("credits")),
        }),
        faction: faction_name(faction),
    };
    list(raw, "Invasions")
        .iter()
        .filter(|inv| inv.get("Completed") != Some(&Value::Bool(true)))
        .map(|inv| {
            let goal = num(inv.get("Goal"));
            let completion = if goal > 0.0 {
                (num(inv.get("Count")) / goal * 1000.0).round() / 10.0
            } else {
                0.0
            };
            Invasion {
                id: oid(inv),
                node: ctx.fissure_node(text(inv, "Node")),
                desc: None,
                attacker: side(inv.get("AttackerReward"), text(inv, "Faction")),
                defender: side(inv.get("DefenderReward"), text(inv, "DefenderFaction")),
                vs_infestation: text(inv, "Faction") == "FC_INFESTATION",
                completion: completion.abs().clamp(0.0, 100.0),
                completed: false,
            }
        })
        .collect()
}

fn parse_daily_deals(raw: &Value, now: i64, ctx: &Ctx) -> Vec<DailyDeal> {
    list(raw, "DailyDeals")
        .iter()
        .filter(|d| ms(d.get("Expiry")) > now)
        .map(|d| {
            let unique_name = store_item_path(text(d, "StoreItem"));
            DailyDeal {
                item: ctx.item_name(&unique_name),
                image_override: ctx.baro_icon(&unique_name),
                discount: uint(d.get("Discount")),
                original_price: uint(d.get("OriginalPrice")),
                sale_price: uint(d.get("SalePrice")),
                total: uint(d.get("AmountTotal")),
                sold: uint(d.get("AmountSold")),
                expiry: de_date(d.get("Expiry")),
                unique_name,
            }
        })
        .collect()
}

/// `None` for anything that is not a JSON object (a failed or empty fetch).
pub fn parse_raw(raw: &Value, now: i64, ctx: &Ctx) -> Option<WorldState> {
    if !raw.is_object() {
        return None;
    }

    let sortie = active_entry(raw.get("Sorties"), now).map(|s| Sortie {
        id: oid(s),
        activation: de_date(s.get("Activation")),
        expiry: de_date(s.get("Expiry")),
        boss: boss_label(text(s, "Boss")),
        missions: list(s, "Variants")
            .iter()
            .map(|v| SortieMission {
                node: ctx.node_label(text(v, "node")),
                mission: ctx.localized_mission(text(v, "missionType"), text(v, "node")),
                modifier: sortie_modifier(text(v, "modifierType")),
            })
            .collect(),
    });

    let archon_hunt = active_entry(raw.get("LiteSorties"), now).map(|s| ArchonHunt {
        id: oid(s),
        activation: de_date(s.get("Activation")),
        expiry: de_date(s.get("Expiry")),
        boss: boss_label(text(s, "Boss")),
        missions: list(s, "Missions")
            .iter()
            .map(|m| ArchonMission {
                node: ctx.node_label(text(m, "node")),
                mission: ctx.localized_mission(text(m, "missionType"), text(m, "node")),
            })
            .collect(),
    });

    let nightwave = raw
        .get("SeasonInfo")
        .filter(|s| s.is_object())
        .map(|s| Nightwave {
            activation: de_date(s.get("Activation")),
            expiry: de_date(s.get("Expiry")),
            season: uint(s.get("Season")),
            phase: uint(s.get("Phase")),
            affiliation_tag: text(s, "AffiliationTag").into(),
            // DE leaves finished acts in ActiveChallenges. A missing expiry keeps the act rather
            // than risking an empty board on a malformed field.
            challenges: list(s, "ActiveChallenges")
                .iter()
                .filter(|c| {
                    let exp = ms(c.get("Expiry"));
                    exp == 0 || exp > now
                })
                .map(|c| parse_nightwave_challenge(c, ctx))
                .collect(),
        });

    let alerts = list(raw, "Alerts")
        .iter()
        .filter(|a| ms(a.get("Expiry")) > now)
        .map(|a| parse_alert(a, ctx))
        .collect();

    let descents = list(raw, "Descents");
    let descent = descents
        .iter()
        .find(|d| ms(d.get("Activation")) <= now && ms(d.get("Expiry")) > now)
        .or_else(|| descents.first());

    let calendar_season = active_entry(raw.get("KnownCalendarSeasons"), now).map(|c| {
        let season = text(c, "Season").trim_start_matches("CST_").to_lowercase();
        let mut chars = season.chars();
        CalendarSeason {
            activation: de_date(c.get("Activation")),
            expiry: de_date(c.get("Expiry")),
            days: parse_calendar_days(list(c, "Days"), ctx),
            season: chars
                .next()
                .map(|f| f.to_uppercase().chain(chars).collect())
                .unwrap_or_default(),
        }
    });

    Some(WorldState {
        fissures: parse_fissures(raw, now, ctx),
        void_trader: parse_void_trader(raw, ctx),
        vault_trader: parse_vault_trader(raw),
        sortie,
        archon_hunt,
        nightwave,
        descents: descent.map(|d| TimeWindow {
            activation: de_date(d.get("Activation")),
            expiry: de_date(d.get("Expiry")),
        }),
        calendar_season,
        alerts,
        steel_path: Some(steel_path::compute(now)),
        duviri_cycle: Some(DuviriCycle {
            state: None,
            next_state: None,
            expiry: descent.and_then(|d| de_date(d.get("Expiry"))),
            choices: parse_circuit_choices(raw, now),
        }),
        invasions: parse_invasions(raw, ctx),
        bounties: parse_raw_bounties(raw, now),
        daily_deals: parse_daily_deals(raw, now, ctx),
        ..WorldState::default()
    })
}

// ---- oracle bounty cycle ----

/// Resolve oracle challenge paths through their language dictionary keys.
fn challenge_desc(path: &str, ally: Option<&str>, ctx: &Ctx) -> Option<String> {
    let slug = path.rsplit('/').next().filter(|s| !s.is_empty())?;
    let count = ctx
        .challenges
        .get(path)
        .map(|c| uint(c.get("requiredCount")))
        .filter(|c| *c != 0);

    // Candidates: the slug, with "Challenge" added, and with the difficulty stripped.
    let mut candidates = vec![];
    let mut add = |s: &str| {
        candidates.push(s.to_owned());
        if !s.ends_with("Challenge") {
            candidates.push(format!("{s}Challenge"));
        }
    };
    add(slug);
    for suffix in DIFFICULTY_SUFFIXES {
        if let Some(stem) = slug.strip_suffix(suffix) {
            add(stem);
            break;
        }
        // EntratiLabKillMurmurEasyChallenge -> EntratiLabKillMurmurChallenge
        if let Some(stem) = slug.strip_suffix(&format!("{suffix}Challenge")) {
            add(&format!("{stem}Challenge"));
            break;
        }
    }
    candidates.iter().find_map(|candidate| {
        CHALLENGE_DESC_PREFIXES.iter().find_map(|prefix| {
            let desc = ctx
                .dict
                .get(&format!("{prefix}{candidate}_Desc"))?
                .as_str()?;
            (!desc.is_empty()).then(|| clean_challenge_text(desc, ally, count))
        })
    })
}

/// `.../QuincyAllyAgent` -> `Quincy`
fn ally_name(path: Option<&str>) -> Option<String> {
    let slug = path?.rsplit('/').next()?;
    Some(slug.strip_suffix("AllyAgent").unwrap_or(slug).to_owned()).filter(|s| !s.is_empty())
}

pub fn parse_bounty_cycle(data: &Value, ctx: &Ctx) -> Vec<SyndicateBounty> {
    let Some(bounties) = data.get("bounties").and_then(Value::as_object) else {
        return vec![];
    };
    let expiry = data
        .get("expiry")
        .and_then(Value::as_f64)
        .map(|e| iso_ms(e as i64));
    let mut out = vec![];
    for (key, jobs) in bounties {
        let Some((_, display, tiers, levels)) =
            BOUNTY_CYCLE_SYNDICATES.iter().find(|(k, ..)| k == key)
        else {
            continue;
        };
        let Some(jobs) = jobs.as_array().filter(|j| !j.is_empty()) else {
            continue;
        };
        let parsed = jobs
            .iter()
            .enumerate()
            .map(|(index, job)| {
                let node = text(job, "node");
                let region = ctx.region(node);
                let kind = region
                    .and_then(|r| r.get("missionType")?.as_str())
                    .map_or_else(|| "Unknown".into(), |mt| ctx.english_mission(mt, node));
                let enemy_levels = if index < *tiers {
                    levels[index]
                } else {
                    [
                        uint(region.and_then(|r| r.get("minEnemyLevel"))),
                        uint(region.and_then(|r| r.get("maxEnemyLevel"))),
                    ]
                };
                // Oracle bounties are single-stage; the tier pair is [base, bonus], not per-stage.
                let ally = ally_name(job.get("ally").and_then(Value::as_str));
                BountyJob {
                    kind,
                    enemy_levels,
                    tier_index: Some(index as u32),
                    standing_stages: STANDING_TIERS
                        .get(index)
                        .filter(|_| index < *tiers)
                        .map(|t| t[0])
                        .into_iter()
                        .collect(),
                    min_mr: 0,
                    challenge_desc: job
                        .get("challenge")
                        .and_then(Value::as_str)
                        .and_then(|c| challenge_desc(c, ally.as_deref(), ctx)),
                }
            })
            .collect();
        out.push(SyndicateBounty {
            syndicate: (*display).into(),
            syndicate_key: key.clone(),
            expiry: expiry.clone(),
            jobs: parsed,
        });
    }
    out
}

#[cfg(test)]
mod tests;
