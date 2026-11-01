//! Card and stat-band crops of a riven screen. Port of WFHelper's `rivenScanImage.ts` (the crop half;
//! the wait-until-painted loop belongs to the scan flow).

use image::RgbaImage;
use image::imageops::{self, FilterType};

/// A crop as fractions of the 16:9 game canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Initial and choice scans use the centered single-card view.
pub const SINGLE_CARD: CropRect = CropRect {
    x: 0.22,
    y: 0.43,
    width: 0.56,
    height: 0.45,
};
pub const CHAT_CARD: CropRect = CropRect {
    x: 0.22,
    y: 0.3,
    width: 0.56,
    height: 0.45,
};
/// Wide on purpose: the aspect trim narrows it back down around the detected text column.
pub const ROLL_CARD: CropRect = CropRect {
    x: 0.36,
    y: 0.416,
    width: 0.28,
    height: 0.434,
};

const CARD_ASPECT: f32 = 287.0 / 433.0;
const MAX_ASPECT_OVERFLOW: f32 = 1.1;
/// Kept width = card width * this margin, so a card sitting off-center still lands inside the trim.
const TRIM_WIDTH_MARGIN: f32 = 1.25;
/// Distrust detected text centers further than this fraction of the rough crop from the middle
/// (likely a bright diorama prop, not the card).
const MAX_CENTER_SHIFT: f32 = 0.2;
const STAT_TOP: f32 = 0.34;
const STAT_BOTTOM: f32 = 0.84;
const STAT_LEFT: f32 = 0.08;
const STAT_RIGHT: f32 = 0.92;
const MIN_CROP: u32 = 30;
/// Fallback trim targets the stat-crop height the recognizer is calibrated for.
const FALLBACK_TARGET_HEIGHT: f32 = 400.0;
/// Height the recognizer reads a stat band reliably at (a 224x162 chat card loses its curse line at 1x).
const RELIABLE_STAT_CROP_HEIGHT: u32 = 320;
const FALLBACK_MAX_UPSCALE: u32 = 3;
const FALLBACK_PAD_FRACTION: f32 = 0.06;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The centered 16:9 canvas of a client-only frame. Menus sit on it, so a non-16:9 window needs
/// the crop base clamped to it on both axes.
/// shortcut: no black-bar detection (`detectGameContentRect` in WFHelper), only needed for whole-screen
/// captures; upgrade when `CaptureKind::Screen` frames must be scanned.
pub fn frame_canvas_content_rect(width: u32, height: u32) -> Rect {
    let cw = width.min((height as f32 * 16.0 / 9.0).round() as u32);
    let ch = height.min((width as f32 * 9.0 / 16.0).round() as u32);
    Rect {
        x: (width - cw) / 2,
        y: (height - ch) / 2,
        width: cw,
        height: ch,
    }
}

/// The pixel rect `crop_rect_content` cuts, clamped into the image.
pub fn rect_pixels(rect: CropRect, content: Rect, (img_w, img_h): (u32, u32)) -> Rect {
    let clamp = |v: f32, min: f32, max: f32, default: f32| {
        if v.is_finite() {
            v.clamp(min, max)
        } else {
            default
        }
    };
    let x = clamp(rect.x, 0.0, 0.98, 0.0);
    let y = clamp(rect.y, 0.0, 0.98, 0.0);
    let w = clamp(rect.width, 0.02, (1.0 - x).max(0.02), 0.2);
    let h = clamp(rect.height, 0.02, (1.0 - y).max(0.02), 0.2);
    let px = (content.x + (content.width as f32 * x).floor() as u32).min(img_w.saturating_sub(1));
    let py = (content.y + (content.height as f32 * y).floor() as u32).min(img_h.saturating_sub(1));
    Rect {
        x: px,
        y: py,
        width: ((content.width as f32 * w).floor() as u32)
            .max(24)
            .min(img_w - px),
        height: ((content.height as f32 * h).floor() as u32)
            .max(24)
            .min(img_h - py),
    }
}

