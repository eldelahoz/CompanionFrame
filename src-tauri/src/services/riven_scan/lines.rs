//! Text clean-up of the recognizer's lines: known misreads, wrapped stat names and the confidence gate.
//! Port of the text half of WFHelper's `rivenOcrOnnx.ts`. Meaning (value, sign, stat) is T14's job.

use std::sync::LazyLock;

use regex::Regex;

use crate::services::ocr_onnx::LineRead;

/// Confidence below which a stat line is considered unreliable.
pub const LOW_CONFIDENCE_THRESHOLD: f32 = 0.8;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

// The reference's `\b` is ASCII-only; `(?-u:\b)` keeps that.
static MISREADS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        // Asterisk-minus artifact: "*-74,2%" -> "-74,2%"
        (r"\*-", "-"),
        // "> " before an uppercase letter: ">Impact" -> "Impact"
        (r">([A-Z])", "$1"),
        // CamelCase boundaries
        (r"%([A-Z])", "% $1"),
        (r"([a-z])([A-Z])", "$1 $2"),
        // Word joins
        (r"Damageto", "Damage to"),
        (r"(?-u:\b)for([A-Z])", "for $1"),
        (r"([0-9])for(?-u:\b)", "$1 for"),
        // Dropped 'x' prefix on multiplier lines
        (
            r"(?m)^\*?[A-Za-z]?([0-9][,.]?[0-9]*)\s*Damage\s+to(?-u:\b)",
            "x$1 Damage to",
        ),
        // Letter misreads
        (r"Mmpact", "Impact"),
        (r"%\s*mpact", "% Impact"),
        // Sign + 'i' + digits -> sign + '1' + digits: "+i29,1%" -> "+129,1%"
        (r"([+-])i([0-9])", "${1}1${2}"),
        // x-multiplier: xi/xl -> x1
        (r"(?-u:\b)x[il]([,.])", "x1$1"),
        (r"([0-9])i([,.])", "${1}1${2}"),
        // Spurious dots: "197.,9%" -> "197,9%"
        (r"([0-9])\.,([0-9])", "$1,$2"),
        (r"([0-9])\.\.([0-9])", "$1.$2"),
        // Space between digits: "+1 56,2%" -> "+156,2%"
        (r"([0-9]) ([0-9])", "$1$2"),
        (r"(?-u:\b)Aditional(?-u:\b)", "Additional"),
    ]
    .into_iter()
    .map(|(p, r)| (re(p), r))
    .collect()
});

/// Deterministic corrections for known PaddleOCR CH misreads (`postprocess_ocr_text`).
pub fn postprocess_text(text: &str) -> String {
    MISREADS.iter().fold(text.to_string(), |t, (re, rep)| {
        re.replace_all(&t, *rep).into_owned()
    })
}

// The element icon after a whole value decodes as one non-ASCII glyph; on 263 benchmark cards that
// glyph alone held a correct "+167,3% Impact" line at 0.799. The reference's lookahead for a Latin
// letter after the glyph is checked by hand (the regex crate has none).
static ICON_GLYPH_AFTER_VALUE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^([+\-\u{2013}x\u{00d7}]?\s*[0-9](?:[0-9.,]*[0-9])?(?:%\s*|\s+))([\u{0080}-\u{ffff}])")
});

/// Mean confidence of the trimmed line. The crop padding decodes as edge spaces at 0.5-0.7 that
/// `trim` drops (counted, they pulled a correct "+86,5% Heat" icon line under the gate), and the
/// icon after the value is left out.
pub fn trimmed_line_confidence(read: &LineRead) -> f32 {
    let confs = &read.char_confidences;
    let chars: Vec<char> = read.text.chars().collect();
    if confs.len() != chars.len() {
        return read.confidence;
    }
    let start = chars.iter().take_while(|c| c.is_whitespace()).count();
    let end = chars.len() - chars.iter().rev().take_while(|c| c.is_whitespace()).count();
    if end <= start {
        return read.confidence;
    }
    let mut kept: Vec<f32> = confs[start..end].to_vec();
    let trimmed: String = chars[start..end].iter().collect();
    if let Some(m) = ICON_GLYPH_AFTER_VALUE.captures(&trimmed) {
        let (prefix, glyph) = (m[1].chars().count(), m[2].chars().count());
        let after = trimmed.chars().skip(prefix + glyph).collect::<String>();
        if after
            .trim_start()
            .starts_with(|c: char| c.is_ascii_alphabetic())
        {
            kept.drain(prefix..prefix + glyph);
        }
    }
    if kept.is_empty() {
        return read.confidence;
    }
    kept.iter().sum::<f32>() / kept.len() as f32
}

