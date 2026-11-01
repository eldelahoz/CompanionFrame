//! Per-slot reading (WFHelper `rewardScannerSlotScan`): OCR the title strip in bands and join the
//! lines, since names wrap to two lines in 3/4-player layouts.
use image::{RgbaImage, imageops};

use super::RewardReader;
use super::counter::{find_cards, guess_cards};
use super::image::{Cut, binarize, title_rects};
use super::matcher::{Candidate, Mode, match_items, rank_reward_candidates};
use super::names::PoolItem;

/// Drops 1-char OCR noise tokens but keeps "&" (for "Cobra & Crane Prime ...") and a leading
/// bonus count: "2 X Forma" reads "2X Forma", the pool's spelling.
pub fn clean(text: &str) -> String {
    let mut words = text.split_whitespace().peekable();
    let mut count = None;
    if let Some(n) = words.next_if(|w| w.len() <= 2 && w.chars().all(|c| c.is_ascii_digit()))
        && words.next_if(|w| w.eq_ignore_ascii_case("x")).is_some()
    {
        count = Some(format!("{n}X"));
    }
    let rest = text
        .split_whitespace()
        .skip(if count.is_some() { 2 } else { 0 });
    count
        .into_iter()
        .chain(
            rest.filter(|w| *w == "&" || w.chars().filter(|c| c.is_alphanumeric()).count() > 1)
                .map(str::to_owned),
        )
        .collect::<Vec<_>>()
        .join(" ")
}

/// WFHelper's `joinRewardLines`: cleaned band texts, empties dropped, space-joined.
pub fn join_lines<S: AsRef<str>>(parts: &[S]) -> String {
    parts
        .iter()
        .map(|p| clean(p.as_ref()))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_band(r: &dyn RewardReader, strip: &RgbaImage, top: f64, height: f64, cut: Cut) -> String {
    clean(&r.ocr_lines(&binarize(strip, top, height, cut)).join(" "))
}

/// Candidate texts for one title strip, duplicates dropped: top+bottom bands joined, the whole
/// strip, and the joined bands with a strict top cut (title text that card art fuses into under
/// plain Otsu, e.g. the wrapped "Xaku Prime Chassis Blueprint"), and the raw-strip read of readers
/// that have one (ONNX).
pub fn read_slot(r: &dyn RewardReader, strip: &RgbaImage) -> Vec<String> {
    let t = std::time::Instant::now();
    let bottom = read_band(r, strip, 0.42, 0.58, Cut::Otsu);
    let joined = join_lines(&[read_band(r, strip, 0.0, 0.58, Cut::Otsu), bottom.clone()]);
    let whole = read_band(r, strip, 0.0, 1.0, Cut::Otsu);
    // The strict cut can drop the dimmer second line, so its top band joins the Otsu bottom.
    let strict = join_lines(&[read_band(r, strip, 0.0, 0.58, Cut::Strict), bottom]);
    let bands = t.elapsed();
    let raw = clean(&r.raw_lines(strip).join(" "));
    tracing::debug!(?bands, raw = ?(t.elapsed() - bands), "reward scan: slot read");
    let mut out: Vec<String> = Vec::new();
    for t in [joined, whole, strict, raw] {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

#[derive(Clone)]
pub struct SlotRead {
    pub index: usize,
    pub candidates: Vec<String>,
}

#[derive(Clone)]
pub struct FrameScan {
    pub cards: usize,
    pub slots: Vec<SlotRead>,
}

/// Counts the cards on a game frame and reads each title. `None` when no card bar is found.
pub fn scan_frame(r: &dyn RewardReader, frame: &RgbaImage, ui_scale: f64) -> Option<FrameScan> {
    let t = std::time::Instant::now();
    let (mut cards, ui_scale) = find_cards(frame, ui_scale);
    if cards == 0 {
        cards = guess_cards(frame, ui_scale);
    }
    tracing::debug!(elapsed = ?t.elapsed(), cards, "reward scan: cards counted");
    if cards == 0 {
        return None;
    }
    let slots = title_rects(cards, frame.width(), frame.height(), ui_scale)
        .into_iter()
        .enumerate()
        .map(|(index, (x, y, w, h))| SlotRead {
            index,
            candidates: read_slot(r, &imageops::crop_imm(frame, x, y, w, h).to_image()),
        })
        .collect();
    Some(FrameScan { cards, slots })
}

fn norm(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '&' || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 1.0 = identical after lowercasing/punctuation strip; Levenshtein ratio otherwise.
pub fn similarity(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (norm(a).chars().collect(), norm(b).chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1)
                .min(cur[j - 1] + 1)
                .min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    1.0 - prev[b.len()] as f64 / a.len().max(b.len()).max(1) as f64
}

// A containment match only clears its tier above the substring score clamp.
const SUBSTRING_SLOT_GATE: f64 = 0.92;

fn usable(c: &Candidate) -> bool {
    let Some(item) = &c.item else { return false };
    let name = item.name.trim();
    if name.split_whitespace().count() <= 1 && name.chars().count() < 5 {
        return c.mode == Mode::Exact && c.confidence >= 0.99;
    }
    match c.mode {
        Mode::Exact => c.confidence >= 0.98,
        Mode::Substring => c.confidence >= SUBSTRING_SLOT_GATE,
        _ => c.confidence >= 0.86,
    }
}

/// Items a slot's candidate texts match, best first (WFHelper `readSlotTitle` ranking and gate).
/// Empty when nothing clears the gate.
pub fn match_slot(texts: &[String], pool: &[PoolItem]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = texts
        .iter()
        .flat_map(|t| rank_reward_candidates(t, pool, 4))
        .filter(usable)
        .collect();
    // A clipped component name can be an exact base Blueprint match in the other read.
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(b.confidence.total_cmp(&a.confidence))
            .then_with(|| match (a.mode, b.mode, &a.item, &b.item) {
                (Mode::Exact, Mode::Exact, Some(x), Some(y)) => y.name.len().cmp(&x.name.len()),
                _ => std::cmp::Ordering::Equal,
            })
    });
    out
}

/// Reward band of the frame read as one text (WFHelper `FALLBACK_BAND`, `balanced[1]`).
const FALLBACK_TOP: f64 = 0.36;
const FALLBACK_HEIGHT: f64 = 0.4;
/// WFHelper's default `matchThreshold`.
const FALLBACK_THRESHOLD: f64 = 0.74;

/// Text fallback for a frame whose slots matched nothing (WFHelper `scanRewardFallbackText`): read
/// the whole reward band and find whole item names in it. Returns the read text and the English
/// names in screen order.
pub fn fallback_names(
    r: &dyn RewardReader,
    frame: &RgbaImage,
    pool: &[PoolItem],
) -> (String, Vec<String>) {
    if frame.height() < 2 || frame.width() < 2 {
        return (String::new(), Vec::new());
    }
    let text = clean(
        &r.ocr_lines(&binarize(frame, FALLBACK_TOP, FALLBACK_HEIGHT, Cut::Otsu))
            .join(" "),
    );
    let names = match_items(&text, FALLBACK_THRESHOLD, pool)
        .matches
        .into_iter()
        .map(|m| m.item.english().to_owned())
        .collect();
    (text, names)
}