fn crop(img: &RgbaImage, r: Rect) -> RgbaImage {
    imageops::crop_imm(img, r.x, r.y, r.width, r.height).to_image()
}

pub fn crop_rect_content(img: &RgbaImage, rect: CropRect, content: Rect) -> RgbaImage {
    crop(img, rect_pixels(rect, content, img.dimensions()))
}

#[derive(Clone, Copy, Debug)]
pub struct TextBounds {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct TextMetrics {
    /// 0..1: how much the crop looks like a card of text (the scan flow waits for it to settle).
    pub score: f32,
    pub coverage: f32,
    pub bounds: Option<TextBounds>,
}

fn smooth(values: &[f32]) -> Vec<f32> {
    if values.len() <= 2 {
        return values.to_vec();
    }
    (0..values.len())
        .map(|i| {
            let prev = values[i.saturating_sub(1)];
            let next = values[(i + 1).min(values.len() - 1)];
            (prev + values[i] + next) / 3.0
        })
        .collect()
}

fn mean_std(values: &[f32]) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
    (mean, var.sqrt())
}

fn count_groups(values: &[f32], threshold: f32, min_run: usize) -> usize {
    let (mut groups, mut run) = (0, 0);
    for &v in values {
        if v >= threshold {
            run += 1;
        } else {
            if run >= min_run {
                groups += 1;
            }
            run = 0;
        }
    }
    groups + usize::from(run >= min_run)
}

fn find_bounds(values: &[f32], threshold: f32) -> Option<(usize, usize)> {
    let start = values.iter().position(|&v| v >= threshold)?;
    let end = values.iter().rposition(|&v| v >= threshold)?;
    Some((start, end))
}

/// Samples the crop on a coarse grid for light, unsaturated pixels (card text) and bounds them.
/// Half-size cards at low interface scales sit under the default `min_bounds_width_fraction` (0.2).
pub fn analyze_text_metrics(img: &RgbaImage, min_bounds_width_fraction: f32) -> TextMetrics {
    let (width, height) = img.dimensions();
    let none = TextMetrics {
        score: 0.0,
        coverage: 0.0,
        bounds: None,
    };
    if width < 24 || height < 24 {
        return none;
    }
    let cols = width.clamp(48, 320) as usize;
    let rows = height.clamp(32, 160) as usize;
    // Fractional steps: an integer step over a 537 px crop sampled only the left 320 columns.
    let (step_x, step_y) = (width as f32 / cols as f32, height as f32 / rows as f32);
    let (mut row_scores, mut col_scores) = (vec![0f32; rows], vec![0f32; cols]);
    let mut active = 0u32;
    for (sy, row_score) in row_scores.iter_mut().enumerate() {
        let y = ((sy as f32 * step_y) as u32).min(height - 1);
        for (sx, col_score) in col_scores.iter_mut().enumerate() {
            let x = ((sx as f32 * step_x) as u32).min(width - 1);
            let [r, g, b, _] = img.get_pixel(x, y).0;
            let (max_c, min_c) = (r.max(g).max(b), r.min(g).min(b));
            let sat = if max_c == 0 {
                0.0
            } else {
                (max_c - min_c) as f32 / max_c as f32
            };
            if max_c >= 146 && sat <= 0.42 {
                active += 1;
                *row_score += 1.0;
                *col_score += 1.0;
            }
        }
    }
    let (rs, cs) = (smooth(&row_scores), smooth(&col_scores));
    let ((rm, rsd), (cm, csd)) = (mean_std(&rs), mean_std(&cs));
    let row_threshold = (rm + rsd * 0.45).max(2.0);
    let col_threshold = (cm + csd * 0.45).max(2.0);
    let active_rows = rs.iter().filter(|&&v| v >= row_threshold).count();
    let active_cols = cs.iter().filter(|&&v| v >= col_threshold).count();
    let row_groups = count_groups(&rs, row_threshold, 2.max((rows as f32 * 0.03) as usize));
    let coverage = active as f32 / (cols * rows).max(1) as f32;

    let bounds = find_bounds(&rs, row_threshold)
        .zip(find_bounds(&cs, col_threshold))
        .and_then(|((r0, r1), (c0, c1))| {
            let pad_x = 2.max((cols as f32 * 0.04) as i64);
            let pad_y = 2.max((rows as f32 * 0.03) as i64);
            let left = ((c0 as i64 - pad_x) as f32 * step_x).floor().max(0.0) as u32;
            let top = ((r0 as i64 - pad_y) as f32 * step_y).floor().max(0.0) as u32;
            let right = (((c1 as i64 + pad_x + 1) as f32 * step_x).floor() as u32).min(width - 1);
            let bottom = (((r1 as i64 + pad_y + 1) as f32 * step_y).floor() as u32).min(height - 1);
            let bw = (right + 1).saturating_sub(left);
            let bh = (bottom + 1).saturating_sub(top);
            (bw >= 24.max((width as f32 * min_bounds_width_fraction) as u32)
                && bh >= 24.max((height as f32 * 0.12) as u32))
            .then_some(TextBounds {
                left,
                top,
                width: bw,
                height: bh,
            })
        });

    let c01 = |v: f32| v.clamp(0.0, 1.0);
    let score = c01(coverage / 0.08) * 0.28
        + c01(active_rows as f32 / 8f32.max(rows as f32 * 0.16)) * 0.24
        + c01(active_cols as f32 / 24f32.max(cols as f32 * 0.2)) * 0.24
        + c01(row_groups as f32 / 3.0) * 0.24;
    TextMetrics {
        score,
        coverage,
        bounds,
    }
}

