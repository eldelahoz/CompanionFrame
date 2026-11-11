//! Riven cards in the game language: the stat labels and weapon names of the active game locale,
//! built from the bundled `ExportUpgrades` / `ExportWeapons` and DE's dictionaries. Localized stat
//! labels are rewritten to their English names before the (English-tuned) text parser runs.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::services::bundled_data::BundledData;
use crate::services::reward_scan::fold::{fold_accents, levenshtein};

static VAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\|[^|]*\|\s*(?:%|s(?:\s|$))?\s*").unwrap());
static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new("<[^>]+>").unwrap());
static PARENS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[(（].*?[)）]\s*").unwrap());
/// A qualifier such as "(x2 pour les Attaques Lourdes)", possibly cut at the line end. A group
/// that starts with a digit is a misread faction multiplier ("(1,25 Dégâts…"), not a qualifier.
static QUALIFIER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[(（]\s*([^\d()（）][^()（）]*)(?:[)）]|$)").unwrap());
/// The sign and value ahead of a stat name, with its % or seconds unit.
static VALUE_HEAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[+\-–x×]?\s*\d[\d.,]*\s*(?:%|s\b)?\s*").unwrap());

/// Card label of a template: "|val|% de Chances de Critique (x2 …)" -> "de Chances de Critique".
fn template_label(raw: &str) -> String {
    let s = VAL.replace_all(raw, "");
    let s = TAGS.replace_all(&s, "");
    PARENS.replace_all(&s, " ").trim().to_owned()
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | '‘' | '`' | '´')
}

/// Lowercase, accents folded, apostrophes and whitespace runs as one space (OCR drops or splits
/// both). Each folded byte keeps the byte range of the source char it came from.
fn fold_with_map(s: &str) -> (String, Vec<(usize, usize)>) {
    let mut out = String::with_capacity(s.len());
    let mut map = Vec::with_capacity(s.len());
    for (i, ch) in s.char_indices() {
        let span = (i, i + ch.len_utf8());
        let folded = if is_apostrophe(ch) {
            " ".to_owned()
        } else {
            fold_accents(&ch.to_lowercase().collect::<String>())
        };
        for c in folded.chars() {
            let c = if c.is_whitespace() { ' ' } else { c };
            if c == ' ' && (out.is_empty() || out.ends_with(' ')) {
                continue;
            }
            out.push(c);
            map.extend(std::iter::repeat_n(span, c.len_utf8()));
        }
    }
    (out, map)
}

pub fn fold(s: &str) -> String {
    fold_with_map(s).0.trim().to_owned()
}

fn boundary(c: Option<char>) -> bool {
    !c.is_some_and(char::is_alphanumeric)
}

struct Label {
    folded: String,
    tag: String,
    english: String,
}

#[derive(Default)]
pub struct CardLabels {
    code: String,
    /// Longest folded label first, so "de degats critiques" wins over "de degats".
    stats: Vec<Label>,
    /// Folded localized weapon name -> English display name, for names that differ.
    weapons: HashMap<String, String>,
}

impl CardLabels {
    /// English (or a bundle without the dictionaries) gets an empty table: the parser reads English.
    pub fn load(bundled: &BundledData, code: &str) -> Self {
        let empty = || Self {
            code: code.to_owned(),
            ..Self::default()
        };
        if code == "en" {
            return empty();
        }
        let (Some(en), Some(loc)) = (bundled.dict("en"), bundled.dict(code)) else {
            return empty();
        };
        let weapons = bundled.export("ExportWeapons").unwrap_or_default();
        let upgrades = bundled.export("ExportUpgrades").unwrap_or_default();
        Self::build(code, &en, &loc, &weapons, &upgrades)
    }

