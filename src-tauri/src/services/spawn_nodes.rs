//! Star-chart nodes as enemy spawn locations. DE's region table names each node's planet,
//! mission, faction and level range; its level path names the tileset. Port of WFHelper's
//! `services/spawnNodes.ts`.

use std::sync::Mutex;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::services::bundled_data::BundledData;
use crate::services::game_locale::GameLocale;
use crate::services::item_db::ItemDbState;
use crate::services::world_state::parser::title_case;

/// One star-chart node as an enemy spawn location. Display fields follow the game language;
/// the English twins are what the codex spawn lists are matched on.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SpawnNode {
    /// Star chart node id, e.g. "SolNode162".
    pub node_id: String,
    pub node: String,
    pub node_en: String,
    pub planet: String,
    pub planet_en: String,
    /// DE star-chart order; a Proxima shares the index of its planet.
    pub planet_index: u32,
    /// DE mission enum, e.g. "MT_CAPTURE".
    pub mission_key: String,
    pub mission: String,
    pub mission_en: String,
    /// Codex faction keys ("infestation"), the node's own faction first.
    pub faction_keys: Vec<String>,
    pub min_level: u32,
    pub max_level: u32,
    /// Wiki tileset name, e.g. "Infested Ship".
    pub tile_set: String,
    pub dark_sector: bool,
}

/// The codex partition of factions. Tenno, Duviri and unaffiliated nodes have no codex faction.
pub const REGION_FACTION_KEYS: [(&str, &str); 8] = [
    ("FC_GRINEER", "grineer"),
    ("FC_CORPUS", "corpus"),
    ("FC_INFESTATION", "infestation"),
    ("FC_OROKIN", "orokin"),
    ("FC_SENTIENT", "sentient"),
    ("FC_MITW", "themurmur"),
    ("FC_TECHROT", "techrot"),
    ("FC_SCALDRA", "scaldra"),
];

const LEVEL_ROOT: &str = "/Lotus/Levels/";

/// Level path under `LEVEL_ROOT` to the wiki tileset name; the longest prefix wins. `None` marks
/// levels that hold no enemy spawns the codex lists: hubs, relays, conclave, quest and event levels.
const LEVEL_TILE_SETS: &[(&str, Option<&str>)] = &[
    ("Proc/Corpus/CorpusGasBoss", Some("Corpus Gas City")),
    ("Proc/Corpus/CorpusGasCity", Some("Corpus Gas City")),
    ("Proc/Corpus/CorpusIcePlanet", Some("Corpus Ice Planet")),
    ("Proc/Corpus/CorpusOutpost", Some("Corpus Outpost")),
    ("Proc/Corpus/CorpusShip", Some("Corpus Ship")),
    (
        "Proc/EntratiLab/EntratiLab",
        Some("Albrecht's Laboratories"),
    ),
    (
        "Proc/EntratiLab/EntratiVoidVault",
        Some("Albrecht's Laboratories"),
    ),
    ("Proc/EntratiLab/EntratiSecretLabNecralisk", None),
    ("Proc/Grineer/GrineerAsteroid", Some("Grineer Asteroid")),
    ("Proc/Grineer/GrineerForest", Some("Grineer Forest")),
    (
        "Proc/Grineer/GrineerFortress",
        Some("Grineer Asteroid Fortress"),
    ),
    ("Proc/Grineer/GrineerGalleon", Some("Grineer Galleon")),
    ("Proc/Grineer/GrineerOcean", Some("Grineer Sealab")),
    ("Proc/Grineer/GrineerSettlement", Some("Grineer Settlement")),
    ("Proc/Grineer/GrineerShipyards", Some("Grineer Shipyard")),
    ("Proc/Infestation/InfestedCorpusShip", Some("Infested Ship")),
    (
        "Proc/Infestation/InfestedMicroplanetLandscape",
        Some("Cambion Drift"),
    ),
    ("Proc/Orokin/OrokinMoon", Some("Orokin Moon")),
    ("Proc/Orokin/OrokinTower", Some("Orokin Tower")),
    ("Proc/Orokin/OrokinTowerDerelict", Some("Orokin Derelict")),
    ("Proc/Zariman/Zariman", Some("Zariman (Tileset)")),
    ("Proc/Zariman/ZarimanHub", None),
    (
        "Proc/CivilianHubs/EidolonLandscape",
        Some("Plains of Eidolon"),
    ),
    ("Proc/CivilianHubs/VenusLandscape", Some("Orb Vallis")),
    ("Proc/CivilianHubs/OstronTown", None),
    ("Proc/CivilianHubs/SolarisUnitedTown", None),
    ("Proc/Space/SpaceGrineer", Some("Free Space")),
    ("SpaceBattles/TR", Some("Corpus Ship (Archwing)")),
    ("Railjack/Proc/", Some("Free Space")),
    ("KelaArenas/OceanArena", Some("Grineer Sealab")),
    ("KelaArenas/ShipyardsArena", Some("Grineer Shipyard")),
    ("Proc/JadeShadows/JSStalkerCaveHub", Some("Stalker's Lair")),
    ("Proc/TauPrequel/", Some("Perita")),
    // The path names no tileset. DE's tileset field says Corpus Outpost, which the wiki places
    // on no Mars node; its node table says Grineer Settlement.
    ("Proc/LastWish/LastWishDefense", Some("Grineer Settlement")),
    ("Proc/Hub/", None),
    ("Proc/Duviri/", None),
    ("Proc/Vania/", Some("H\u{f6}llvania")),
    ("1999/Railjack/", None),
    ("DarkSectors/", None),
    ("DevilTower/", None),
    ("FiveFates/", None),
    ("HarrowQuest/", None),
    ("Muse/Proc/", None),
    ("NokkoColony/Proc/", None),
    ("PVP/", None),
];

