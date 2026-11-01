//! Riven scanner: card crop -> stat-line boxes -> Paddle per line -> cleaned lines,
//! then `text` parses them into stats; `weapon_label`, `fingerprint` and `session` work on the result.
//! Everything here blocks on inference: call from `spawn_blocking`, never from setup or a sync command.

mod detect;
pub mod fingerprint;
pub mod footer;
mod image;
mod lines;
pub mod locale;
pub mod session;
pub mod text;
pub mod weapon_label;

use std::path::Path;

use ::image::RgbaImage;

use crate::services::ocr_onnx::{self, LineRead};
use locale::CardLabels;
use text::{ParseDiagnostics, RivenStat, looks_like_whole_stat_line, parse_riven_stats};

pub use image::{
    CHAT_CARD, CropRect, ROLL_CARD, Rect, SINGLE_CARD, TextMetrics, align_rect, align_rect_to,
    analyze_text_metrics, card_is_small, frame_canvas_content_rect, locate_cards, locate_dimmed,
    taller,
};
pub use lines::{LOW_CONFIDENCE_THRESHOLD, has_low_confidence_line, merge_split_lines};

/// Retries re-read the same frame through the text-bounds crop, so one is enough.
const MAX_RETRIES: usize = 1;
/// Every riven rolls at least two buffs, with or without a curse.
const MIN_STATS: usize = 2;
const MIN_BUFFS: usize = 2;
const MAX_STATS: usize = 4;

#[derive(Debug)]
pub enum ScanError {
    /// The DLL or a model is missing: the scanner reports it instead of failing.
    Unavailable,
    Failed(String),
}

#[derive(Debug, Default)]
pub struct RivenOcrResult {
    /// Cleaned, merged lines in reading order, one per stat (plus title and footer).
    pub lines: Vec<LineRead>,
    /// The lines joined with `\n`.
    pub text: String,
    /// Lowest confidence over the stat lines; -1 with no lines.
    pub min_confidence: f32,
    pub box_count: usize,
}

/// A read that lost something: too few stats or buffs, more than a riven has (a crop over both roll
/// cards), or under four stats with a signed line that parsed to nothing or no title above the stats.
pub fn is_incomplete_read(stats: &[RivenStat], dropped_whole_line: bool, title_seen: bool) -> bool {
    if stats.is_empty() {
        return false;
    }
    if stats.len() < MIN_STATS || stats.iter().filter(|s| s.positive).count() < MIN_BUFFS {
        return true;
    }
    if stats.len() > MAX_STATS {
        return true;
    }
    stats.len() < MAX_STATS && (dropped_whole_line || !title_seen)
}

#[derive(Debug)]
pub struct CardRead {
    pub result: RivenOcrResult,
    /// `result.text` parsed.
    pub stats: Vec<RivenStat>,
    /// The parse set a signed line aside or saw no title: the read may have lost a stat.
    pub incomplete: bool,
    /// 1 for a clean first read, 2 when the text-bounds retry ran.
    pub attempts: usize,
    /// A stat line stayed under the gate after every attempt: the text is too small to trust.
    pub low_confidence: bool,
    pub footer: footer::Footer,
}

/// Detects the stat lines of a stat-area image and reads them.
pub fn recognize_stat_area(root: &Path, img: &RgbaImage) -> Result<RivenOcrResult, String> {
    let det = ocr_onnx::det(root)?;
    let paddle = ocr_onnx::paddle(root)?;
    let boxes = detect::detect_stat_lines(det, img)?;
    let empty = |box_count| RivenOcrResult {
        min_confidence: -1.0,
        box_count,
        ..Default::default()
    };
    if boxes.is_empty() {
        return Ok(empty(0));
    }
    let crops = detect::extract_and_upscale_crops(img, &boxes);
    if crops.is_empty() {
        return Ok(empty(boxes.len()));
    }
    let valid = paddle
        .recognize(&crops)?
        .into_iter()
        .filter_map(|read| {
            let text = lines::postprocess_text(read.text.trim()).trim().to_string();
            (!read.text.trim().is_empty() && !text.is_empty()).then(|| LineRead {
                confidence: lines::trimmed_line_confidence(&read),
                text,
                char_confidences: Vec::new(),
            })
        })
        .collect();
    let lines = lines::merge_split_lines(valid);
    Ok(RivenOcrResult {
        text: lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        min_confidence: lines::min_confidence(&lines),
        box_count: boxes.len(),
        lines,
    })
}

struct Parsed {
    stats: Vec<RivenStat>,
    incomplete: bool,
}

fn parse(result: &RivenOcrResult, labels: &CardLabels) -> Parsed {
    let mut diag = ParseDiagnostics::default();
    let stats = parse_riven_stats(&labels.to_english(&result.text), Some(&mut diag));
    let dropped = diag
        .dropped_lines
        .iter()
        .any(|l| looks_like_whole_stat_line(l));
    let incomplete = is_incomplete_read(&stats, dropped, diag.title_seen != Some(false));
    Parsed { stats, incomplete }
}