/// Rows of the canvas the card locator scans: the stat text of every cycle-screen layout sits here.
const LOCATE_TOP: f32 = 0.5;
const LOCATE_BOTTOM: f32 = 0.82;
/// A card text panel is 0.12..0.5 canvas heights wide (about 0.3 for the 16:9 default UI).
const LOCATE_MIN_WIDTH: f32 = 0.12;
const LOCATE_MAX_WIDTH: f32 = 0.5;
const LOCATE_MAX_GAP: f32 = 0.03;
/// A run wider than this many canvas heights is split again (a card is about 0.3 wide).
const LOCATE_SPLIT_WIDTH: f32 = 0.4;
const LOCATE_SPLIT_GAP: f32 = 0.01;
const LOCATE_SPLIT_DARK: f32 = 0.52;
/// The text of a real card panel spans most of its width; glow and tile grids leave a narrow core.
/// Wrapped stat lines are short, so a card of them measures about 0.67; glow measures under 0.45.
const LOCATE_MIN_TEXT_SPAN: f32 = 0.6;
const LOCATE_MIN_PANEL: f32 = 0.15;
/// Title-to-footer height of a card text panel, as a fraction of the located width (0.70 measured).
/// A shorter block lost its title or first stat to the art or a glare: it grows upward to this.
/// A card text panel narrower than this many canvas heights is a small card (0.39 at the default UI).
const SMALL_CARD_WIDTH: f32 = 0.25;
const PANEL_MIN_ASPECT: f32 = 0.65;

/// Brightness cut-offs of the locator: a card text panel is mostly darker than `dark_max` with
/// text brighter than `text_min` on it.
#[derive(Clone, Copy)]
struct Tuning {
    dark_max: u8,
    text_min: u8,
    /// Share of a column that must be text for it to count as a panel.
    text_share: f32,
}

const BRIGHT: Tuning = Tuning {
    dark_max: 70,
    text_min: 146,
    text_share: 0.03,
};
/// The dimmed card of the compare screen: the game greys its panel and text.
const DIMMED: Tuning = Tuning {
    dark_max: 110,
    text_min: 70,
    text_share: 0.03,
};

