//! Ranking of an OCR read against the reward pool. Ported from WFHelper `rewardScannerMatch.ts`:
//! the score bands are disjoint (exact > substring > fuzzy) so a fuzzy near-miss on a longer name
//! can never outrank a perfect hit.

use std::collections::{HashMap, HashSet};

use super::fold::{
    collapse_quantity_spacing, fold_accents, has_quantity_prefix, normalize_for_search, similarity,
    strip_quantity_prefix,
};
use super::names::PoolItem;

pub const MAX_REWARD_SLOTS: usize = 4;
/// Every containment match is clamped up to this, so its score is not a measurement.
pub const SUBSTRING_SCORE_FLOOR: f64 = 0.88;
const EXACT_MATCH_SKIP_OVERLAP_COUNT: usize = 3;
const MIN_MATCHED_WORDS_FOR_OVERLAP: usize = 2;
const OVERLAP_CONFIDENCE_FLOOR: f64 = 0.86;
/// Lift a partial structural match only when it uniquely identifies one item.
const UNIQUE_STRUCTURAL_CONFIDENCE: f64 = 0.93;
const MIN_FUZZY_NAME_LENGTH_RATIO: f64 = 0.45;
const MIN_FUZZY_WORD_RATIO: f64 = 0.45;
const MIN_SUBSEQUENCE_WORD_RATIO: f64 = 0.6;
/// Shortest read word that carries evidence; below this it is OCR noise.
const MIN_MEANINGFUL_READ_WORD_LENGTH: usize = 3;
/// A confusion-folded hit is an equality, but not of the printed spelling.
const FOLDED_EXACT_CONFIDENCE: f64 = 0.99;
// Sibling part names sit near 0.7 folded; the length floor keeps short mod names out.
const FOLDED_NEAR_MIN_SIMILARITY: f64 = 0.94;
const FOLDED_NEAR_MIN_LENGTH: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Exact,
    Substring,
    Fuzzy,
    None,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub item: Option<PoolItem>,
    pub confidence: f64,
    pub score: f64,
    pub mode: Mode,
}

/// Misreads the OCR model is known to make; keys are already lowercase letters/digits.
fn token_alias(word: &str) -> Option<&'static str> {
    Some(match word {
        "bluedrint" | "blueorint" | "blueprlnt" | "blueprini" | "bluepnnt" | "biueprint"
        | "lueprint" => "blueprint",
        "svst" | "svstems" | "systerns" | "syst" | "syslems" => "systems",
        "neurootics" | "neurotics" | "neuroptlcs" | "neurcptics" | "neuropiics" | "neuraptics"
        | "eurobtic" | "europtics" => "neuroptics",
        "chassls" | "chassi" | "chassl" | "chassi5" | "hassis" => "chassis",
        "recelver" | "recelvar" | "recei" | "rece1ver" => "receiver",
        "wukon" => "wukong",
        "rhln" | "rhin" => "rhino",
        "sarv" => "saryn",
        "nek" | "nekr" => "nekros",
        "obero" | "obenon" => "oberon",
        "trinlty" | "trini" | "trinit" => "trinity",
        "bans" | "bansh" => "banshee",
        "equlnox" | "equln" => "equinox",
        "hy" | "hyd" => "hydroid",
        "ivar" => "ivara",
        "llmbo" => "limbo",
        "prlme" | "pnme" | "prix" | "priime" => "prime",
        "barre" | "banel" => "barrel",
        "bilade" => "blade",
        "stoc" | "slock" => "stock",
        "grlp" => "grip",
        "forrna" | "forna" => "forma",
        _ => return None,
    })
}

fn normalize_reward_word(word: &str) -> String {
    let folded: String = fold_accents(&normalize_for_search(word))
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    token_alias(&folded).map_or(folded, str::to_owned)
}

/// The card prints the count as "2 X" while the pool name is "2X ...", so the spacing is
/// collapsed before anything compares the two.
pub fn normalize_reward_text(text: &str) -> String {
    let joined = text
        .split_whitespace()
        .map(normalize_reward_word)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    collapse_quantity_spacing(&joined)
}

/// High-resolution captures merge glyph pairs and print digit lookalikes ("81ueprint"); folding
/// those back and dropping spacing recovers the name.
fn fold_ocr_confusions(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '8' => 'b',
            '0' => 'o',
            '1' => 'l',
            '5' => 's',
            c => c,
        })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}