/// Nodes DE ships without a level path, plus per-node overrides of the path map.
const NODE_TILE_SETS: &[(&str, Option<&str>)] = &[
    ("SolNode701", Some("Infested Ship")),
    ("SolNode705", Some("Infested Ship")),
    ("CrewBattleNode560", Some("Free Space")),
    ("CrewBattleNode561", Some("Free Space")),
    // The visible Index node stands in for the three hidden risk tiers, which DE names in capitals.
    ("EventNode763", Some("The Index")),
    ("SolNode761", None),
    ("SolNode762", None),
    ("SolNode763", None),
    ("JadeShadows2HUB", None),
];

/// Wiki tileset of a node: `Some(None)` for a level that holds no codex spawns, and `None` for
/// one neither table knows yet.
pub fn region_tile_set(
    node_id: &str,
    level_override: Option<&str>,
) -> Option<Option<&'static str>> {
    if let Some((_, set)) = NODE_TILE_SETS.iter().find(|(id, _)| *id == node_id) {
        return Some(*set);
    }
    let level = level_override?.strip_prefix(LEVEL_ROOT)?;
    LEVEL_TILE_SETS
        .iter()
        .filter(|(prefix, _)| level.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map(|(_, set)| *set)
}

fn faction_keys(factions: [Option<&str>; 2]) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for f in factions.into_iter().flatten() {
        if let Some((_, key)) = REGION_FACTION_KEYS.iter().find(|(fc, _)| *fc == f)
            && !keys.iter().any(|k| k == key)
        {
            keys.push((*key).to_owned());
        }
    }
    keys
}