fn is_text_pixel(t: Tuning, [r, g, b, _]: [u8; 4]) -> bool {
    let (max_c, min_c) = (r.max(g).max(b), r.min(g).min(b));
    max_c >= t.text_min && (max_c - min_c) as f32 <= 0.42 * max_c as f32
}

fn is_dark_pixel(t: Tuning, [r, g, b, _]: [u8; 4]) -> bool {
    r.max(g).max(b) < t.dark_max
}

/// Card text panels in a cycle-screen frame, brightest text first: a run of columns that are mostly
/// dark with light text on them (a card's text panel), not the glowing crystal or the black backdrop.
/// The dimmed card of the compare screen scores low, so the first box is the card to grade.
pub fn locate_cards(frame: &RgbaImage) -> Vec<Rect> {
    locate_with(frame, BRIGHT)
}

/// The dimmed card of the compare screen (the Current one): the first panel found with the dimmed
/// cut-offs that does not overlap `bright`, the cards `locate_cards` found. The Current card only
/// exists beside the new one, so without a bright card nothing is dimmed (a relic grid is not).
pub fn locate_dimmed(frame: &RgbaImage, bright: &[Rect]) -> Option<Rect> {
    if bright.is_empty() {
        return None;
    }
    locate_with(frame, DIMMED).into_iter().find(|c| {
        bright
            .iter()
            .all(|b| c.x + c.width <= b.x || b.x + b.width <= c.x)
    })
}

fn locate_with(frame: &RgbaImage, tune: Tuning) -> Vec<Rect> {
    let (w, h) = frame.dimensions();
    let canvas = frame_canvas_content_rect(w, h);
    let step = (canvas.height / 270).max(1);
    let (y0, y1) = (
        canvas.y + (canvas.height as f32 * LOCATE_TOP) as u32,
        canvas.y + (canvas.height as f32 * LOCATE_BOTTOM) as u32,
    );
    let rows = ((y1 - y0) / step).max(1) as f32;
    let cols: Vec<(f32, f32)> = (canvas.x..canvas.x + canvas.width)
        .step_by(step as usize)
        .map(|x| {
            let (mut dark, mut text) = (0.0, 0.0);
            for y in (y0..y1.min(h)).step_by(step as usize) {
                let p = frame.get_pixel(x.min(w - 1), y).0;
                dark += f32::from(is_dark_pixel(tune, p));
                text += f32::from(is_text_pixel(tune, p));
            }
            (dark / rows, text / rows)
        })
        .collect();
    // Glyph gaps leave single columns without text, so a column counts when its neighbours do.
    let reach = 3;
    let panel_at = |min_dark: f32| -> Vec<bool> {
        (0..cols.len())
            .map(|i| {
                let near = &cols[i.saturating_sub(reach)..(i + reach + 1).min(cols.len())];
                let n = near.len() as f32;
                near.iter().map(|c| c.0).sum::<f32>() / n >= min_dark
                    && near.iter().map(|c| c.1).sum::<f32>() / n >= tune.text_share
            })
            .collect()
    };
    let panel = panel_at(0.45);
    let px = |i: usize| canvas.x + i as u32 * step;
    let max_gap = (canvas.height as f32 * LOCATE_MAX_GAP / step as f32) as usize;
    let group = |on: &dyn Fn(usize) -> bool, max_gap: usize| {
        let mut runs: Vec<(usize, usize)> = vec![];
        for i in (0..panel.len()).filter(|&i| on(i)) {
            match runs.last_mut() {
                Some(r) if i - r.1 <= max_gap + 1 => r.1 = i,
                _ => runs.push((i, i)),
            }
        }
        runs
    };
    // Glow or a second card beside the panel fuses with it into a run too wide for one card: split
    // it again on a darker panel test and a tight gap.
    let strict = panel_at(LOCATE_SPLIT_DARK);
    let tight = (canvas.height as f32 * LOCATE_SPLIT_GAP / step as f32) as usize;
    let runs: Vec<(usize, usize)> = group(&|i| panel[i], max_gap)
        .into_iter()
        .flat_map(|r| {
            if (px(r.1) - px(r.0)) as f32 / canvas.height as f32 <= LOCATE_SPLIT_WIDTH {
                return vec![r];
            }
            let split = group(&|i| i >= r.0 && i <= r.1 && strict[i], tight);
            if split.is_empty() { vec![r] } else { split }
        })
        .collect();
    let found: Vec<(f32, Rect)> = runs
        .into_iter()
        .filter(|&(a, b)| {
            let width = (px(b) - px(a)) as f32 / canvas.height as f32;
            (LOCATE_MIN_WIDTH..=LOCATE_MAX_WIDTH).contains(&width)
        })
        .map(|(a, b)| {
            let text: f32 = cols[a..=b].iter().map(|c| c.1).sum();
            let rect = Rect {
                x: px(a),
                y: y0,
                width: px(b) - px(a),
                height: y1 - y0,
            };
            (text, rect)
        })
        .collect();
    // A band with a text block on a dark panel (title to footer) outranks one without: bright text on
    // a dark backdrop, such as a dialog beside the card, has none.
    let mut cards: Vec<(bool, f32, Rect)> = found
        .into_iter()
        .map(|(text, band)| {
            let panel = panel_rows(frame, canvas, band, tune);
            (panel != band, text, panel)
        })
        .filter(|(_, _, r)| has_card_text(frame, *r, tune))
        .collect();
    cards.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
    cards.into_iter().map(|c| c.2).collect()
}

