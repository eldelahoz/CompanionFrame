//! Game-language item names. DE ships a name dictionary per client language, so localized names
//! need no human translation. English stays canonical everywhere inside the backend: it is the
//! join key for warframe.market, OCR matching and every by-name lookup.

use std::collections::HashMap;

use crate::services::bundled_data::BundledData;

pub const GAME_LOCALES: [&str; 15] = [
    "en", "de", "es", "fr", "it", "ja", "ko", "pl", "pt", "ru", "tc", "th", "tr", "uk", "zh",
];
pub const DEFAULT_GAME_LOCALE: &str = "en";
/// The `game_language` setting that follows EE.log's `-language:`.
pub const AUTO: &str = "auto";

/// The game language in use: an explicit setting wins, `auto` takes EE.log's, else English.
pub fn effective<'a>(setting: &'a str, detected: Option<&'a str>) -> &'a str {
    if setting == AUTO {
        detected.unwrap_or(DEFAULT_GAME_LOCALE)
    } else {
        setting
    }
}

/// The app languages the frontend ships; item names follow these, not the game language.
const UI_LOCALES: [&str; 5] = ["en", "de", "es", "fr", "zh"];

/// The name table for an app `language` setting; anything else reads English.
pub fn ui_code(language: &str) -> &'static str {
    UI_LOCALES
        .iter()
        .find(|c| **c == language)
        .copied()
        .unwrap_or(DEFAULT_GAME_LOCALE)
}

/// A `game_language` value the settings may hold.
pub fn is_setting(value: &str) -> bool {
    value == AUTO || GAME_LOCALES.contains(&value)
}

#[derive(Clone)]
pub struct GameLocale {
    code: &'static str,
    names: HashMap<String, String>,
}

impl Default for GameLocale {
    fn default() -> Self {
        Self {
            code: DEFAULT_GAME_LOCALE,
            names: HashMap::new(),
        }
    }
}

impl GameLocale {
    /// Unknown codes fall back to English, like WFHelper's `setGameLocale`.
    pub fn new(code: &str, names: HashMap<String, String>) -> Self {
        let code = GAME_LOCALES
            .iter()
            .find(|c| **c == code)
            .copied()
            .unwrap_or(DEFAULT_GAME_LOCALE);
        Self { code, names }
    }

    /// Loads `dict.<code>.json` from the bundle; a locale the app does not ship localizes nothing.
    pub fn load(code: &str, bundled: &BundledData) -> Self {
        let names = (code != DEFAULT_GAME_LOCALE)
            .then(|| bundled.dict(code))
            .flatten()
            .map(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| Some((k, v.as_str()?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        let locale = Self::new(code, names);
        if locale.names.is_empty() && locale.code != DEFAULT_GAME_LOCALE {
            tracing::warn!("{code}: no name table found, falling back to English");
        }
        locale
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    /// True while the active locale can actually change a name.
    pub fn is_localizing(&self) -> bool {
        self.code != DEFAULT_GAME_LOCALE && !self.names.is_empty()
    }

    /// Localized name for a `/Lotus/Language/...` key, or the English fallback.
    /// A locale may be partial, so every hole resolves to English.
    pub fn localize(&self, key: Option<&str>, fallback: &str) -> String {
        key.and_then(|k| self.names.get(k))
            .filter(|s| !s.is_empty())
            .map_or_else(|| fallback.to_owned(), Clone::clone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn de() -> GameLocale {
        GameLocale::new(
            "de",
            HashMap::from([("/Lotus/Language/Mods/Serration".into(), "Einkerbung".into())]),
        )
    }

    #[test]
    fn auto_follows_the_log_and_an_explicit_choice_wins() {
        assert_eq!(effective(AUTO, Some("fr")), "fr");
        assert_eq!(effective(AUTO, None), "en");
        assert_eq!(effective("de", Some("fr")), "de");
        assert!(is_setting("auto") && is_setting("es") && !is_setting("xx"));
    }

    #[test]
    fn app_language_maps_to_a_shipped_table_or_english() {
        assert_eq!(ui_code("es"), "es");
        assert_eq!(ui_code("zh"), "zh");
        // Game-only languages and junk read English.
        assert_eq!(ui_code("ja"), "en");
        assert_eq!(ui_code(""), "en");
    }

    #[test]
    fn english_never_localizes() {
        let en = GameLocale::default();
        assert!(!en.is_localizing());
        assert_eq!(
            en.localize(Some("/Lotus/Language/Mods/Serration"), "Serration"),
            "Serration"
        );
    }

    #[test]
    fn unknown_code_falls_back_to_english() {
        let l = GameLocale::new("xx", HashMap::from([("k".into(), "v".into())]));
        assert_eq!(l.code(), "en");
        assert!(!l.is_localizing());
    }

    #[test]
    fn partial_locale_resolves_holes_to_english() {
        let l = de();
        assert!(l.is_localizing());
        assert_eq!(
            l.localize(Some("/Lotus/Language/Mods/Serration"), "Serration"),
            "Einkerbung"
        );
        assert_eq!(
            l.localize(Some("/Lotus/Language/Missing"), "Vitality"),
            "Vitality"
        );
        assert_eq!(l.localize(None, "Vitality"), "Vitality");
    }

    #[test]
    fn locale_without_names_does_not_localize() {
        assert!(!GameLocale::new("de", HashMap::new()).is_localizing());
    }
}
