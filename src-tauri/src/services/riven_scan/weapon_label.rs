//! Weapon matching for riven scans (port of WFHelper's `findWeaponInText` / `findWeaponByLabelLine`
//! and the label-vs-current-weapon rule of `rivenWeaponLabel`). The caller feeds the OCR lines; the
//! crops and the recogniser are `riven_scan` image code.

use std::collections::HashMap;

use crate::services::reward_scan::fold::levenshtein;

/// Names the export lags behind on, keyed by their normalized card text.
const WEAPON_OCR_ALIASES: &[(&str, &str)] = &[
    ("euphona", "Euphona Prime"),
    ("gotva", "Gotva Prime"),
    ("reaper", "Reaper Prime"),
];

/// Shortest plate line a one-letter miss may match: Akzani is six characters.
const MIN_FUZZY_LABEL_CHARS: usize = 5;

fn alias(normalized: &str) -> Option<&'static str> {
    WEAPON_OCR_ALIASES
        .iter()
        .find(|(k, _)| *k == normalized)
        .map(|(_, v)| *v)
}

/// Lowercase ASCII words separated by single spaces.
pub fn normalize_weapon_ocr_text(text: &str) -> String {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !c.is_ascii_lowercase() && !c.is_ascii_digit())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponLabelMatch {
    pub name: String,
    /// True for a byte-exact (normalized) hit; false for a distance-tolerant one.
    pub exact: bool,
}

/// Where the current weapon of a scan came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponSource {
    None,
    Dialog,
    Ocr,
    Diorama,
    Label,
}

/// Whether a fits-in label read replaces the current weapon.
pub fn should_apply_label_weapon(
    label: &WeaponLabelMatch,
    current_name: &str,
    current_source: WeaponSource,
    same_family: bool,
) -> bool {
    if current_name.is_empty() || current_name == "Riven" {
        return true;
    }
    if label.name == current_name {
        return false;
    }
    if label.exact || same_family {
        return true;
    }
    current_source == WeaponSource::Ocr
}

/// The weapon list in export order, with the normalized forms the matchers compare.
pub struct WeaponIndex {
    /// (lowercase, display) per weapon.
    lower: Vec<(String, String)>,
    /// (normalized, display); a later weapon with the same normalized form overwrites the display.
    normalized: Vec<(String, String)>,
    by_normalized: HashMap<String, usize>,
}