/// Dictionary lookup; a plain (non key) string passes through unchanged.
fn resolve_dict(dict: &Map<String, Value>, value: Option<&Value>) -> Option<String> {
    let v = value?.as_str().filter(|s| !s.is_empty())?;
    if !v.starts_with('/') {
        return Some(v.to_owned());
    }
    dict.get(v)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Same lookup in the player's game language, with English filling every hole.
fn localized(
    dict: &Map<String, Value>,
    locale: &GameLocale,
    value: Option<&Value>,
) -> Option<String> {
    let v = value?.as_str().filter(|s| !s.is_empty())?;
    if !v.starts_with('/') {
        return Some(v.to_owned());
    }
    let en = resolve_dict(dict, value);
    let l = locale.localize(Some(v), en.as_deref().unwrap_or(""));
    (!l.is_empty()).then_some(l)
}

fn mission_label(
    dict: &Map<String, Value>,
    name: Option<&Value>,
    mission_type: Option<&str>,
) -> String {
    // Per-node mission name from DE's own data (dictionary values are uppercase, e.g.
    // "INFESTED SALVAGE"); truer than a hand-kept MT_ map.
    if let Some(resolved) = resolve_dict(dict, name) {
        return title_case(&resolved);
    }
    match mission_type.and_then(|m| m.strip_prefix("MT_")) {
        Some(mt) => title_case(&mt.replace('_', " ")),
        None => "Unknown".into(),
    }
}

/// Builds the catalog from DE's `ExportRegions` and English dictionary.
pub fn build(
    regions: &Map<String, Value>,
    dict: &Map<String, Value>,
    locale: &GameLocale,
) -> Vec<SpawnNode> {
    let mut rows = Vec::new();
    let mut unknown: Vec<&str> = Vec::new();
    for (node_id, region) in regions {
        let level = region.get("levelOverride").and_then(Value::as_str);
        let tile_set = match region_tile_set(node_id, level) {
            None => {
                unknown.push(node_id);
                continue;
            }
            Some(None) => continue,
            Some(Some(t)) => t,
        };
        let faction = |k: &str| region.get(k).and_then(Value::as_str);
        let factions = faction_keys([faction("faction"), faction("secondaryFaction")]);
        let node_en = resolve_dict(dict, region.get("name"));
        let planet_en = resolve_dict(dict, region.get("systemName"));
        let num = |k: &str| region.get(k).and_then(Value::as_u64).map(|n| n as u32);
        let (Some(node_en), Some(planet_en)) = (node_en, planet_en) else {
            continue;
        };
        let (Some(planet_index), Some(min_level), Some(max_level)) = (
            num("systemIndex"),
            num("minEnemyLevel"),
            num("maxEnemyLevel"),
        ) else {
            continue;
        };
        if factions.is_empty() {
            continue;
        }
        let mission_type = region.get("missionType").and_then(Value::as_str);
        let mission_name = region.get("missionName");
        // A localized name is a plain string, which `mission_label` passes through.
        let local_name = localized(dict, locale, mission_name).map(Value::String);
        rows.push(SpawnNode {
            node_id: node_id.clone(),
            node: localized(dict, locale, region.get("name")).unwrap_or_else(|| node_en.clone()),
            node_en,
            planet: localized(dict, locale, region.get("systemName"))
                .unwrap_or_else(|| planet_en.clone()),
            planet_en,
            planet_index,
            mission_key: mission_type.unwrap_or("").to_owned(),
            mission: mission_label(dict, local_name.as_ref().or(mission_name), mission_type),
            mission_en: mission_label(dict, mission_name, mission_type),
            faction_keys: factions,
            min_level,
            max_level,
            tile_set: tile_set.to_owned(),
            dark_sector: region.get("darkSectorData").is_some(),
        });
    }
    if !unknown.is_empty() {
        tracing::warn!(
            "no tileset known for {} nodes: {}",
            unknown.len(),
            unknown.join(", ")
        );
    }
    rows
}

/// The catalog in the current game language, rebuilt when it changes.
#[derive(Default)]
pub struct SpawnNodes(Mutex<Option<(&'static str, Vec<SpawnNode>)>>);

#[tauri::command]
#[specta::specta]
pub fn spawn_nodes_get(
    app: tauri::AppHandle,
    items: tauri::State<ItemDbState>,
    cache: tauri::State<SpawnNodes>,
) -> Result<Vec<SpawnNode>, String> {
    let (_, locale) = items.ui_snapshot();
    let mut cached = cache.0.lock().unwrap();
    if let Some((code, nodes)) = &*cached
        && *code == locale.code()
    {
        return Ok(nodes.clone());
    }
    let bundled = BundledData::from_app(&app)?;
    let (regions, dict) = (
        bundled
            .export("ExportRegions")
            .ok_or("ExportRegions not bundled")?,
        bundled.dict("en").ok_or("English dictionary not bundled")?,
    );
    let nodes = build(&regions, &dict, &locale);
    *cached = Some((locale.code(), nodes.clone()));
    Ok(nodes)
}

#[cfg(test)]
mod tests;