/// Folded spelling of an already normalized name.
pub(super) fn folded_name(normalized: &str) -> String {
    fold_ocr_confusions(normalized)
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

fn find_phrase(text: &str, phrase: &str) -> Option<usize> {
    if text.is_empty() || phrase.is_empty() {
        return None;
    }
    text.match_indices(phrase).map(|(i, _)| i).find(|&i| {
        !text[..i].chars().next_back().is_some_and(is_word_char)
            && !text[i + phrase.len()..]
                .chars()
                .next()
                .is_some_and(is_word_char)
    })
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    find_phrase(text, phrase).is_some()
}

fn is_useful_partial(text: &str) -> bool {
    text.split(' ').filter(|w| w.len() > 1).count() >= 2 || text.len() >= 5
}

/// Word-level tolerance: OCR corruptions the alias table doesn't know must not break matching.
fn words_equivalent(text_word: &str, name_word: &str) -> bool {
    text_word == name_word
        || similarity(text_word, name_word) >= if name_word.len() >= 7 { 0.7 } else { 0.78 }
}

fn is_ordered_subsequence(text_words: &[&str], name_words: &[&str]) -> bool {
    let mut name_index = 0;
    for word in text_words {
        while name_index < name_words.len() && !words_equivalent(word, name_words[name_index]) {
            name_index += 1;
        }
        if name_index >= name_words.len() {
            return false;
        }
        name_index += 1;
    }
    true
}

/// "2X Forma Blueprint" and "Forma Blueprint" are both real rewards. A pair that differs only by
/// the leading count follows the read; a counted name with no bare sibling stays reachable.
fn drop_quantity_mismatches(ranked: Vec<Candidate>, text: &str) -> Vec<Candidate> {
    let info: Vec<Option<(String, bool)>> = ranked
        .iter()
        .map(|c| {
            let name = &c.item.as_ref()?.normalized;
            let bare = strip_quantity_prefix(name);
            (!bare.is_empty()).then(|| (bare.to_owned(), bare != name))
        })
        .collect();
    let mut bare_seen = HashSet::new();
    let mut counted_seen = HashSet::new();
    for (bare, counted) in info.iter().flatten() {
        if *counted {
            &mut counted_seen
        } else {
            &mut bare_seen
        }
        .insert(bare.as_str());
    }
    if counted_seen.is_empty() {
        return ranked;
    }
    let text_has_quantity = has_quantity_prefix(text);
    let keep: Vec<bool> = ranked
        .iter()
        .zip(&info)
        .map(|(c, info)| {
            let Some((bare, counted)) = info else {
                return true;
            };
            // An equality names the read outright (OCR welds "2 X" onto the next word).
            !(bare_seen.contains(bare.as_str()) && counted_seen.contains(bare.as_str()))
                || c.mode == Mode::Exact
                || text_has_quantity == *counted
        })
        .collect();
    ranked
        .into_iter()
        .zip(keep)
        .filter_map(|(c, k)| k.then_some(c))
        .collect()
}

fn boost_unique_structural(ranked: &mut [Candidate], text: &str, text_words: &[&str]) {
    if text_words.len() < 2 || ranked.iter().any(|c| c.mode == Mode::Exact) {
        return;
    }
    // Keyed by name so duplicate pool entries cannot fake ambiguity.
    let mut prefix: HashMap<&str, usize> = HashMap::new();
    let mut contains: HashMap<&str, usize> = HashMap::new();
    let mut subsequence: HashMap<&str, usize> = HashMap::new();
    let mut thin: HashSet<&str> = HashSet::new();
    let text_space = format!("{text} ");
    for (i, c) in ranked.iter().enumerate() {
        let Some(item) = &c.item else { continue };
        let name = item.normalized.as_str();
        if c.mode == Mode::Substring && name.starts_with(&text_space) {
            prefix.insert(name, i);
        } else if c.mode == Mode::Substring && contains_phrase(text, name) {
            contains.insert(name, i);
        } else {
            let name_words: Vec<&str> = name.split(' ').filter(|w| w.len() > 1).collect();
            if name_words.len() <= text_words.len()
                || !is_ordered_subsequence(text_words, &name_words)
            {
                continue;
            }
            // Thin evidence bars a name from winning, never from competing.
            subsequence.insert(name, i);
            if (text_words.len() as f64 / name_words.len() as f64) < MIN_SUBSEQUENCE_WORD_RATIO {
                thin.insert(name);
            }
        }
    }
    // The strongest tier decides; a tier holding more than one name is ambiguous.
    let Some(tier) = [&prefix, &contains, &subsequence]
        .into_iter()
        .find(|t| !t.is_empty())
    else {
        return;
    };
    if tier.len() != 1 {
        return;
    }
    let (&name, &i) = tier.iter().next().unwrap();
    if thin.contains(name) || ranked[i].confidence >= UNIQUE_STRUCTURAL_CONFIDENCE {
        return;
    }
    let score = 400.0 + UNIQUE_STRUCTURAL_CONFIDENCE * 92.0 + (name.len() as f64 / 4.0).min(8.0);
    ranked[i].confidence = UNIQUE_STRUCTURAL_CONFIDENCE;
    ranked[i].score = score;
}

fn hit(item: &PoolItem, confidence: f64, score: f64, mode: Mode) -> Candidate {
    Candidate {
        item: Some(item.clone()),
        confidence,
        score,
        mode,
    }
}

const NO_CANDIDATE: Candidate = Candidate {
    item: None,
    confidence: 0.0,
    score: 0.0,
    mode: Mode::None,
};

pub fn rank_reward_candidates(ocr_text: &str, pool: &[PoolItem], limit: usize) -> Vec<Candidate> {
    let text = normalize_reward_text(ocr_text);
    if text.is_empty() {
        return vec![NO_CANDIDATE];
    }
    let text_words: Vec<&str> = text.split(' ').filter(|w| w.len() > 1).collect();
    let folded_text = fold_ocr_confusions(&text);
    let text_has_quantity = has_quantity_prefix(&text);
    let mut ranked = Vec::new();

    for item in pool {
        let name = item.normalized.as_str();
        if name.is_empty() {
            continue;
        }
        if text == name {
            ranked.push(hit(item, 1.0, 1000.0, Mode::Exact));
            continue;
        }
        if !folded_text.is_empty() && folded_text == item.folded {
            // Just under a literal hit so the untouched spelling still wins.
            ranked.push(hit(item, FOLDED_EXACT_CONFIDENCE, 990.0, Mode::Exact));
            continue;
        }
        let tail = (name.len() as f64 / 4.0).min(8.0);
        if contains_phrase(&text, name)
            || (is_useful_partial(&text) && contains_phrase(name, &text))
        {
            let confidence = SUBSTRING_SCORE_FLOOR.max(similarity(&text, name));
            ranked.push(hit(
                item,
                confidence,
                400.0 + confidence * 92.0 + tail,
                Mode::Substring,
            ));
            continue;
        }
        // A merged word breaks the per-word fuzzy pass even when the whole read is one edit off,
        // so compare folded spellings too. Long names only, never across a leading count: "2X"
        // and "5X" differ by the one character folding blurs.
        if !text_has_quantity
            && !has_quantity_prefix(name)
            && folded_text.len() >= FOLDED_NEAR_MIN_LENGTH
            && item.folded.len() >= FOLDED_NEAR_MIN_LENGTH
        {
            let folded = similarity(&folded_text, &item.folded);
            if folded >= FOLDED_NEAR_MIN_SIMILARITY {
                ranked.push(hit(
                    item,
                    folded,
                    400.0 + folded * 92.0 + tail,
                    Mode::Substring,
                ));
                continue;
            }
        }
        // A name much shorter than the read could ride one good word over the gate.
        if (name.len() as f64) < text.len() as f64 * MIN_FUZZY_NAME_LENGTH_RATIO {
            continue;
        }
        let item_words: Vec<&str> = name.split(' ').filter(|w| w.len() > 1).collect();
        if item_words.is_empty() {
            continue;
        }
        let mut matched_words = 0;
        let mut explained = vec![false; text_words.len()];
        for item_word in &item_words {
            let mut matched = false;
            for (i, tw) in text_words.iter().enumerate() {
                if words_equivalent(tw, item_word) {
                    matched = true;
                    explained[i] = true;
                }
            }
            matched_words += usize::from(matched);
        }
        let word_ratio = matched_words as f64 / item_words.len() as f64;
        if word_ratio < MIN_FUZZY_WORD_RATIO {
            continue;
        }
        // The weaker direction: scoring only the name's words would pay a candidate for the read
        // words it ignores. One or two character tokens are noise and count for neither side.
        let explained_count = explained.iter().filter(|e| **e).count();
        let unexplained = text_words
            .iter()
            .zip(&explained)
            .filter(|(w, e)| !**e && w.len() >= MIN_MEANINGFUL_READ_WORD_LENGTH)
            .count();
        let read_ratio = explained_count as f64 / (explained_count + unexplained) as f64;
        let coverage = word_ratio.min(read_ratio);

        let mut best_span = similarity(&text, name);
        if text_words.len() >= item_words.len() {
            for start in 0..=text_words.len() - item_words.len() {
                let span = text_words[start..start + item_words.len()].join(" ");
                best_span = best_span.max(similarity(&span, name));
            }
        }
        let confidence = (coverage * 0.6 + best_span * 0.4).min(0.97);
        ranked.push(hit(
            item,
            confidence,
            confidence * 100.0 + matched_words as f64 * 2.0,
            Mode::Fuzzy,
        ));
    }

    let mut survivors = drop_quantity_mismatches(ranked, &text);
    boost_unique_structural(&mut survivors, &text, &text_words);
    let name_len = |c: &Candidate| c.item.as_ref().map_or(0, |i| i.name.chars().count());
    survivors.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.confidence.total_cmp(&a.confidence))
            .then(name_len(b).cmp(&name_len(a)))
    });
    survivors.truncate(limit.max(1));
    survivors
}