/// True when the text of `rect` spans most of its width and sits centred, as a card's stat lines do. A
/// dimmed card is brightened first, so the same light-text metric applies to it.
fn has_card_text(frame: &RgbaImage, rect: Rect, tune: Tuning) -> bool {
    let (w, h) = frame.dimensions();
    let (x, y) = (rect.x.min(w - 1), rect.y.min(h - 1));
    let mut crop =
        ::image::imageops::crop_imm(frame, x, y, rect.width.min(w - x), rect.height.min(h - y))
            .to_image();
    let gain = f32::from(BRIGHT.text_min) / f32::from(tune.text_min);
    if gain > 1.0 {
        for p in crop.pixels_mut() {
            p.0[..3]
                .iter_mut()
                .for_each(|c| *c = (f32::from(*c) * gain).min(255.0) as u8);
        }
    }
    analyze_text_metrics(&crop, 0.08).bounds.is_some_and(|b| {
        let span = rect.width as f32;
        let centre = b.left as f32 + b.width as f32 / 2.0;
        b.width as f32 >= span * LOCATE_MIN_TEXT_SPAN && (centre - span / 2.0).abs() <= span * 0.1
    })
}

/// `card` with its rows replaced by the text block around the band: title down to the footer. The
/// biggest cluster of text rows wins, so a lone bright spot in the art does not stretch it. Keeps the
/// band when no block of at least `LOCATE_MIN_PANEL` canvas heights is found.
fn panel_rows(frame: &RgbaImage, canvas: Rect, card: Rect, tune: Tuning) -> Rect {
    let step = (canvas.height / 270).max(1);
    let at = |f: f32| canvas.y + (canvas.height as f32 * f) as u32;
    let cols = (card.x..card.x + card.width).step_by(step as usize);
    let n = cols.clone().count() as f32;
    let text_rows: Vec<u32> = (at(0.42)..at(0.9).min(frame.height()))
        .step_by(step as usize)
        .filter(|&y| {
            let px = |x| frame.get_pixel(x, y).0;
            let text = cols.clone().filter(|&x| is_text_pixel(tune, px(x))).count();
            let dark = cols.clone().filter(|&x| is_dark_pixel(tune, px(x))).count();
            // Text on the dark panel: the art above it is light all over.
            text as f32 >= n * 0.04 && dark as f32 >= n * 0.45
        })
        .collect();
    let max_gap = (canvas.height as f32 * LOCATE_MAX_GAP) as u32;
    let mut clusters: Vec<Vec<u32>> = vec![];
    for y in text_rows {
        match clusters.last_mut() {
            Some(c) if y - c[c.len() - 1] <= max_gap => c.push(y),
            _ => clusters.push(vec![y]),
        }
    }
    let best = clusters.into_iter().max_by_key(Vec::len);
    match best {
        Some(c) if (c[c.len() - 1] - c[0]) as f32 >= canvas.height as f32 * LOCATE_MIN_PANEL => {
            let (top, bottom) = (c[0], c[c.len() - 1]);
            let height = (bottom - top).max((card.width as f32 * PANEL_MIN_ASPECT).ceil() as u32);
            Rect {
                y: bottom.saturating_sub(height).max(canvas.y),
                height,
                ..card
            }
        }
        _ => card,
    }
}

