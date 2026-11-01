//! The card footer: mastery rank ("PM 10" / "MR 10") and cycle count ("⟳ 39"). Both are digits only,
//! so each pill is cut on its own and read for its last digit run, whatever the label language.

use std::path::Path;

use ::image::{Rgb, RgbImage, RgbaImage};

use super::image::{Rect, analyze_text_metrics, upscale};
use crate::services::ocr_onnx::{self, RgbCrop};

/// Rivens unlock at MR 8 and top out at 16; the cycle count is a plain counter.
const MR_RANGE: std::ops::RangeInclusive<u32> = 8..=16;
const ROLLS_MAX: u32 = 9999;
/// Footer characters the recognizer is less sure of than this are the cycle icon, not digits.
const ICON_CONFIDENCE: f32 = 0.75;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Footer {
    pub mr: Option<u32>,
    pub rolls: Option<u32>,
}

/// (x0, x1, y0, y1) as fractions of the text bounds: the MR pill (left) and the cycle pill (right).
const MR_PILL: (f32, f32, f32, f32) = (0.15, 0.38, 0.82, 0.90);
const ROLLS_PILL: (f32, f32, f32, f32) = (0.71, 0.88, 0.82, 0.90);

fn pill(card: &RgbaImage, b: (u32, u32, u32, u32), f: (f32, f32, f32, f32)) -> Option<RgbCrop> {
    let (left, top, w, h) = b;
    let at = |o: u32, size: u32, a: f32| o + (size as f32 * a) as u32;
    let (x0, x1) = (at(left, w, f.0), at(left, w, f.1).min(card.width()));
    let (y0, y1) = (at(top, h, f.2), at(top, h, f.3).min(card.height()));
    if x1 <= x0 + 8 || y1 <= y0 + 4 {
        return None;
    }
    let sub = ::image::imageops::crop_imm(card, x0, y0, x1 - x0, y1 - y0).to_image();
    let big = upscale(&sub, 3);
    let rgb = RgbImage::from_fn(big.width(), big.height(), |x, y| {
        let p = big.get_pixel(x, y).0;
        Rgb([p[0], p[1], p[2]])
    });
    Some(RgbCrop {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    })
}

/// The last run of digits in a read ("PM 10" -> 10, "C 39" -> 39).
pub fn last_number(text: &str) -> Option<u32> {
    let end = text.rfind(|c: char| c.is_ascii_digit())? + 1;
    let start = text[..end]
        .rfind(|c: char| !c.is_ascii_digit())
        .map_or(0, |i| i + 1);
    text[start..end].parse().ok()
}

/// The footer line as the stat reader sees it: both pills fused into one digit run ("PM 1040"). MR
/// is 8-9 or 10-16, so the split is unambiguous. A fallback for a crop the pills could not be cut from.
pub fn footer_from_text(text: &str) -> Footer {
    let Some(line) = text.lines().last().filter(|l| !l.contains('%')) else {
        return Footer::default();
    };
    let digits: String = line.chars().filter(char::is_ascii_digit).collect();
    let mr_len = if digits.starts_with('1') { 2 } else { 1 };
    let Some(mr) = digits.get(..mr_len).and_then(|d| d.parse().ok()) else {
        return Footer::default();
    };
    // A card never rerolled has no cycle pill: the digits are the MR alone.
    let rolls = digits.get(mr_len..).and_then(|d| d.parse::<u32>().ok());
    Footer {
        mr: Some(mr).filter(|v| MR_RANGE.contains(v)),
        rolls: rolls.filter(|v| *v <= ROLLS_MAX),
    }
}

/// A read without the cycle icon, which the recognizer takes for a stray digit ("PM10540", "91"):
/// characters it is unsure of go, except the last, which is the cycle count however unsure.
/// shortcut: a genuinely doubtful digit is dropped too; upgrade by cutting the icon off the pill.
fn without_icon(r: &ocr_onnx::LineRead) -> String {
    let last = r.char_confidences.len().saturating_sub(1);
    r.text
        .chars()
        .zip(&r.char_confidences)
        .enumerate()
        .filter(|(i, (_, c))| **c >= ICON_CONFIDENCE || *i == last)
        .map(|(_, (ch, _))| ch)
        .collect()
}

/// A footer pill row is about this fraction of the located card's width tall.
const STRIP_HEIGHT: f32 = 0.075;
/// Strips tried, each moved up half its height: the located block's bottom is off by a frame glow.
const STRIPS: u32 = 8;