impl WeaponIndex {
    pub fn new<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut me = Self {
            lower: vec![],
            normalized: vec![],
            by_normalized: HashMap::new(),
        };
        for name in names {
            me.lower.push((name.to_lowercase(), name.to_owned()));
            let norm = normalize_weapon_ocr_text(name);
            match me.by_normalized.get(&norm) {
                Some(&slot) => me.normalized[slot].1 = name.to_owned(),
                None => {
                    me.by_normalized.insert(norm.clone(), me.normalized.len());
                    me.normalized.push((norm, name.to_owned()));
                }
            }
        }
        me
    }

    /// Also matches each `(card name, display)`, a game-language name, without displacing a
    /// weapon already indexed under the same normalized form.
    pub fn with_aliases<'a>(
        mut self,
        aliases: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        for (name, display) in aliases {
            self.lower.push((name.to_lowercase(), display.to_owned()));
            let norm = normalize_weapon_ocr_text(name);
            if !self.by_normalized.contains_key(&norm) {
                self.by_normalized
                    .insert(norm.clone(), self.normalized.len());
                self.normalized.push((norm, display.to_owned()));
            }
        }
        self
    }

    /// Prefers the longest OCR weapon match so "Bo" does not match inside "Boar", and only at the
    /// start of a line, where a card's "<Weapon> <RivenSuffix>" puts it. Suffixes are built from
    /// stat syllables ("Lexi-gelitron") that collide with short weapon names and graded the wrong
    /// gun when the real one was unknown.
    pub fn find_in_text(&self, text: &str) -> Option<String> {
        let line_starts: Vec<String> = text
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty())
            .collect();

        let mut best_exact: Option<&str> = None;
        let mut best_exact_len = 0;
        for (name_lc, display) in &self.lower {
            let len = name_lc.chars().count();
            // Short names are skipped to avoid false positives.
            if len <= best_exact_len || len < 3 {
                continue;
            }
            // Anchored, and on a word boundary: "Lex" must not match "Lexi-gelitron".
            let anchored = line_starts.iter().any(|line| {
                line.strip_prefix(name_lc.as_str()).is_some_and(|rest| {
                    !rest
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                })
            });
            if anchored {
                best_exact = Some(display);
                best_exact_len = len;
            }
        }

        let lines: Vec<String> = text
            .lines()
            .map(normalize_weapon_ocr_text)
            .filter(|l| l.chars().count() >= 4)
            .take(4)
            .collect();

        if let Some(found) = lines.iter().find_map(|l| alias(l)) {
            return Some(found.to_owned());
        }

        struct Candidate<'a> {
            name: &'a str,
            distance: usize,
            token_count: usize,
        }
        let mut best_candidate: Option<Candidate> = None;

        for line in &lines {
            let words: Vec<&str> = line.split(' ').filter(|w| w.chars().count() >= 2).collect();
            if words.is_empty() {
                continue;
            }
            // Only phrases that begin the line - see the anchoring note above.
            for len in 1..=4.min(words.len()) {
                let phrase = words[..len].join(" ");
                if phrase.chars().count() < 4 {
                    continue;
                }
                if let Some(found) = alias(&phrase) {
                    return Some(found.to_owned());
                }
                for (normalized_weapon, display) in &self.normalized {
                    if normalized_weapon.is_empty() {
                        continue;
                    }
                    let weapon_words = normalized_weapon.split(' ').count();
                    if len.abs_diff(weapon_words) > 1 {
                        continue;
                    }
                    let weapon_len = normalized_weapon.chars().count();
                    let max_distance = if weapon_len >= 14 {
                        3
                    } else if weapon_len >= 8 {
                        2
                    } else {
                        1
                    };
                    let distance = levenshtein(&phrase, normalized_weapon);
                    if distance > max_distance {
                        continue;
                    }
                    let better = best_candidate.as_ref().is_none_or(|b| {
                        distance < b.distance
                            || (distance <= b.distance + 1 && weapon_words > b.token_count)
                    });
                    if better {
                        best_candidate = Some(Candidate {
                            name: display,
                            distance,
                            token_count: weapon_words,
                        });
                    }
                }
            }
        }

        if let Some(candidate) = &best_candidate {
            let Some(exact) = best_exact else {
                return Some(candidate.name.to_owned());
            };
            let words = |s: &str| normalize_weapon_ocr_text(s).split(' ').count();
            if words(candidate.name) > words(exact) {
                return Some(candidate.name.to_owned());
            }
        }
        best_exact
            .map(str::to_owned)
            .or_else(|| best_candidate.map(|c| c.name.to_owned()))
    }

    /// Last resort for a title that does not start its line (a localized "Mod Riven de Mêlée"
    /// header read on the same line): the longest weapon name inside the first lines, on word
    /// boundaries. Names under six characters are skipped, as stat syllables collide with them.
    pub fn find_inside_lines(&self, text: &str) -> Option<String> {
        let words = |s: &str| format!(" {} ", normalize_weapon_ocr_text(s));
        let lines: Vec<String> = text.lines().take(3).map(words).collect();
        self.lower
            .iter()
            .map(|(lc, display)| (words(lc), display))
            .filter(|(w, _)| w.chars().count() >= 8)
            .filter(|(w, _)| lines.iter().any(|l| l.contains(w.as_str())))
            .max_by_key(|(w, _)| w.len())
            .map(|(_, d)| d.clone())
    }

    /// Matches whole OCR lines against the weapon list. The panel caption holds nothing but a
    /// weapon name, so unlike [`find_in_text`](Self::find_in_text) only a full-line hit counts;
    /// that keeps FITS IN, CANCEL and stat rows from ever matching.
    pub fn find_by_label_line<S: AsRef<str>>(&self, lines: &[S]) -> Option<WeaponLabelMatch> {
        struct Best<'a> {
            name: &'a str,
            exact: bool,
            len: usize,
            distance: usize,
        }
        let mut best: Option<Best> = None;
        for raw in lines {
            let norm = normalize_weapon_ocr_text(raw.as_ref());
            let len = norm.chars().count();
            if len < 3 {
                continue;
            }
            if let Some(&slot) = self.by_normalized.get(&norm) {
                if best.as_ref().is_none_or(|b| !b.exact || len > b.len) {
                    best = Some(Best {
                        name: &self.normalized[slot].1,
                        exact: true,
                        len,
                        distance: 0,
                    });
                }
                continue;
            }
            if len < MIN_FUZZY_LABEL_CHARS || best.as_ref().is_some_and(|b| b.exact) {
                continue;
            }
            let max_distance = if len >= 14 { 2 } else { 1 };
            let mut line_best: Option<(&str, usize)> = None;
            let mut ambiguous = false;
            for (weapon, display) in &self.normalized {
                if weapon.chars().count().abs_diff(len) > max_distance {
                    continue;
                }
                let distance = levenshtein(&norm, weapon);
                if distance > max_distance {
                    continue;
                }
                match line_best {
                    Some((_, d)) if distance > d => {}
                    Some((name, d)) if distance == d => {
                        if display != name {
                            ambiguous = true;
                        }
                    }
                    _ => {
                        line_best = Some((display, distance));
                        ambiguous = false;
                    }
                }
            }
            let Some((name, distance)) = line_best.filter(|_| !ambiguous) else {
                continue;
            };
            let better = best
                .as_ref()
                .is_none_or(|b| distance < b.distance || (distance == b.distance && len > b.len));
            if better {
                best = Some(Best {
                    name,
                    exact: false,
                    len,
                    distance,
                });
            }
        }
        best.map(|b| WeaponLabelMatch {
            name: b.name.to_owned(),
            exact: b.exact,
        })
    }
}
