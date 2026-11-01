//! Relic era words read off the fissure / inventory screens. Ported from `rewardScannerMatch.ts`.

use std::collections::HashSet;
use std::sync::RwLock;

use super::fold::{fold_accents, levenshtein, normalize_for_ocr, normalize_ocr_phrase};

const ERAS: [&str; 5] = ["lith", "meso", "neo", "axi", "requiem"];

/// Dictionary keys of the era words the screens print, with the English word they stand for.
/// The bundled dictionaries carry no "All" filter-tab label, so that one stays English.
const WORD_KEYS: [(&str, &str); 7] = [
    ("/Lotus/Language/Relics/Era_LITH", "LITH"),
    ("/Lotus/Language/Relics/Era_MESO", "MESO"),
    ("/Lotus/Language/Relics/Era_NEO", "NEO"),
    ("/Lotus/Language/Relics/Era_AXI", "AXI"),
    ("/Lotus/Language/Relics/Era_REQUIEM", "REQUIEM"),
    ("/Lotus/Language/Relics/Era_OMNI", "OMNIA"),
    ("/Lotus/Language/Alerts/AlertRewardCategoryRelic", "RELIC"),
];

/// The "all eras" filter tab label per game language; the dictionaries have no key for it.
/// shortcut: only fr is verified from a real capture; de/es/zh are the game's usual words, check
/// them against a capture. The tab is a single word, so every language is tried whatever the locale.
const FILTER_ALL_WORDS: [(&str, &str); 4] = [
    ("fr", "TOUT"),
    ("de", "ALLE"),
    ("es", "TODO"),
    ("zh", "全部"),
];