fn strip_crop(frame: &RgbaImage, (x, y, w, h): (u32, u32, u32, u32)) -> RgbCrop {
    let big = upscale(
        &::image::imageops::crop_imm(frame, x, y, w, h).to_image(),
        3,
    );
    let rgb = RgbImage::from_fn(big.width(), big.height(), |x, y| {
        let p = big.get_pixel(x, y).0;
        Rgb([p[0], p[1], p[2]])
    });
    RgbCrop {
        width: rgb.width(),
        height: rgb.height(),
        data: rgb.into_raw(),
    }
}

/// Reads the footer as one line off the bottom of a located card (`card` in frame pixels): the pills'
/// place depends on the crop, the located text block does not. The strip is wider than the block and
/// slid up until one reads as "MR + cycles".
pub fn read_strip(root: &Path, frame: &RgbaImage, card: Rect) -> Footer {
    let pad = card.width / 20;
    let x = card.x.saturating_sub(pad);
    let w = (card.width + 2 * pad).min(frame.width() - x);
    let h = ((card.width as f32 * STRIP_HEIGHT) as u32).max(8);
    let bottom = (card.y + card.height).min(frame.height());
    let boxes: Vec<_> = (0..STRIPS)
        .filter_map(|i| {
            let y = bottom.checked_sub(h + i * h / 2)?;
            Some((x, y, w, h))
        })
        .collect();
    let crops: Vec<_> = boxes.iter().map(|b| strip_crop(frame, *b)).collect();
    let reads = match ocr_onnx::paddle(root).and_then(|p| p.recognize(&crops)) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("riven footer strip read failed: {e}");
            return Footer::default();
        }
    };
    let footers: Vec<Footer> = reads
        .iter()
        .map(|r| {
            let text = without_icon(r);
            tracing::debug!("riven footer strip reads: {:?} -> {text:?}", r.text);
            footer_from_text(&text)
        })
        .collect();
    // A card never rerolled prints the MR pill alone: take it when no strip reads both.
    footers
        .iter()
        .find(|f| f.mr.is_some() && f.rolls.is_some())
        .or_else(|| footers.iter().find(|f| f.mr.is_some()))
        .copied()
        .unwrap_or_default()
}

pub fn read_footer(root: &Path, card: &RgbaImage) -> Footer {
    let Some(b) = analyze_text_metrics(card, 0.08).bounds else {
        return Footer::default();
    };
    let b = (b.left, b.top, b.width, b.height);
    let crops: Vec<_> = [MR_PILL, ROLLS_PILL]
        .into_iter()
        .filter_map(|f| pill(card, b, f))
        .collect();
    let reads = match ocr_onnx::paddle(root).and_then(|p| p.recognize(&crops)) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("riven footer read failed: {e}");
            return Footer::default();
        }
    };
    tracing::debug!(
        "riven footer reads: {:?}",
        reads.iter().map(|r| &r.text).collect::<Vec<_>>()
    );
    let n = |i: usize| reads.get(i).and_then(|r| last_number(&without_icon(r)));
    Footer {
        mr: n(0).filter(|v| MR_RANGE.contains(v)),
        rolls: n(1).filter(|v| *v <= ROLLS_MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_last_digit_run() {
        assert_eq!(last_number("PM 10"), Some(10));
        assert_eq!(last_number("C 39"), Some(39));
        assert_eq!(last_number("O"), None);
        assert_eq!(last_number("3 PM 10"), Some(10));
    }

    #[test]
    fn splits_the_fused_footer_line() {
        let got = |t| footer_from_text(t);
        assert_eq!(
            (
                got("x
PM 1040")
                .mr,
                got("x
PM 1040")
                .rolls
            ),
            (Some(10), Some(40))
        );
        assert_eq!((got("PM 839").mr, got("PM 839").rolls), (Some(8), Some(39)));
        assert_eq!(
            got("PM 10"),
            Footer {
                mr: Some(10),
                rolls: None
            }
        );
        assert_eq!(
            got("PM 9"),
            Footer {
                mr: Some(9),
                rolls: None
            }
        );
        assert_eq!(got("+145% Poison"), Footer::default());
        assert_eq!(got("Orthos Toxitis"), Footer::default());
    }

    #[cfg(windows)]
    #[test]
    fn reads_mr_and_rolls_off_the_french_card() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let card = ::image::open(root.join("tests/fixtures/riven/initial-card-orthos-fr.png"))
            .expect("fixture")
            .to_rgba8();
        let f = read_footer(&root.join("resources"), &card);
        assert_eq!(
            f,
            Footer {
                mr: Some(10),
                rolls: Some(39)
            }
        );
    }
}