    pub fn build(
        code: &str,
        en: &Map<String, Value>,
        loc: &Map<String, Value>,
        weapons: &Map<String, Value>,
        upgrades: &Map<String, Value>,
    ) -> Self {
        let text =
            |d: &Map<String, Value>, k: &str| d.get(k).and_then(Value::as_str).map(str::to_owned);
        let mut stats: Vec<Label> = Vec::new();
        let entries = upgrades
            .iter()
            .filter(|(k, _)| k.contains("Randomized"))
            .filter_map(|(_, m)| m.get("upgradeEntries").and_then(Value::as_array))
            .flatten();
        for ue in entries {
            let Some(tag) = ue.get("tag").and_then(Value::as_str) else {
                continue;
            };
            let values = ue.get("upgradeValues").and_then(Value::as_array);
            for loc_tag in values
                .into_iter()
                .flatten()
                .filter_map(|v| v.get("locTag")?.as_str())
            {
                let (Some(e), Some(l)) = (text(en, loc_tag), text(loc, loc_tag)) else {
                    continue;
                };
                let (english, folded) = (template_label(&e), fold(&template_label(&l)));
                if english.is_empty()
                    || folded.is_empty()
                    || stats.iter().any(|s| s.folded == folded)
                {
                    continue;
                }
                stats.push(Label {
                    folded,
                    tag: tag.to_owned(),
                    english,
                });
            }
        }
        stats.sort_by_key(|s| std::cmp::Reverse(s.folded.len()));

        let mut names = HashMap::new();
        for w in weapons
            .values()
            .filter(|w| w.get("omegaAttenuation").is_some())
        {
            let Some(key) = w
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| n.starts_with('/'))
            else {
                continue;
            };
            if let (Some(e), Some(l)) = (text(en, key), text(loc, key))
                && fold(&e) != fold(&l)
            {
                names.insert(fold_accents(&l), e);
            }
        }
        Self {
            code: code.to_owned(),
            stats,
            weapons: names,
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    /// Upgrade tag of a card label in this language ("de Chances de Critique" -> WeaponCritChanceMod).
    pub fn tag(&self, label: &str) -> Option<&str> {
        let f = fold(label);
        self.stats
            .iter()
            .find(|s| s.folded == f)
            .map(|s| s.tag.as_str())
    }

    /// (localized name with accents folded, English name) for weapons whose name differs.
    pub fn weapon_aliases(&self) -> impl Iterator<Item = (&str, &str)> {
        self.weapons.iter().map(|(l, e)| (l.as_str(), e.as_str()))
    }

    /// Rewrites each line's localized stat labels to the English names; lines with none stay as
    /// read, so English text still parses (the English fallback).
    pub fn to_english(&self, text: &str) -> String {
        if self.stats.is_empty() {
            return text.to_owned();
        }
        let mut lines: Vec<String> = Vec::new();
        for l in text.split('\n') {
            if let Some(prev) = lines.last_mut()
                && self.completes_label(prev, l)
            {
                prev.push(' ');
                prev.push_str(l.trim());
                continue;
            }
            lines.push(l.to_owned());
        }
        lines
            .iter()
            .map(|l| self.line_to_english(l))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A label wrapped onto the next line: "+90,1% de Dégâts en" + "Mêlée". Read alone, the first
    /// line would match the shorter "de Dégâts" (Damage) instead of "de Dégâts en Mêlée".
    fn completes_label(&self, prev: &str, next: &str) -> bool {
        let digit = |s: &str| s.chars().any(|c| c.is_ascii_digit());
        let tail = fold(next);
        if tail.is_empty() || digit(next) || !digit(prev) {
            return false;
        }
        let joined = fold(&format!("{prev} {next}"));
        // The fragment may also sit mid-label ("... de Critique pour l'Attaque" + "Glissée"): the text
        // up to it must be the label's opening words.
        let needle = format!(" {tail}");
        self.stats.iter().any(|s| {
            s.folded.match_indices(&needle).any(|(k, _)| {
                let end = k + needle.len();
                s.folded[end..].is_empty() || s.folded[end..].starts_with(' ')
            }) && s
                .folded
                .match_indices(&needle)
                .any(|(k, _)| joined.ends_with(&s.folded[..k + needle.len()]))
        })
    }

    fn line_to_english(&self, line: &str) -> String {
        let has_value = line.chars().any(|c| c.is_ascii_digit());
        let (folded, map) = fold_with_map(line);
        let mut hits: Vec<(usize, usize, &str)> = Vec::new();
        for s in &self.stats {
            for (at, _) in folded.match_indices(s.folded.as_str()) {
                let end = at + s.folded.len();
                if !boundary(folded[..at].chars().next_back())
                    || !boundary(folded[end..].chars().next())
                {
                    continue;
                }
                if hits.iter().any(|&(a, b, _)| at < b && a < end) {
                    continue;
                }
                hits.push((at, end, &s.english));
            }
        }
        // A title line ("Épée de Chaleur Crita-ata") has no value: only a whole-label line (a name
        // wrapped below its value) is rewritten there.
        if !has_value {
            let whole = folded.trim_matches(|c: char| !c.is_alphanumeric());
            return match self.stats.iter().find(|s| s.folded == whole) {
                Some(s) => s.english.clone(),
                None => line.to_owned(),
            };
        }
        if hits.is_empty() {
            return self.fuzzy(line).unwrap_or_else(|| line.to_owned());
        }
        hits.sort_by_key(|h| h.0);
        let mut out = String::with_capacity(line.len() + 16);
        let mut last = 0;
        for (a, b, english) in hits {
            let (from, to) = (map[a].0, map[b - 1].1);
            out.push_str(&line[last..from]);
            out.push(' ');
            out.push_str(english);
            out.push(' ');
            last = to;
        }
        out.push_str(&line[last..]);
        QUALIFIER
            .replace_all(&out, |c: &regex::Captures| {
                if c[1].chars().filter(|ch| ch.is_alphabetic()).count() >= 3 {
                    String::new()
                } else {
                    c[0].to_owned()
                }
            })
            .trim()
            .to_owned()
    }

    /// One OCR slip in a value line's name ("de Chances de Critiqve").
    fn fuzzy(&self, line: &str) -> Option<String> {
        let head = VALUE_HEAD.find(line)?;
        let name = fold(&line[head.end()..]);
        let name = name.trim_end_matches(|c: char| !c.is_alphanumeric());
        if name.chars().count() < 5 {
            return None;
        }
        let best = self
            .stats
            .iter()
            .filter(|s| s.folded.chars().count() >= 6)
            .map(|s| (levenshtein(name, &s.folded), s))
            .filter(|(d, s)| *d <= (s.folded.chars().count() / 6).max(1))
            .min_by_key(|(d, _)| *d)?;
        Some(format!("{} {}", head.as_str().trim_end(), best.1.english))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;
    use crate::services::riven_scan::text::parse_riven_stats;
    use crate::services::rivens::vocab::stat_name_to_tag;

    struct Bundle {
        dicts: HashMap<&'static str, Map<String, Value>>,
        weapons: Map<String, Value>,
        upgrades: Map<String, Value>,
    }

    /// The real bundle; tests skip when `pnpm game-data` has not run.
    fn bundle() -> Option<&'static Bundle> {
        static B: OnceLock<Option<Bundle>> = OnceLock::new();
        B.get_or_init(|| {
            let root = crate::services::bundled_data::test_root()?;
            let b = BundledData::new(root);
            let dicts = ["en", "fr", "es", "de", "zh"]
                .into_iter()
                .map(|c| (c, b.dict(c).expect("dict missing from the bundle")))
                .collect();
            Some(Bundle {
                dicts,
                weapons: b
                    .export("ExportWeapons")
                    .expect("ExportWeapons in the bundle"),
                upgrades: b
                    .export("ExportUpgrades")
                    .expect("ExportUpgrades in the bundle"),
            })
        })
        .as_ref()
    }

    fn labels(b: &Bundle, code: &str) -> CardLabels {
        CardLabels::build(
            code,
            &b.dicts["en"],
            &b.dicts[code],
            &b.weapons,
            &b.upgrades,
        )
    }

    const UPG: &str = "/Lotus/Language/Upgrades/";

    /// A card line as the game prints it: the template with its value filled in.
    fn line(b: &Bundle, code: &str, loc: &str, value: &str) -> String {
        let t = b.dicts[code][&format!("{UPG}{loc}")].as_str().unwrap();
        let t = TAGS.replace_all(t, "");
        Regex::new(r"\|[^|]*\|")
            .unwrap()
            .replace(&t, value)
            .into_owned()
    }

    /// Commas for the decimal point, as the French and Spanish cards print it.
    fn card(b: &Bundle, code: &str, rows: &[(&str, &str)]) -> String {
        let comma = |v: &str| {
            if code == "en" {
                v.to_owned()
            } else {
                v.replace('.', ",")
            }
        };
        let mut out = vec!["Braton Crita-tis".to_owned()];
        out.extend(rows.iter().map(|(loc, v)| line(b, code, loc, &comma(v))));
        out.join("\n")
    }

    fn tags(l: &CardLabels, text: &str) -> Vec<(String, bool, Option<f64>)> {
        parse_riven_stats(&l.to_english(text), None)
            .into_iter()
            .map(|s| {
                let tag = stat_name_to_tag(&s.name).unwrap_or("?").to_owned();
                (tag, s.positive, s.value)
            })
            .collect()
    }

    const GUN: &[(&str, &str)] = &[
        ("WeaponCritChance", "+123.4"),
        ("WeaponMultishotModDesc", "+88.8"),
        ("WeaponFactionDamageGrineerDesc", "x1.25"),
        ("WeaponRecoilModDesc", "+45.6"),
    ];
    const MELEE: &[(&str, &str)] = &[
        ("WeaponHeavyCritChance", "+150.2"),
        ("MeleeDamageBuffDesc", "+90.1"),
        ("WeaponMeleeAttackSpeed", "+33.3"),
        ("WeaponCritDamage", "-40.5"),
    ];

    #[test]
    fn fr_and_es_tables_hold_the_riven_stats() {
        let Some(b) = bundle() else { return };
        for code in ["fr", "es", "de", "zh"] {
            let l = labels(b, code);
            let tag = |loc: &str| {
                l.tag(&template_label(
                    b.dicts[code][&format!("{UPG}{loc}")].as_str().unwrap(),
                ))
            };
            assert_eq!(
                tag("WeaponCritChance"),
                Some("WeaponCritChanceMod"),
                "{code}"
            );
            assert_eq!(
                tag("WeaponMultishotModDesc"),
                Some("WeaponFireIterationsMod"),
                "{code}"
            );
            assert!(
                tag("WeaponFactionDamageGrineerDesc")
                    .is_some_and(|t| t.ends_with("FactionDamageGrineer")),
                "{code}"
            );
            assert_eq!(
                tag("MeleeDamageBuffDesc"),
                Some("WeaponMeleeDamageMod"),
                "{code}"
            );
            assert_eq!(
                tag("WeaponMeleeAttackSpeed"),
                Some("WeaponFireRateMod"),
                "{code}"
            );
        }
        assert_eq!(
            labels(b, "fr").tag("DE CHANCES DE CRITIQUE"),
            Some("WeaponCritChanceMod")
        );
    }

    #[test]
    fn localized_cards_parse_like_the_english_one() {
        let Some(b) = bundle() else { return };
        let en = CardLabels::default();
        for rows in [GUN, MELEE] {
            let want = tags(&en, &card(b, "en", rows));
            assert_eq!(want.len(), 4, "{want:?}");
            assert!(!want.iter().any(|t| t.0 == "?"), "{want:?}");
            for code in ["fr", "es", "de", "zh"] {
                let text = card(b, code, rows);
                assert_eq!(tags(&labels(b, code), &text), want, "{code}: {text}");
            }
        }
    }

    #[test]
    fn ocr_without_accents_or_with_a_slip_still_matches() {
        let Some(b) = bundle() else { return };
        let fr = labels(b, "fr");
        let text = "Braton Crita-tis\n+123,4% de Chances de Critiqve\n+60% de Degats Critiques\n+30% de Vitesse d Attaque\n-12% de Recul de lArme";
        let got: Vec<_> = tags(&fr, text).into_iter().map(|t| t.0).collect();
        assert_eq!(
            got,
            [
                "WeaponCritChanceMod",
                "WeaponCritDamageMod",
                "WeaponFireRateMod",
                "WeaponRecoilReductionMod"
            ]
        );
    }

    #[test]
    fn a_french_melee_damage_label_wrapped_onto_two_lines_stays_melee() {
        let Some(b) = bundle() else { return };
        let fr = labels(b, "fr");
        for text in [
            "Orthos Crita-tis\n+90,1% de Dégâts en\nMêlée\n+33,3% de Vitesse d'Attaque",
            "Orthos Crita-tis\n+90,1% de Degats en\nMelee\n+33,3% de Vitesse d'Attaque",
        ] {
            let got: Vec<_> = tags(&fr, text).into_iter().map(|t| t.0).collect();
            assert_eq!(got, ["WeaponMeleeDamageMod", "WeaponFireRateMod"], "{text}");
        }
        // A whole label wrapped below its value is left to the parser, as before.
        let got = tags(&fr, "Orthos\n+60%\nde Dégâts Critiques");
        assert_eq!(got.len(), 1, "{got:?}");
    }

    #[test]
    fn english_text_passes_through_a_localized_table() {
        let Some(b) = bundle() else { return };
        let text = card(b, "en", GUN);
        assert_eq!(
            tags(&labels(b, "fr"), &text),
            tags(&CardLabels::default(), &text)
        );
    }

    #[test]
    fn localized_weapon_names_resolve_to_english() {
        let Some(b) = bundle() else { return };
        let fr = labels(b, "fr");
        let names: HashMap<_, _> = fr.weapon_aliases().collect();
        assert_eq!(names.get("Epee a Plasma"), Some(&"Plasma Sword"));
    }
}
