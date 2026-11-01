//! Stat-line detection (PP-OCRv5 DBNet) and the crops fed to the recognizer.

use image::imageops::{self, FilterType};
use image::{RgbImage, RgbaImage};

use crate::services::ocr_onnx::{Det, RgbCrop};

/// DBNet's own post-process settings (PaddleOCR / RapidOCR defaults).
const BIN_THRESHOLD: f32 = 0.3;
const BOX_THRESHOLD: f32 = 0.6;
const UNCLIP_RATIO: f32 = 1.6;
/// The shorter side is lifted to this before inference (RapidOCR's `limit_side_len`), the longer is capped.
const MIN_SIDE: u32 = 736;
const MAX_SIDE: u32 = 1920;
const MAX_STAT_CROP_HEIGHT: u32 = 80;
const MIN_OCR_WIDTH: u32 = 1200;
const MAX_UPSCALED_SIDE: u32 = 6000;
const CROP_PAD: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineBox {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

pub struct Prepared {
    /// `[3, h, w]` CHW, BGR, mapped to -1..1.
    pub blob: Vec<f32>,
    pub w: usize,
    pub h: usize,
}

fn round32(v: f32) -> u32 {
    ((v / 32.0).round() as u32).max(1) * 32
}

/// Resizes to multiples of 32 with the shorter side at least `MIN_SIDE`.
pub fn prepare(img: &RgbaImage) -> Prepared {
    let (w, h) = img.dimensions();
    let lift = (MIN_SIDE as f32 / w.min(h).max(1) as f32).max(1.0);
    let scale = lift.min(MAX_SIDE as f32 / w.max(h).max(1) as f32);
    let (nw, nh) = (round32(w as f32 * scale), round32(h as f32 * scale));
    let rgb = RgbImage::from_fn(w, h, |x, y| {
        let p = img.get_pixel(x, y).0;
        image::Rgb([p[0], p[1], p[2]])
    });
    let resized = imageops::resize(&rgb, nw, nh, FilterType::Triangle);
    let plane = (nw * nh) as usize;
    let mut blob = vec![0.0; 3 * plane];
    for (x, y, px) in resized.enumerate_pixels() {
        let at = (y * nw + x) as usize;
        for c in 0..3 {
            blob[c * plane + at] = px[2 - c] as f32 / 127.5 - 1.0;
        }
    }
    Prepared {
        blob,
        w: nw as usize,
        h: nh as usize,
    }
}

/// Boxes from a `[1, 1, h, w]` probability map: binarize, group touching pixels, keep groups whose
/// mean score passes `BOX_THRESHOLD`, grow each by DB's unclip, map back to image pixels, join the
/// boxes of one text row, top to bottom.
pub fn boxes_from_map(dims: &[i64], map: &[f32], (w, h): (u32, u32)) -> Vec<LineBox> {
    let &[1, 1, mh, mw] = dims else {
        return Vec::new();
    };
    let (mh, mw) = (mh as usize, mw as usize);
    if map.len() < mh * mw {
        return Vec::new();
    }
    let (sx, sy) = (w as f32 / mw as f32, h as f32 / mh as f32);
    let mut seen = vec![false; mh * mw];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..mh * mw {
        if seen[start] || map[start] <= BIN_THRESHOLD {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (mw, mh, 0, 0);
        let (mut sum, mut count) = (0.0, 0usize);
        while let Some(i) = stack.pop() {
            let (x, y) = (i % mw, i / mw);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            sum += map[i];
            count += 1;
            for (nx, ny) in [
                (x.wrapping_sub(1), y),
                (x + 1, y),
                (x, y.wrapping_sub(1)),
                (x, y + 1),
            ] {
                if nx < mw && ny < mh && !seen[ny * mw + nx] && map[ny * mw + nx] > BIN_THRESHOLD {
                    seen[ny * mw + nx] = true;
                    stack.push(ny * mw + nx);
                }
            }
        }
        let (bw, bh) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
        // A tall sliver (card-edge glow) is no text line, and would chain the lines it crosses in `merge_rows`.
        if bw.min(bh) < 3.0 || bh > 2.0 * bw || sum / (count as f32) < BOX_THRESHOLD {
            continue;
        }
        // DB's unclip distance: area * ratio / perimeter, on the rectangle.
        let d = bw * bh * UNCLIP_RATIO / (2.0 * (bw + bh));
        let px = |v: f32, s: f32, max: u32| (v * s).round().clamp(0.0, max as f32) as i32;
        let b = LineBox {
            x1: px(x0 as f32 - d, sx, w),
            y1: px(y0 as f32 - d, sy, h),
            x2: px(x1 as f32 + 1.0 + d, sx, w),
            y2: px(y1 as f32 + 1.0 + d, sy, h),
        };
        if b.x2 - b.x1 >= 10 && b.y2 - b.y1 >= 3 {
            out.push(b);
        }
    }
    merge_rows(out)
}

/// DB splits a line at wide word gaps: boxes sharing most of their height are one line again.
/// Returned top to bottom.
fn merge_rows(mut boxes: Vec<LineBox>) -> Vec<LineBox> {
    boxes.sort_by_key(|b| b.y1);
    let mut rows: Vec<LineBox> = Vec::new();
    for b in boxes {
        let same_row = |r: &LineBox| {
            let overlap = r.y2.min(b.y2) - r.y1.max(b.y1);
            overlap * 2 > (r.y2 - r.y1).min(b.y2 - b.y1)
        };
        match rows.iter_mut().find(|r| same_row(r)) {
            Some(r) => {
                *r = LineBox {
                    x1: r.x1.min(b.x1),
                    y1: r.y1.min(b.y1),
                    x2: r.x2.max(b.x2),
                    y2: r.y2.max(b.y2),
                }
            }
            None => rows.push(b),
        }
    }
    rows.sort_by_key(|r| r.y1);
    rows
}

pub fn detect_stat_lines(det: &Det, img: &RgbaImage) -> Result<Vec<LineBox>, String> {
    let p = prepare(img);
    let (dims, map) = det.run(p.blob, p.h, p.w)?;
    Ok(boxes_from_map(&dims, &map, img.dimensions()))
}

/// Padded RGB crops of the boxes, uniformly upscaled so the widest reaches `MIN_OCR_WIDTH`.
pub fn extract_and_upscale_crops(img: &RgbaImage, boxes: &[LineBox]) -> Vec<RgbCrop> {
    let (w, h) = img.dimensions();
    let raw: Vec<RgbImage> = boxes
        .iter()
        .filter_map(|b| {
            let x1 = (b.x1.max(0) as u32).saturating_sub(CROP_PAD);
            let y1 = (b.y1.max(0) as u32).saturating_sub(CROP_PAD);
            let x2 = (b.x2.max(0) as u32 + CROP_PAD).min(w);
            let y2 = (b.y2.max(0) as u32 + CROP_PAD).min(h);
            let (cw, ch) = (x2.saturating_sub(x1), y2.saturating_sub(y1));
            if cw < 20 || !(5..=MAX_STAT_CROP_HEIGHT).contains(&ch) {
                return None;
            }
            Some(RgbImage::from_fn(cw, ch, |x, y| {
                let p = img.get_pixel(x1 + x, y1 + y).0;
                image::Rgb([p[0], p[1], p[2]])
            }))
        })
        .collect();
    let Some(max_w) = raw.iter().map(|c| c.width()).max() else {
        return Vec::new();
    };
    let factor = if max_w >= MIN_OCR_WIDTH {
        1
    } else {
        MIN_OCR_WIDTH.div_ceil(max_w)
    };
    raw.into_iter()
        .map(|c| {
            let c = if factor > 1 {
                imageops::resize(
                    &c,
                    (c.width() * factor).min(MAX_UPSCALED_SIDE),
                    (c.height() * factor).min(MAX_UPSCALED_SIDE),
                    FilterType::Triangle,
                )
            } else {
                c
            };
            RgbCrop {
                width: c.width(),
                height: c.height(),
                data: c.into_raw(),
            }
        })
        .collect()
}
