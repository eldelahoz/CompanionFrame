//! Node, mission and faction naming from DE's own region table and language dictionaries,
//! plus the export tables the parser resolves item and challenge paths through.

use serde_json::{Map, Value};

use crate::services::bundled_data::BundledData;
use crate::services::game_locale::GameLocale;

use super::parser::title_case;

/// Only Infestation and Orokin differ from the `FC_` tail fallback; in game they read as
/// Infested and Corrupted.
const FACTION_LABELS: [(&str, &str); 6] = [
    ("FC_GRINEER", "Grineer"),
    ("FC_CORPUS", "Corpus"),
    ("FC_INFESTATION", "Infested"),
    ("FC_OROKIN", "Corrupted"),
    ("FC_SENTIENT", "Sentient"),
    ("FC_NARMER", "Narmer"),
];

/// Tables that name items; later tables win on a shared path, like WFHelper's merge order.
const ITEM_TABLES: [&str; 9] = [
    "ExportResources",
    "ExportRecipes",
    "ExportUpgrades",
    "ExportGear",
    "ExportRelics",
    "ExportKeys",
    "ExportWeapons",
    "ExportWarframes",
    "ExportSentinels",
];

#[derive(Default)]
pub struct Ctx {
    pub regions: Map<String, Value>,
    /// English dictionary: `/Lotus/Language/...` key to text.
    pub dict: Map<String, Value>,
    pub locale: GameLocale,
    pub challenges: Map<String, Value>,
    pub mission_types: Map<String, Value>,
    pub items: Map<String, Value>,
}

impl Ctx {
    /// Missing tables stay empty: names then degrade to path slugs instead of failing the parse.
    pub fn load(bundled: &BundledData, locale: &str) -> Self {
        let table = |name: &str| {
            bundled.export(name).unwrap_or_else(|| {
                tracing::warn!("world state: {name} missing from the bundled package");
                Map::new()
            })
        };
        let mut items = Map::new();
        for name in ITEM_TABLES {
            items.extend(table(name));
        }
        Self {
            regions: table("ExportRegions"),
            dict: bundled.dict("en").unwrap_or_default(),
            locale: GameLocale::load(locale, bundled),
            challenges: table("ExportChallenges"),
            mission_types: table("ExportMissionTypes"),
            items,
        }
    }

    /// Dictionary lookup; a plain (non key) string passes through unchanged.
    pub fn resolve(&self, value: Option<&Value>) -> Option<String> {
        let s = value?.as_str().filter(|s| !s.is_empty())?;
        if !s.starts_with('/') {
            return Some(s.to_owned());
        }
        self.dict
            .get(s)
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    }

    /// Same lookup in the player's game language, with English filling every hole.
    pub fn localized(&self, value: Option<&Value>) -> Option<String> {
        let s = value?.as_str().filter(|s| !s.is_empty())?;
        if !s.starts_with('/') {
            return Some(s.to_owned());
        }
        let local = self.locale.localize(Some(s), "");
        if !local.is_empty() {
            return Some(local);
        }
        self.resolve(value)
    }

    pub fn region(&self, node_id: &str) -> Option<&Value> {
        self.regions.get(node_id)
    }

    /// "Pacific (Earth)", or the raw id when DE has no entry for the node.
    pub fn node_label(&self, node_id: &str) -> String {
        if node_id.is_empty() {
            return "Unknown".into();
        }
        let region = self.region(node_id);
        let name = self
            .resolve(region.and_then(|r| r.get("name")))
            .unwrap_or_else(|| node_id.to_owned());
        match self.resolve(region.and_then(|r| r.get("systemName"))) {
            Some(system) => format!("{name} ({system})"),
            None => name,
        }
    }
}

pub fn faction_label(faction: Option<&str>) -> String {
    let fc = faction.unwrap_or("");
    if let Some((_, label)) = FACTION_LABELS.iter().find(|(k, _)| *k == fc) {
        return (*label).into();
    }
    match fc.strip_prefix("FC_") {
        Some(tail) => title_case(&tail.replace('_', " ")),
        None => "Unknown".into(),
    }
}

#[cfg(test)]
pub(crate) mod test_ctx {
    use super::*;

    /// The real bundled tables (`pnpm game-data`); tests that need names skip without them.
    pub fn bundled() -> Option<Ctx> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/game-data");
        let ctx = Ctx::load(&BundledData::new(root), "en");
        (!ctx.regions.is_empty()).then_some(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faction_labels() {
        assert_eq!(faction_label(Some("FC_OROKIN")), "Corrupted");
        assert_eq!(faction_label(Some("FC_MADE_UP")), "Made Up");
        assert_eq!(faction_label(None), "Unknown");
    }

    #[test]
    fn resolve_passes_plain_text_and_looks_up_keys() {
        let mut ctx = Ctx::default();
        ctx.dict
            .insert("/Lotus/Language/X".into(), Value::String("Pacific".into()));
        assert_eq!(ctx.resolve(Some(&"Plain".into())).as_deref(), Some("Plain"));
        assert_eq!(
            ctx.resolve(Some(&"/Lotus/Language/X".into())).as_deref(),
            Some("Pacific")
        );
        assert_eq!(ctx.resolve(Some(&"/Lotus/Language/Y".into())), None);
        assert_eq!(ctx.node_label("Nope"), "Nope");
        assert_eq!(ctx.node_label(""), "Unknown");
    }
}