/// One-best convenience over [`rank_reward_candidates`]; the pipeline ranks several, tests read one.
#[cfg(test)]
pub fn match_single_reward_text(ocr_text: &str, pool: &[PoolItem]) -> Candidate {
    rank_reward_candidates(ocr_text, pool, 1)
        .into_iter()
        .next()
        .unwrap_or(NO_CANDIDATE)
}

#[derive(Clone, Debug)]
pub struct MatchEntry {
    pub item: PoolItem,
    pub pos: usize,
    pub confidence: f64,
    pub exact: bool,
}

#[derive(Debug)]
pub struct MatchResult {
    pub matches: Vec<MatchEntry>,
    pub score: f64,
    pub exact_count: usize,
}

/// Several names in one read (full-text path): whole names first, then word overlap.
pub fn match_items(ocr_text: &str, threshold: f64, pool: &[PoolItem]) -> MatchResult {
    let text = normalize_for_search(ocr_text);
    let mut found: Vec<MatchEntry> = Vec::new();
    if text.is_empty() {
        return MatchResult {
            matches: found,
            score: 0.0,
            exact_count: 0,
        };
    }
    let words: HashSet<&str> = text.split(' ').filter(|w| w.len() > 2).collect();
    let mut used = HashSet::new();
    let overlap_threshold = threshold.max(OVERLAP_CONFIDENCE_FLOOR);

    for item in pool {
        if found.len() >= MAX_REWARD_SLOTS {
            break;
        }
        let name = normalize_for_search(&item.name);
        if name.is_empty() || used.contains(&name) {
            continue;
        }
        if let Some(pos) = find_phrase(&text, &name) {
            found.push(MatchEntry {
                item: item.clone(),
                pos,
                confidence: 1.0,
                exact: true,
            });
            used.insert(name);
        }
    }
    let exact_count = found.len();
    if exact_count < EXACT_MATCH_SKIP_OVERLAP_COUNT && found.len() < MAX_REWARD_SLOTS {
        for item in pool {
            if found.len() >= MAX_REWARD_SLOTS {
                break;
            }
            let name = normalize_for_search(&item.name);
            if name.is_empty() || used.contains(&name) {
                continue;
            }
            let item_words: Vec<&str> = name.split(' ').filter(|w| w.len() > 2).collect();
            let matched = item_words.iter().filter(|w| words.contains(**w)).count();
            if item_words.is_empty() || matched < MIN_MATCHED_WORDS_FOR_OVERLAP {
                continue;
            }
            let ratio = matched as f64 / item_words.len() as f64;
            if ratio < overlap_threshold {
                continue;
            }
            let first = item_words.iter().find(|w| words.contains(**w)).unwrap();
            let pos = text.find(first).unwrap_or(text.len());
            found.push(MatchEntry {
                item: item.clone(),
                pos,
                confidence: ratio,
                exact: false,
            });
            used.insert(name);
        }
    }
    found.sort_by_key(|m| m.pos);
    let confidence_sum: f64 = found.iter().map(|m| m.confidence).sum();
    let coverage = found.len().min(MAX_REWARD_SLOTS) as f64 * 0.6;
    let exact_boost = found.iter().filter(|m| m.exact).count() as f64 * 0.35;
    MatchResult {
        score: confidence_sum + coverage + exact_boost,
        matches: found,
        exact_count,
    }
}