/// `rect` moved sideways onto the best-scoring card of `frame`; unchanged when no card is located.
pub fn align_rect(frame: &RgbaImage, rect: CropRect) -> CropRect {
    locate_cards(frame)
        .first()
        .map_or(rect, |card| align_rect_to(frame, rect, *card))
}

/// `rect` moved onto one located `card`.
pub fn align_rect_to(frame: &RgbaImage, rect: CropRect, card: Rect) -> CropRect {
    let (w, h) = frame.dimensions();
    let canvas = frame_canvas_content_rect(w, h);
    let centre = ((card.x - canvas.x) as f32 + card.width as f32 / 2.0) / canvas.width as f32;
    // The stat band is STAT_TOP..STAT_BOTTOM of the crop: size and place the crop so it spans the panel.
    let (band, mid) = (STAT_BOTTOM - STAT_TOP, (STAT_TOP + STAT_BOTTOM) / 2.0);
    let height = (card.height as f32 / canvas.height as f32 * 1.4 / band).min(1.0);
    let panel_mid = ((card.y - canvas.y) as f32 + card.height as f32 / 2.0) / canvas.height as f32;
    // A little wider than the card, so the glow beside it stays out of the footer geometry.
    let width = (card.width as f32 / canvas.width as f32 * 1.3).min(rect.width);
    CropRect {
        x: (centre - width / 2.0).max(0.0),
        width,
        y: (panel_mid - mid * height).max(0.0),
        height,
    }
}

/// `rect` grown to `factor` times its height about the same centre: a second look when the first
/// crop missed the title.
pub fn taller(rect: CropRect, factor: f32) -> CropRect {
    let height = (rect.height * factor).min(1.0);
    CropRect {
        y: (rect.y - (height - rect.height) / 2.0).clamp(0.0, 1.0 - height),
        height,
        ..rect
    }
}

/// True when the located card is small in pixels (a chat card, a low interface scale): the text
/// then really is hard to read. A card of the usual size is not "too small".
pub fn card_is_small(frame: &RgbaImage, card: Rect) -> bool {
    let canvas = frame_canvas_content_rect(frame.width(), frame.height());
    (card.width as f32) < canvas.height as f32 * SMALL_CARD_WIDTH
}

