//! Slot geometry and strip binarisation (WFHelper `rewardScannerImage`).
use image::{GrayImage, Luma, RgbaImage, imageops};

use super::REFERENCE_UI_SCALE;

/// Normalised rect on the game frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

const fn card(x: f64) -> Rect {
    Rect {
        x,
        y: 0.225,
        w: 0.122,
        h: 0.225,
    }
}

/// Card rows measured on 16:9 at the reference UI scale, indexed by `count - 1`.
pub const FIXED_LAYOUTS: [&[Rect]; 4] = [
    &[card(0.439)],
    &[card(0.3755), card(0.5025)],
    &[card(0.312), card(0.439), card(0.566)],
    &[card(0.245), card(0.372), card(0.499), card(0.626)],
];

const MIN_UI_SCALE: f64 = 0.25;

/// Shrink the off axis about the centre: x on frames wider than 16:9, y on taller ones.
pub fn aspect_scale(width: u32, height: u32) -> (f64, f64) {
    let (w, h) = (width as f64, height as f64);
    (
        ((h * 16.0 / 9.0) / w).min(1.0),
        ((w * 9.0 / 16.0) / h).min(1.0),
    )
}

/// `rects` scaled for the configured interface scale and the frame's aspect ratio.
pub fn correct(rects: &[Rect], width: u32, height: u32, ui_scale: f64) -> Vec<Rect> {
    let ui = ui_scale.clamp(MIN_UI_SCALE, 1.0) / REFERENCE_UI_SCALE;
    let (sx, sy) = aspect_scale(width, height);
    rects
        .iter()
        .map(|r| {
            let (x, y) = (0.5 + (r.x - 0.5) * ui, 0.5 + (r.y - 0.5) * ui);
            let (w, h) = (r.w * ui, r.h * ui);
            Rect {
                x: 0.5 + (x - 0.5) * sx,
                y: 0.5 + (y - 0.5) * sy,
                w: w * sx,
                h: h * sy,
            }
        })
        .collect()
}

/// Title strip of a card: 70 %..88 % of its height.
pub fn title_rect(card: Rect) -> Rect {
    Rect {
        y: card.y + card.h * 0.7,
        h: card.h * 0.18,
        ..card
    }
}

/// Title strips in pixels (x, y, w, h) for `count` (1..=4) cards, clamped to the frame.
pub fn title_rects(
    count: usize,
    width: u32,
    height: u32,
    ui_scale: f64,
) -> Vec<(u32, u32, u32, u32)> {
    let (w, h) = (width as f64, height as f64);
    correct(FIXED_LAYOUTS[count - 1], width, height, ui_scale)
        .into_iter()
        .map(|c| {
            let r = title_rect(c);
            let x = ((w * r.x) as u32).min(width.saturating_sub(24));
            let y = ((h * r.y) as u32).min(height.saturating_sub(24));
            let cw = ((w * r.w) as u32).max(24).min(width - x);
            let ch = ((h * r.h) as u32).max(24).min(height - y);
            (x, y, cw, ch)
        })
        .collect()
}

pub fn otsu(gray: &[u8]) -> u8 {
    let mut hist = [0u64; 256];
    gray.iter().for_each(|&g| hist[g as usize] += 1);
    let total = gray.len() as f64;
    let sum: f64 = hist
        .iter()
        .enumerate()
        .map(|(t, &n)| t as f64 * n as f64)
        .sum();
    let (mut sum_b, mut w_b, mut best, mut thr) = (0.0, 0.0, 0.0, 127u8);
    for (t, &n) in hist.iter().enumerate() {
        w_b += n as f64;
        if w_b == 0.0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0.0 {
            break;
        }
        sum_b += t as f64 * n as f64;
        let (m_b, m_f) = (sum_b / w_b, (sum - sum_b) / w_f);
        let between = w_b * w_f * (m_b - m_f).powi(2);
        if between > best {
            best = between;
            thr = t as u8;
        }
    }
    thr
}

/// sharp `.normalise()`: stretch the 1st..99th percentile to 0..255.
pub fn normalise(gray: &mut GrayImage) {
    let mut hist = [0usize; 256];
    gray.iter().for_each(|&g| hist[g as usize] += 1);
    let pct = |p: f64| {
        let (mut acc, goal) = (0, (gray.len() as f64 * p) as usize);
        (0..256)
            .find(|&i| {
                acc += hist[i];
                acc > goal
            })
            .unwrap_or(255) as f64
    };
    let (lo, hi) = (pct(0.01), pct(0.99));
    if hi > lo {
        gray.iter_mut()
            .for_each(|g| *g = ((*g as f64 - lo) / (hi - lo) * 255.0).clamp(0.0, 255.0) as u8);
    }
}

/// How hard the binariser cuts. `Strict` re-runs Otsu on the pixels above the first cut: bright
/// title text survives while mid-bright card art that merges into it under plain Otsu does not.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cut {
    Otsu,
    Strict,
}

/// Rows `top_frac..top_frac+height_frac` of a title strip -> resized, normalised (1-99 %),
/// thresholded, dark text on white (WFHelper's `binarizeRewardRegion`).
pub fn binarize(strip: &RgbaImage, top_frac: f64, height_frac: f64, cut: Cut) -> RgbaImage {
    let (sw, sh) = strip.dimensions();
    let top = ((sh as f64 * top_frac).round() as u32).min(sh - 1);
    let h = ((sh as f64 * height_frac).round() as u32).clamp(1, sh - top);
    let region = imageops::crop_imm(strip, 0, top, sw, h).to_image();
    let tw = (sw as f64 * 3.0).min(720.0).round().max(8.0) as u32;
    let th = ((h as f64 * tw as f64 / sw as f64).round() as u32).max(1);
    let mut gray = imageops::grayscale(&imageops::resize(
        &region,
        tw,
        th,
        imageops::FilterType::Lanczos3,
    ));

    normalise(&mut gray);

    let mut thr = otsu(&gray);
    let bright = gray.iter().filter(|&&g| g >= thr).count();
    let bright_is_bg = bright * 2 > gray.len();
    if cut == Cut::Strict && !bright_is_bg {
        let above: Vec<u8> = gray.iter().copied().filter(|&g| g >= thr).collect();
        if !above.is_empty() {
            thr = otsu(&above);
        }
    }
    let mono = GrayImage::from_fn(tw, th, |x, y| {
        Luma([if (gray.get_pixel(x, y)[0] >= thr) == bright_is_bg {
            255
        } else {
            0
        }])
    });
    image::DynamicImage::ImageLuma8(mono).to_rgba8()
}