fn nulls(stats: &[RivenStat]) -> usize {
    stats.iter().filter(|s| s.value.is_none()).count()
}

/// True when the retry beats the best so far: it completes an incomplete read, or finds more stats,
/// or ties on count and reads cleaner (fewer null values, then higher confidence).
fn better(new: (&RivenOcrResult, &Parsed), best: (&RivenOcrResult, &Parsed)) -> bool {
    let ((nr, np), (br, bp)) = (new, best);
    if bp.stats.is_empty() {
        return true;
    }
    if bp.incomplete && !np.incomplete && np.stats.len() >= bp.stats.len() {
        return true;
    }
    if np.incomplete != bp.incomplete {
        return false;
    }
    np.stats.len() > bp.stats.len()
        || (!np.stats.is_empty()
            && np.stats.len() == bp.stats.len()
            && (nulls(&np.stats) < nulls(&bp.stats)
                || (nulls(&np.stats) == nulls(&bp.stats) && nr.min_confidence > br.min_confidence)))
}

/// A whole-card crop around a located text block (art above it, frame below): the shape the footer
/// pills are cut from.
pub fn card_around(frame: &RgbaImage, block: image::Rect) -> RgbaImage {
    let (px, up, down) = (block.width / 10, block.height * 9 / 20, block.height / 20);
    let x = block.x.saturating_sub(px);
    let y = block.y.saturating_sub(up);
    let (w, h) = (
        (block.width + 2 * px).min(frame.width() - x),
        (block.height + up + down).min(frame.height() - y),
    );
    ::image::imageops::crop_imm(frame, x, y, w, h).to_image()
}

/// Reads one riven card out of a game frame. `rect` picks the layout (`SINGLE_CARD`, `CHAT_CARD`, `ROLL_CARD`).
/// A first read that is incomplete, has null values or a low-confidence line is retried once on the
/// text-bounds crop, upscaled; the better read wins. `labels` reads the game language's stat names.
pub fn scan_card(
    root: &Path,
    frame: &RgbaImage,
    rect: CropRect,
    labels: &CardLabels,
) -> Result<CardRead, ScanError> {
    scan_card_titled(root, frame, rect, labels, &|_| true)
}

/// [`scan_card`] where a read whose text `has_title` rejects (the fixed band clipped the title line,
/// so the weapon is unknown) also counts as unclean and is retried; a read with a title beats one
/// without it when it finds as many stats.
pub fn scan_card_titled(
    root: &Path,
    frame: &RgbaImage,
    rect: CropRect,
    labels: &CardLabels,
    has_title: &dyn Fn(&str) -> bool,
) -> Result<CardRead, ScanError> {
    if !ocr_onnx::riven_ocr_onnx_available(root) {
        return Err(ScanError::Unavailable);
    }
    let crops = image::crop_stat_image(frame, rect);
    let mut best: Option<(RivenOcrResult, Parsed)> = None;
    let mut last_err = None;
    let mut attempts = 0;
    for attempt in 0..=MAX_RETRIES {
        // Rereading identical pixels cannot improve, so a retry switches to the text-bounds crop.
        // The first read keeps the fixed band and only scales it: a short band drops its curse
        // line silently, so it never reaches the retry gate.
        let scan_image = if attempt == 0 {
            image::upscale(
                &crops.stat,
                image::stat_crop_upscale_factor(crops.stat.height()),
            )
        } else {
            match image::crop_stat_area_fallback(&crops.card) {
                Some(f) => image::upscale(&f.image, f.upscale_factor),
                None => crops.stat.clone(),
            }
        };
        attempts += 1;
        match recognize_stat_area(root, &scan_image) {
            Ok(read) => {
                let parsed = parse(&read, labels);
                let titled = has_title(&read.text);
                let clean = titled
                    && parsed.stats.len() >= MIN_STATS
                    && !parsed.incomplete
                    && !parsed.stats.iter().any(|s| s.value.is_none())
                    && !has_low_confidence_line(&read.lines);
                let wins = best.as_ref().is_none_or(|(br, bp)| {
                    let best_titled = has_title(&br.text);
                    if titled != best_titled {
                        return titled && parsed.stats.len() >= bp.stats.len();
                    }
                    better((&read, &parsed), (br, bp))
                });
                if wins {
                    best = Some((read, parsed));
                }
                if clean {
                    break;
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    let (result, parsed) = best.ok_or_else(|| ScanError::Failed(last_err.unwrap_or_default()))?;
    let footer = footer::read_footer(root, &crops.card);
    Ok(CardRead {
        footer,
        low_confidence: parsed.stats.len() >= MIN_STATS && has_low_confidence_line(&result.lines),
        attempts,
        incomplete: parsed.incomplete,
        stats: parsed.stats,
        result,
    })
}

#[cfg(test)]
mod scan_tests;
#[cfg(test)]
mod tests;