/// The stat band of a card crop. Centers on detected text first: letterbox trimming can move the
/// card off center and a geometric crop would then cut the start of every stat line.
fn crop_stat_area(rough: &RgbaImage) -> RgbaImage {
    let (w, h) = rough.dimensions();
    if w < 50 || h < 50 {
        return rough.clone();
    }
    let expected_w = (h as f32 * CARD_ASPECT).floor();
    let (mut trimmed, mut tw) = (None, w);
    if w as f32 > expected_w * MAX_ASPECT_OVERFLOW {
        let keep_w = (w as f32).min((expected_w * TRIM_WIDTH_MARGIN).floor());
        let mut center_x = w as f32 / 2.0;
        if let Some(b) = analyze_text_metrics(rough, 0.2).bounds
            && b.width as f32 <= keep_w
        {
            let c = b.left as f32 + b.width as f32 / 2.0;
            if (c - w as f32 / 2.0).abs() <= w as f32 * MAX_CENTER_SHIFT {
                center_x = c;
            }
        }
        let keep = keep_w as u32;
        let x1 = ((center_x - keep_w / 2.0).round().max(0.0) as u32).min(w - keep);
        tw = keep;
        trimmed = Some(crop(
            rough,
            Rect {
                x: x1,
                y: 0,
                width: keep,
                height: h,
            },
        ));
    }
    let src = trimmed.as_ref().unwrap_or(rough);
    let (sy0, sy1) = (
        (h as f32 * STAT_TOP) as u32,
        (h as f32 * STAT_BOTTOM) as u32,
    );
    let (sx0, sx1) = (
        (tw as f32 * STAT_LEFT) as u32,
        (tw as f32 * STAT_RIGHT) as u32,
    );
    if sx1 - sx0 < MIN_CROP || sy1 - sy0 < MIN_CROP {
        return rough.clone();
    }
    crop(
        src,
        Rect {
            x: sx0,
            y: sy0,
            width: sx1 - sx0,
            height: sy1 - sy0,
        },
    )
}

pub struct StatImageCrop {
    pub card: RgbaImage,
    pub stat: RgbaImage,
}

pub fn crop_stat_image(frame: &RgbaImage, rect: CropRect) -> StatImageCrop {
    let (w, h) = frame.dimensions();
    let card = crop_rect_content(frame, rect, frame_canvas_content_rect(w, h));
    let stat = crop_stat_area(&card);
    StatImageCrop { card, stat }
}

/// Lanczos factor that lifts a short stat band to a height the recognizer reads. Framing is left
/// alone; re-trimming the band regressed the mod card.
pub fn stat_crop_upscale_factor(height: u32) -> u32 {
    if height == 0 {
        return 1;
    }
    RELIABLE_STAT_CROP_HEIGHT.div_ceil(height).clamp(1, 3)
}

pub struct FallbackCrop {
    pub image: RgbaImage,
    pub upscale_factor: u32,
}

/// Small-card fallback: the fixed stat band clips half-size cards, so re-trim around the detected
/// text bounds. The caller upscales for OCR.
pub fn crop_stat_area_fallback(card: &RgbaImage) -> Option<FallbackCrop> {
    let (w, h) = card.dimensions();
    if w < 50 || h < 50 {
        return None;
    }
    let b = analyze_text_metrics(card, 0.08).bounds?;
    let pad_x = (b.width as f32 * FALLBACK_PAD_FRACTION).round() as u32;
    let pad_y = (b.height as f32 * FALLBACK_PAD_FRACTION).round() as u32;
    let x = b.left.saturating_sub(pad_x);
    let y = b.top.saturating_sub(pad_y);
    let width = (w - x).min(b.width + pad_x * 2);
    let height = (h - y).min(b.height + pad_y * 2);
    if width < MIN_CROP || height < MIN_CROP {
        return None;
    }
    let upscale_factor =
        ((FALLBACK_TARGET_HEIGHT / height as f32).round() as u32).clamp(1, FALLBACK_MAX_UPSCALE);
    Some(FallbackCrop {
        image: crop(
            card,
            Rect {
                x,
                y,
                width,
                height,
            },
        ),
        upscale_factor,
    })
}

pub fn upscale(img: &RgbaImage, factor: u32) -> RgbaImage {
    if factor <= 1 {
        return img.clone();
    }
    imageops::resize(
        img,
        img.width() * factor,
        img.height() * factor,
        FilterType::Lanczos3,
    )
}
