//! PaddleOCR as a second reward reader (WFHelper `rewardOcrOnnx`): rows are found on the Otsu mono
//! strip but recognised from the raw RGB, which keeps glare-thinned strokes. It rescues the small
//! 1080x607 text Windows OCR garbles; on a machine without the ONNX runtime it is simply not built.
use image::{GrayImage, RgbImage, RgbaImage, imageops};

use super::RewardReader;
use super::image::{normalise, otsu};
use crate::services::ocr_onnx::{Paddle, RgbCrop};

// One game text line is ~17-22 px at 1080p => ~51-66 px at the 720 px strip width; taller
// segments are wrapped lines merged together and get split at the ink valley.
const MAX_LINE_HEIGHT: u32 = 72;
const MIN_LINE_HEIGHT: u32 = 10;
const ROW_GAP_TOLERANCE: u32 = 2;
const MIN_ROW_CONFIDENCE: f32 = 0.55;

/// Row span `y1..=y2`.
type Rows = (u32, u32);

/// Text rows by horizontal ink projection (`ink[y]` = ink pixels in row y); over-tall segments
/// split at the weakest row of their middle half.
pub fn split_rows(ink: &[u32], width: u32) -> Vec<Rows> {
    let min_ink = 3.max((width as f64 * 0.015) as u32);
    let mut segs: Vec<Rows> = Vec::new();
    let (mut start, mut gap) = (None, 0);
    for (y, &n) in ink.iter().enumerate() {
        let y = y as u32;
        if n >= min_ink {
            start.get_or_insert(y);
            gap = 0;
        } else if let Some(s) = start {
            gap += 1;
            if gap > ROW_GAP_TOLERANCE {
                segs.push((s, y - gap));
                start = None;
                gap = 0;
            }
        }
    }
    if let Some(s) = start {
        segs.push((s, ink.len() as u32 - 1));
    }

    let mut out = Vec::new();
    let mut queue: std::collections::VecDeque<Rows> = segs.into();
    while let Some((y1, y2)) = queue.pop_front() {
        let h = y2 - y1 + 1;
        if h <= MAX_LINE_HEIGHT {
            out.push((y1, y2));
            continue;
        }
        let (from, to) = (y1 + h / 4, y1 + h * 3 / 4);
        let valley = (from..=to).min_by_key(|&y| ink[y as usize]).unwrap_or(from);
        let peak = (y1..=y2).map(|y| ink[y as usize]).max().unwrap_or(0);
        if ink[valley as usize] as f64 >= peak as f64 * 0.8 || valley == y1 || valley == y2 {
            out.push((y1, y2));
            continue;
        }
        queue.push_front((valley + 1, y2));
        queue.push_front((y1, valley - 1));
    }
    out.retain(|(a, b)| b - a + 1 >= MIN_LINE_HEIGHT);
    out.sort_unstable();
    out
}

/// Restores spaces the model drops at case boundaries and keeps only name characters. Accented
/// and non-Latin letters stay (the Latin model reads "Châssis"); the matcher folds accents.
pub fn clean_row_text(text: &str) -> String {
    let mut spaced = String::new();
    let mut prev = ' ';
    for c in text.chars() {
        if (prev.is_lowercase() || prev.is_ascii_digit()) && c.is_uppercase() {
            spaced.push(' ');
        }
        spaced.push(c);
        prev = c;
    }
    spaced
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "&'- ".contains(c) {
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

/// Raw RGB crops of each text row of a title strip, trimmed to their ink columns.
fn row_crops(strip: &RgbaImage) -> Vec<RgbCrop> {
    let (sw, sh) = strip.dimensions();
    if sw < 8 || sh < 8 {
        return vec![];
    }
    let tw = (sw as f64 * 3.0).min(720.0).round().max(8.0) as u32;
    let th = ((sh as f64 * tw as f64 / sw as f64).round() as u32).max(1);
    let rgb: RgbImage = image::DynamicImage::ImageRgba8(imageops::resize(
        strip,
        tw,
        th,
        imageops::FilterType::Lanczos3,
    ))
    .to_rgb8();
    let mut gray: GrayImage = imageops::grayscale(&rgb);
    normalise(&mut gray);
    let thr = otsu(&gray);
    let is_ink = |x: u32, y: u32| gray.get_pixel(x, y)[0] >= thr;
    let ink: Vec<u32> = (0..th)
        .map(|y| (0..tw).filter(|&x| is_ink(x, y)).count() as u32)
        .collect();

    split_rows(&ink, tw)
        .into_iter()
        .filter_map(|(a, b)| {
            let (y1, y2) = (a.saturating_sub(4), (b + 4).min(th - 1));
            let cols: Vec<u32> = (0..tw)
                .filter(|&x| (y1..=y2).any(|y| is_ink(x, y)))
                .collect();
            let (min_x, max_x) = (*cols.first()?, *cols.last()?);
            let (x1, x2) = (min_x.saturating_sub(8), (max_x + 8).min(tw - 1));
            let crop = imageops::crop_imm(&rgb, x1, y1, x2 - x1 + 1, y2 - y1 + 1).to_image();
            Some(RgbCrop {
                width: crop.width(),
                height: crop.height(),
                data: crop.into_raw(),
            })
        })
        .collect()
}

/// Windows OCR (or any reader) for the binarised bands, plus Paddle over the raw strip.
pub struct OnnxReader<'a, R: RewardReader> {
    pub inner: R,
    pub paddle: &'a Paddle,
}

impl<R: RewardReader> RewardReader for OnnxReader<'_, R> {
    fn ocr_lines(&self, strip: &RgbaImage) -> Vec<String> {
        self.inner.ocr_lines(strip)
    }

    fn raw_lines(&self, strip: &RgbaImage) -> Vec<String> {
        let crops = row_crops(strip);
        if crops.is_empty() {
            return vec![];
        }
        // A failed inference only loses this reader's candidate; the Windows reads still stand.
        let reads = self.paddle.recognize(&crops).unwrap_or_default();
        reads
            .into_iter()
            .filter(|r| r.confidence >= MIN_ROW_CONFIDENCE)
            .map(|r| clean_row_text(&r.text))
            .filter(|t| !t.is_empty())
            .collect()
    }

    fn text_fallback(&self) -> bool {
        false
    }
}

/// No band reads: as the `inner` of an [`OnnxReader`] it makes Paddle the only engine.
pub struct NoBands;

impl RewardReader for NoBands {
    fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
        Vec::new()
    }
}
