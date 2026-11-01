//! warframe.market v2 data models (wfm-docs `data-models.mdx`: Item, Riven Item), the fields we read.
//! Optional fields are `Option`/default because the docs flag them "optional/contextual".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct I18n {
    #[serde(default)]
    pub name: String,
    pub icon: Option<String>,
    pub thumb: Option<String>,
    pub sub_icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub slug: String,
    pub game_ref: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub set_root: Option<bool>,
    pub set_parts: Option<Vec<String>>,
    pub quantity_in_set: Option<u32>,
    pub rarity: Option<String>,
    pub bulk_tradable: Option<bool>,
    #[serde(default)]
    pub subtypes: Vec<String>,
    pub max_rank: Option<u32>,
    pub max_charges: Option<u32>,
    pub max_amber_stars: Option<u32>,
    pub max_cyan_stars: Option<u32>,
    pub base_endo: Option<u32>,
    pub endo_multiplier: Option<f64>,
    pub ducats: Option<u32>,
    pub vosfor: Option<u32>,
    pub req_mastery_rank: Option<u32>,
    pub vaulted: Option<bool>,
    pub trading_tax: Option<u32>,
    pub tradable: Option<bool>,
    #[serde(default)]
    pub i18n: BTreeMap<String, I18n>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenItem {
    pub id: String,
    pub slug: String,
    pub game_ref: Option<String>,
    pub group: Option<String>,
    pub riven_type: Option<String>,
    pub disposition: Option<f64>,
    pub req_mastery_rank: Option<u32>,
    #[serde(default)]
    pub i18n: BTreeMap<String, I18n>,
}

pub fn title_from_slug(slug: &str) -> String {
    let mut cap = true;
    slug.chars()
        .map(|c| {
            let c = if c == '_' { ' ' } else { c };
            let out = if cap { c.to_ascii_uppercase() } else { c };
            cap = c == ' ';
            out
        })
        .collect()
}

const ASSET_BASE: &str = "https://warframe.market/static/assets/";

/// Relative asset path -> absolute URL; absolute ones pass through.
pub fn asset_url(path: &str) -> Option<String> {
    let p = path.trim();
    match p {
        "" => None,
        _ if p.starts_with("http://") || p.starts_with("https://") => Some(p.to_owned()),
        _ => Some(format!("{ASSET_BASE}{p}")),
    }
}

fn en(i18n: &BTreeMap<String, I18n>) -> Option<&I18n> {
    i18n.get("en")
}

fn name_of(i18n: &BTreeMap<String, I18n>, slug: &str) -> String {
    en(i18n)
        .map(|e| e.name.trim())
        .filter(|n| !n.is_empty())
        .map_or_else(|| title_from_slug(slug), str::to_owned)
}

/// What the front end indexes: one row per tradable item, English name, absolute asset URLs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub thumb: Option<String>,
    pub icon: Option<String>,
    pub max_rank: Option<u32>,
    pub game_ref: Option<String>,
}

impl From<&Item> for CatalogItem {
    fn from(i: &Item) -> Self {
        let e = en(&i.i18n);
        Self {
            id: i.id.clone(),
            slug: i.slug.clone(),
            name: name_of(&i.i18n, &i.slug),
            thumb: e.and_then(|e| e.thumb.as_deref()).and_then(asset_url),
            icon: e.and_then(|e| e.icon.as_deref()).and_then(asset_url),
            max_rank: i.max_rank.filter(|r| *r > 0),
            game_ref: i.game_ref.clone().filter(|g| !g.trim().is_empty()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenWeapon {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub group: Option<String>,
    pub riven_type: Option<String>,
    pub disposition: Option<f64>,
    pub req_mastery_rank: Option<u32>,
    pub thumb: Option<String>,
    pub game_ref: Option<String>,
}

impl From<&RivenItem> for RivenWeapon {
    fn from(i: &RivenItem) -> Self {
        Self {
            id: i.id.clone(),
            slug: i.slug.clone(),
            name: name_of(&i.i18n, &i.slug),
            group: i.group.clone(),
            riven_type: i.riven_type.clone(),
            disposition: i.disposition,
            req_mastery_rank: i.req_mastery_rank,
            thumb: en(&i.i18n)
                .and_then(|e| e.thumb.as_deref())
                .and_then(asset_url),
            game_ref: i.game_ref.clone(),
        }
    }
}