const SPLIT_STAT_TAILS: [&str; 21] = [
    "slide attack",
    "for slide attack",
    "for slide",
    "count chance",
    "combo count chance",
    "combo count",
    "gain combo count",
    "damage",
    "efficiency",
    "attack efficiency",
    "capacity",
    "heavy attacks",
    "for heavy attacks",
    "x2 for heavy attacks",
    "for bows",
    "x2 for bows",
    "duration",
    "speed",
    "maximum",
    "recoil",
    "chance",
];

const SPLIT_STAT_HEADS: [&str; 22] = [
    "critical chance",
    "critical chance for",
    "additional combo",
    "additional combo count",
    "aditional combo",
    "aditional combo count",
    "chance to gain",
    "chance to gain combo",
    "finisher",
    "melee",
    "heavy attack",
    "heavy",
    "magazine",
    "fire rate",
    "fire rate x2",
    "fire rate x2 for",
    "critical chance x2",
    "critical chance x2 for",
    "status",
    "reload",
    "ammo",
    "weapon",
];

static VALUE_PREFIX: LazyLock<Regex> = LazyLock::new(|| re(r"^[+\-x]?[0-9.,]+%?\s*"));
static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| re(r"[^a-z0-9 ]"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));
// A lone CJK glyph at 0.04-0.46 confidence (263 benchmark screenshots) is a mark, not text, and can
// sit between the two halves of a wrapped name.
static GLYPH_JUNK_LINE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^[^A-Za-z0-9%+\-\u{2013}\u{00d7}]*$"));

fn norm_for_merge(s: &str) -> String {
    let lower = s.to_lowercase().replace(['(', ')'], "");
    SPACES.replace_all(lower.trim(), " ").into_owned()
}

fn clean(s: &str) -> String {
    NON_ALNUM.replace_all(s, "").trim().to_string()
}

/// Merges consecutive fragments of a known multi-word stat ("-41,2% Heavy Attack" + "Efficiency").
pub fn merge_split_lines(lines: Vec<LineRead>) -> Vec<LineRead> {
    let mut merged = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let norm = norm_for_merge(&line.text);
        let stat_part = clean(VALUE_PREFIX.replace(&norm, "").trim());
        // Tolerate a right-truncated head ("magazin"): the stat crop clips the card edge.
        let head = SPLIT_STAT_HEADS.contains(&stat_part.as_str())
            || (stat_part.len() >= 4 && SPLIT_STAT_HEADS.iter().any(|h| h.starts_with(&stat_part)));
        let mut next = i + 1;
        while head && next < lines.len() && GLYPH_JUNK_LINE.is_match(&lines[next].text) {
            next += 1;
        }
        if head
            && next < lines.len()
            && SPLIT_STAT_TAILS.contains(&clean(&norm_for_merge(&lines[next].text)).as_str())
        {
            let tail = &lines[next];
            merged.push(LineRead {
                text: format!("{} {}", line.text.trim_end(), tail.text.trim_start()),
                confidence: line.confidence.min(tail.confidence),
                char_confidences: Vec::new(),
            });
            i = next + 1;
            continue;
        }
        merged.push(line.clone());
        i += 1;
    }
    merged
}

static STAT_START: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^[+\-x\u{00d7}]"));
static GATED_LINE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^[+-]|^[x\u{00d7}]\s*[0-9]"));

/// Only value-prefixed lines gate confidence; `x` needs a digit so the garbled MR badge cannot veto a good read.
pub fn has_low_confidence_line(lines: &[LineRead]) -> bool {
    lines
        .iter()
        .filter(|l| GATED_LINE.is_match(l.text.trim()))
        .any(|l| l.confidence < LOW_CONFIDENCE_THRESHOLD)
}

/// Lowest confidence over the stat lines (value-prefixed; the title, MR footer and continuations
/// are excluded), else over every line; -1 with no lines.
pub fn min_confidence(lines: &[LineRead]) -> f32 {
    let stats = || lines.iter().filter(|l| STAT_START.is_match(l.text.trim()));
    let pick = if stats().next().is_some() {
        stats().map(|l| l.confidence).fold(f32::MAX, f32::min)
    } else {
        lines.iter().map(|l| l.confidence).fold(f32::MAX, f32::min)
    };
    if lines.is_empty() { -1.0 } else { pick }
}

/// Lines that start with a value (`+12%`, `-3,4%`, `x1,5`): one per stat, wrapped names already merged.
#[cfg(all(test, windows))]
pub fn stat_line_count(lines: &[LineRead]) -> usize {
    lines
        .iter()
        .filter(|l| GATED_LINE.is_match(l.text.trim()))
        .count()
}