/// The game language's era words ("古纪", "Relique"), rewritten to English before detection.
#[derive(Default)]
pub struct EraWords {
    /// (folded uppercase word, English word), longest first.
    words: Vec<(String, &'static str)>,
}

impl EraWords {
    /// `lookup` is the game language's dictionary.
    pub fn build(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let mut words: Vec<(String, &'static str)> = WORD_KEYS
            .iter()
            .filter_map(|(k, en)| {
                let w = fold_accents(&lookup(k)?).to_uppercase();
                (!w.trim().is_empty() && w != *en).then_some((w, *en))
            })
            .collect();
        words.sort_by_key(|(w, _)| std::cmp::Reverse(w.len()));
        Self { words }
    }

    /// Uppercase, accents folded (an English recogniser reads "Réquiem" as "Rèquiem" or
    /// "Requiem"), localized words as English. Latin words match whole; CJK has no word gaps.
    pub fn to_english(&self, text: &str) -> String {
        let mut text = fold_accents(text).to_uppercase();
        for (word, en) in &self.words {
            let mut out = String::with_capacity(text.len());
            let mut last = 0;
            for (at, _) in text.match_indices(word.as_str()) {
                let end = at + word.len();
                let alnum = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
                if at < last
                    || alnum(text[..at].chars().next_back()) && word.is_ascii()
                    || alnum(text[end..].chars().next()) && word.is_ascii()
                {
                    continue;
                }
                out.push_str(&text[last..at]);
                out.push_str(&format!(" {en} "));
                last = end;
            }
            out.push_str(&text[last..]);
            text = out;
        }
        text
    }
}

static WORDS: RwLock<EraWords> = RwLock::new(EraWords { words: Vec::new() });

/// Era words of the game language; called whenever the game language changes.
pub fn set_words(words: EraWords) {
    *WORDS.write().unwrap() = words;
}

fn english(text: &str) -> String {
    WORDS.read().unwrap().to_english(text)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EraHit {
    pub era: Option<&'static str>,
    pub confidence: f64,
}

const NONE: EraHit = EraHit {
    era: None,
    confidence: 0.0,
};

/// The one acceptance rule for an era word: exact, either-way prefix, or one edit. The detector
/// and the ambiguity guard share it, or an era read through a misread ("LITK") stays invisible
/// to the guard.
fn match_era_token(word: &str) -> Option<(&'static str, f64)> {
    let upper = |e: &str| e.to_uppercase();
    if let Some(e) = ERAS.iter().find(|e| upper(e) == word) {
        return Some((e, 1.0));
    }
    if word.len() < 3 {
        return None;
    }
    if let Some(e) = ERAS
        .iter()
        .find(|e| upper(e).starts_with(word) || word.starts_with(&upper(e)))
    {
        return Some((e, 0.82));
    }
    ERAS.iter()
        .find(|e| levenshtein(word, &upper(e)) <= 1)
        .map(|e| (*e, 0.76))
}

/// Era words mentioned as whole words, so a screen listing several fissures reads as ambiguous.
fn distinct_era_mentions(normalized: &str) -> usize {
    let mut found = HashSet::new();
    for raw in normalized.split(' ') {
        let word = normalize_for_ocr(raw);
        if word == "OMNIA" {
            found.insert("omnia");
        } else if let Some((era, _)) = match_era_token(&word) {
            found.insert(era);
        }
    }
    found.len()
}

fn detect_from_text(text: &str) -> EraHit {
    let mut best = NONE;
    for word in text
        .split(' ')
        .map(normalize_for_ocr)
        .filter(|w| w.len() >= 2)
    {
        let Some((era, confidence)) = match_era_token(&word) else {
            continue;
        };
        if confidence >= 1.0 {
            return EraHit {
                era: Some(era),
                confidence: 1.0,
            };
        }
        if confidence > best.confidence {
            best = EraHit {
                era: Some(era),
                confidence,
            };
        }
    }
    best
}

/// Filter-tab label above the relic grid. I is folded to L so "ALL" misreads (AII, A11, ALI)
/// still hit, without lev-1 false positives (AXL).
pub fn detect_from_filter_label(text: &str) -> EraHit {
    let upper = fold_accents(text).to_uppercase();
    let is_all_word = |w: &str| {
        FILTER_ALL_WORDS.iter().any(|(_, all)| {
            if all.is_ascii() {
                w == *all
            } else {
                w.contains(all)
            }
        })
    };
    if upper
        .split(|c: char| c.is_ascii() && !c.is_ascii_alphanumeric())
        .any(is_all_word)
    {
        return EraHit {
            era: Some("omnia"),
            confidence: 1.0,
        };
    }
    let normalized = normalize_ocr_phrase(&english(text));
    if normalized.is_empty() {
        return NONE;
    }
    let mut best = NONE;
    for raw in normalized.split(' ') {
        let word = normalize_for_ocr(raw);
        if word == "ALL" || word == "OMNIA" {
            return EraHit {
                era: Some("omnia"),
                confidence: 1.0,
            };
        }
        if word.len() == 3 && word.replace('I', "L") == "ALL" && best.confidence < 0.95 {
            best = EraHit {
                era: Some("omnia"),
                confidence: 0.95,
            };
        }
    }
    let hit = detect_from_text(&normalized);
    if hit.confidence > best.confidence {
        hit
    } else {
        best
    }
}

/// Header band above the relic grid; wide enough to catch a fissure list or a neighbouring panel.
pub fn detect_from_band(text: &str) -> EraHit {
    let normalized = normalize_ocr_phrase(&english(text));
    if normalized.is_empty() || distinct_era_mentions(&normalized) > 1 {
        return NONE;
    }
    detect_from_text(&normalized)
}

pub fn detect_from_tile_label(text: &str) -> EraHit {
    let normalized = normalize_ocr_phrase(&english(text));
    // The star chart lists every active fissure, so a crop that catches the panel sees several.
    if normalized.is_empty() || distinct_era_mentions(&normalized) > 1 {
        return NONE;
    }
    let base = detect_from_text(&normalized);
    if base.era.is_none() || base.confidence <= 0.0 {
        return base;
    }
    let mut confidence = base.confidence;
    if ERAS
        .iter()
        .any(|e| normalized.starts_with(&format!("{} ", e.to_uppercase())))
    {
        confidence += 0.08;
    }
    if normalized.contains(" RELIC") {
        confidence += 0.06;
    }
    EraHit {
        era: base.era,
        confidence: confidence.min(1.0),
    }
}
