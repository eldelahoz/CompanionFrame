//! Text folding shared by the matcher: accents, OCR lookalikes, quantity prefixes, edit distance.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Rewards carry the bonus count ("2X Forma Blueprint") and a card prints "2 X", so "2X", "2 X",
/// "2x" all fold to one. A leading "x2" is never a count ("X3lp Glyph" is a real name).
static QUANTITY_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\d+\s*x\s+").unwrap());
static QUANTITY_SPACING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)\s+x(\s|$)").unwrap());

pub fn has_quantity_prefix(value: &str) -> bool {
    QUANTITY_PREFIX.is_match(value)
}

pub fn strip_quantity_prefix(value: &str) -> &str {
    QUANTITY_PREFIX
        .find(value)
        .map_or(value, |m| &value[m.end()..])
}

/// "2 x forma" → "2x forma", so the card spelling meets the pool spelling.
pub fn collapse_quantity_spacing(value: &str) -> String {
    QUANTITY_SPACING.replace(value, "${1}x${2}").into_owned()
}

/// Accents are folded, not dropped: "Châssis" must still meet an OCR "Chassis".
/// Letters with no decomposition (ß, ø) stay as they are; callers filter them out.
pub fn fold_accents(value: &str) -> String {
    value.nfd().filter(|c| !is_combining_mark(*c)).collect()
}

pub fn normalize_for_search(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Uppercase letters only, with the usual digit lookalikes read as letters.
pub fn normalize_for_ocr(value: &str) -> String {
    value
        .to_uppercase()
        .chars()
        .map(|c| match c {
            '1' | '|' | '!' => 'I',
            '0' => 'O',
            '5' => 'S',
            c => c,
        })
        .filter(char::is_ascii_uppercase)
        .collect()
}

/// Keeps word boundaries: which words appeared, and in what order, is the evidence.
pub fn normalize_ocr_phrase(value: &str) -> String {
    value
        .to_uppercase()
        .chars()
        .map(|c| {
            if c.is_ascii_uppercase() || c.is_ascii_digit() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

pub fn similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let longest = a.chars().count().max(b.chars().count());
    (1.0 - levenshtein(a, b) as f64 / longest as f64).max(0.0)
}
