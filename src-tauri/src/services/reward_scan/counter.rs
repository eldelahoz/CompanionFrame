//! Card counter (WFHelper `countRewardCardsInBitmap` / `findRewardCardsInBitmap`): every reward
//! card draws a thin bar just under its title, and the outermost card of an N-card row only exists
//! when N cards are up, so the bar is probed from 4 down to 1 in the outer third of that card.
//! When the configured interface scale finds no bar, the Legacy-menu scale and then every step of
//! 25-100 % are tried.
use std::collections::HashMap;

use image::RgbaImage;

use super::image::{FIXED_LAYOUTS, Rect, aspect_scale, correct};

const BAND: (f64, f64) = (0.431, 0.027); // top, height
const WINDOW_WIDTH: f64 = 0.064;
// Window starts as fractions of the four-card row width.
const WINDOWS: [(usize, [f64; 2]); 4] = [
    (4, [0.01, 0.91]),
    (3, [0.15, 0.78]),
    (2, [0.28, 0.665]),
    (1, [0.4, 0.535]),
];

// Legacy menu scale (EE.cfg DSM_MATCH_SCREEN) ignores the slider and keeps a fixed pixel size:
// uiScale = 885 / (16:9 canvas height).
const LEGACY_UI_SCALE_TIMES_HEIGHT: f64 = 885.0;
const LEGACY_PROBE_STEPS: i64 = 3;
const SEARCH_MIN_STEP: f64 = 50.0;
const MIN_STEP: i64 = 25;
const MAX_STEP: i64 = 100;

struct Frame<'a> {
    rgba: &'a [u8],
    stride: usize,
    width: usize,
    height: usize,
}

struct Band {
    row_left: i64,
    row_width: i64,
    top: usize,
    height: usize,
    window_w: usize,
}

fn band(width: u32, height: u32, ui_scale: f64) -> Band {
    let left0 = FIXED_LAYOUTS[3][0].x;
    let last = FIXED_LAYOUTS[3][3];
    let row = correct(
        &[Rect {
            x: left0,
            y: BAND.0,
            w: last.x + last.w - left0,
            h: BAND.1,
        }],
        width,
        height,
        ui_scale,
    )[0];
    let row_width = (row.w * width as f64).round() as i64;
    let top = ((row.y * height as f64).round() as i64).max(0) as usize;
    Band {
        row_left: (row.x * width as f64).round() as i64,
        row_width,
        top,
        height: (height as usize - top).min((row.h * height as f64).round() as usize),
        window_w: (row_width as f64 * WINDOW_WIDTH).round() as usize,
    }
}

impl<'a> Frame<'a> {
    fn new(img: &'a RgbaImage) -> Self {
        Frame {
            rgba: img.as_raw(),
            stride: img.width() as usize * 4,
            width: img.width() as usize,
            height: img.height() as usize,
        }
    }

    fn dist(&self, a: usize, b: usize) -> f64 {
        let d = |i: usize| f64::from(self.rgba[a + i]) - f64::from(self.rgba[b + i]);
        (d(0).powi(2) + d(1).powi(2) + d(2).powi(2)).sqrt()
    }

    /// One thin horizontal bar of even colour: the middle row runs uniform across the window and
    /// each column's uniform run around it is short and of similar height. `thin` also takes a
    /// 2 px bar.
    fn has_bar(&self, left: usize, top: usize, width: usize, height: usize, thin: bool) -> bool {
        if width < 12 || height < 6 || top + height > self.height || left + width > self.width {
            return false;
        }
        let mid = top + height / 2;
        let (from, to) = (left + width / 6, left + width * 5 / 6);
        let run = (from + 1..to)
            .take_while(|&x| {
                let here = mid * self.stride + x * 4;
                self.dist(here, here - 4) < 45.0
            })
            .count();
        if (run as f64) / (width as f64 * 0.6) < 0.9 {
            return false;
        }
        let heights: Vec<usize> = (left..left + width)
            .map(|x| {
                let up = (top..mid)
                    .rev()
                    .take_while(|&y| {
                        let here = y * self.stride + x * 4;
                        self.dist(here, here + self.stride) <= 32.0
                    })
                    .count();
                let down = (mid + 1..top + height)
                    .take_while(|&y| {
                        let here = y * self.stride + x * 4;
                        self.dist(here, here - self.stride) <= 32.0
                    })
                    .count();
                up + down
            })
            .collect();
        let n = heights.len() as f64;
        let avg = heights.iter().sum::<usize>() as f64 / n;
        let mut sorted = heights.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        let trimmed = &sorted[3.min(sorted.len())..];
        let variance = trimmed
            .iter()
            .map(|&h| (h as f64 - avg).powi(2))
            .sum::<f64>()
            / trimmed.len().max(1) as f64;
        let norm_avg = avg / height as f64;
        let norm_std = 5.0 * variance.sqrt() / height as f64;
        let floor = if thin { 1 } else { 2 };
        if heights.iter().filter(|&&h| h < floor).count() as f64 > n / 3.0 {
            return false;
        }
        (if thin { avg >= 1.0 } else { norm_avg >= 0.05 }) && norm_avg <= 0.27 && norm_std <= 0.36
    }

    /// Share of columns in `[from, to)` where `row` stands out from the rows `reach` above and
    /// below it, i.e. crosses a thin bar (bars shade along their length, so compare vertically).
    fn thin_bar_share(&self, row: usize, reach: usize, from: usize, to: usize, step: usize) -> f64 {
        if row < reach || row + reach >= self.height {
            return 0.0;
        }
        let (mut crossing, mut sampled) = (0usize, 0usize);
        for x in (from..to.min(self.width)).step_by(step) {
            sampled += 1;
            let here = row * self.stride + x * 4;
            if self.dist(here, here - reach * self.stride) > 32.0
                && self.dist(here, here + reach * self.stride) > 32.0
            {
                crossing += 1;
            }
        }
        crossing as f64 / sampled.max(1) as f64
    }

    /// The bar runs up to `edge` and stops there.
    fn bar_ends_at(&self, mid: usize, reach: usize, edge: i64, outward: i64, card_w: f64) -> bool {
        let near = ((card_w * 0.03).round() as i64).max(1);
        let far = ((card_w * 0.1).round() as i64).max(near + 2);
        let (inside, outside) = if outward < 0 {
            ((edge + near, edge + far), (edge - far, edge - near))
        } else {
            ((edge - far, edge - near), (edge + near, edge + far))
        };
        if outside.0 < 0 || outside.1 > self.width as i64 || inside.0 < 0 {
            return false;
        }
        let share = |r: (i64, i64)| self.thin_bar_share(mid, reach, r.0 as usize, r.1 as usize, 1);
        share(inside) >= 2.0 / 3.0 && share(outside) <= 1.0 / 3.0
    }

    /// The row near the band middle that crosses the most bar columns along the four-card span.
    fn bar_row_near(&self, b: &Band) -> Option<(usize, usize)> {
        let middle = (b.top + b.height / 2) as i64;
        // The bar is under a sixth of the band tall, so rows a quarter band off are clear of it.
        let reach = ((b.height as f64 / 4.0).ceil() as usize).max(2);
        // The 3 px Legacy bar at 1080p sits 2 rows above the scaled band middle.
        let slack = ((b.height as f64 / 8.0).round() as i64).max(2);
        let from = b.row_left.max(0) as usize;
        let to = (b.row_left + b.row_width).clamp(0, self.width as i64) as usize;
        let step = ((b.row_width / 256) as usize).max(1);
        let mut best = None;
        let mut best_share = -1.0;
        for row in middle - slack..=middle + slack {
            if row < b.height as i64 || row + b.height as i64 >= self.height as i64 {
                continue;
            }
            let share = self.thin_bar_share(row as usize, reach, from, to, step);
            if share > best_share {
                best_share = share;
                best = Some((row as usize, reach));
            }
        }
        best
    }

    /// N cards at this scale: the bar near both edges of every card, ending at the row's predicted
    /// outer edges, and no bar in the slot beyond either end.
    fn row_verified(&self, count: usize, ui_scale: f64, bar: (usize, usize), b: &Band) -> bool {
        let (mid, reach) = bar;
        let top = mid.saturating_sub(b.height / 2);
        let has =
            |left: i64| left >= 0 && self.has_bar(left as usize, top, b.window_w, b.height, true);
        let (w, h) = (self.width as u32, self.height as u32);
        let cards: Vec<(i64, f64)> = correct(FIXED_LAYOUTS[count - 1], w, h, ui_scale)
            .iter()
            .map(|c| ((c.x * w as f64).round() as i64, c.w * w as f64))
            .collect();
        let inset = |cw: f64| (cw * 0.08).round() as i64;
        let ww = b.window_w as i64;
        for &(left, cw) in &cards {
            if !has(left + inset(cw)) || !has((left as f64 + cw).round() as i64 - inset(cw) - ww) {
                return false;
            }
        }
        let (first, last) = (cards[0], cards[cards.len() - 1]);
        let (left_edge, right_edge) = (first.0, (last.0 as f64 + last.1).round() as i64);
        if !self.bar_ends_at(mid, reach, left_edge, -1, first.1)
            || !self.bar_ends_at(mid, reach, right_edge, 1, last.1)
        {
            return false;
        }
        let mut offset = inset(first.1);
        while (offset + ww) as f64 <= first.1 * 1.1 {
            if has(left_edge - offset - ww) || has(right_edge + offset) {
                return false;
            }
            offset += (ww >> 1).max(1);
        }
        true
    }
}

/// Number of reward cards (1..=4) read off the frame's card bars at `ui_scale`; 0 when none found.
pub fn count_cards(img: &RgbaImage, ui_scale: f64) -> usize {
    let b = band(img.width(), img.height(), ui_scale);
    let f = Frame::new(img);
    for (count, starts) in WINDOWS {
        for start in starts {
            let left = b.row_left + (b.row_width as f64 * start).round() as i64;
            if left >= 0 && f.has_bar(left as usize, b.top, b.window_w, b.height, false) {
                return count;
            }
        }
    }
    0
}

/// Card count plus the interface scale it was read at. When the configured scale finds no bar, the
/// steps around the Legacy size are tried, then every step of the search range; the count must
/// agree across all verified steps of a pass, and any disagreement reports 0.
pub fn find_cards(img: &RgbaImage, ui_scale: f64) -> (usize, f64) {
    let configured = count_cards(img, ui_scale);
    if configured > 0 {
        return (configured, ui_scale);
    }
    let f = Frame::new(img);
    let (w, h) = img.dimensions();
    let mut rows: HashMap<i64, usize> = HashMap::new();
    let mut row_at = |step: i64| -> usize {
        *rows.entry(step).or_insert_with(|| {
            let cand = step as f64 / 100.0;
            let b = band(w, h, cand);
            f.bar_row_near(&b).map_or(0, |bar| {
                [4, 3, 2, 1]
                    .into_iter()
                    .find(|&n| f.row_verified(n, cand, bar, &b))
                    .unwrap_or(0)
            })
        })
    };
    let in_range = |step: i64| (MIN_STEP..=MAX_STEP).contains(&step);
    let legacy = LEGACY_UI_SCALE_TIMES_HEIGHT / (h as f64 * aspect_scale(w, h).1);
    let legacy_step = (legacy * 100.0).round() as i64;
    // Probe order: legacy, +1, -1, +2, -2, ... (as WFHelper's `i % 2 ? (i+1)/2 : -i/2`).
    let nearest = (0..LEGACY_PROBE_STEPS * 2 + 1)
        .map(|i| legacy_step + if i % 2 == 1 { (i + 1) / 2 } else { -i / 2 })
        .find(|&s| in_range(s) && row_at(s) > 0);
    if let Some(nearest) = nearest {
        let count = row_at(nearest);
        let (mut lo, mut hi) = (nearest, nearest);
        while in_range(lo - 1) && row_at(lo - 1) == count {
            lo -= 1;
        }
        while in_range(hi + 1) && row_at(hi + 1) == count {
            hi += 1;
        }
        if [lo - 1, hi + 1]
            .into_iter()
            .filter(|&s| in_range(s))
            .all(|s| row_at(s) == 0)
        {
            // Synthetic Legacy runs hold that size 2+ steps from each end; a custom screen near
            // it only touches the run's end.
            let step = if legacy_step - lo >= 2 && hi - legacy_step >= 2 {
                legacy_step
            } else {
                lo + ((hi - lo + 1) >> 1)
            };
            return (count, step as f64 / 100.0);
        }
    }
    let floor = SEARCH_MIN_STEP.min(legacy * 90.0).round() as i64;
    let hits: Vec<(usize, f64)> = (floor..=MAX_STEP)
        .filter(|&s| in_range(s))
        .map(|s| (row_at(s), s as f64 / 100.0))
        .filter(|h| h.0 > 0)
        .collect();
    if hits.is_empty() || hits.iter().any(|h| h.0 != hits[0].0) {
        return (0, ui_scale);
    }
    hits[hits.len() / 2]
}

/// Share of bright, textured pixels in a card rect (WFHelper `computeSlotActivity`).
fn activity(img: &RgbaImage, r: Rect) -> f64 {
    let (iw, ih) = img.dimensions();
    let (x, y) = (
        (iw as f64 * r.x.clamp(0.0, 0.98)) as u32,
        (ih as f64 * r.y.clamp(0.0, 0.98)) as u32,
    );
    let w = ((iw as f64 * r.w) as u32).max(24).min(iw - x);
    let h = ((ih as f64 * r.h) as u32).max(24).min(ih - y);
    if w < 30 || h < 30 {
        return 0.0;
    }
    let lum = |px: u32, py: u32| {
        let p = img.get_pixel(x + px, y + py);
        (77 * u32::from(p[0]) + 150 * u32::from(p[1]) + 29 * u32::from(p[2])) >> 8
    };
    let (sx, sy) = ((w / 80).max(1), (h / 80).max(1));
    let (mut bright, mut texture, mut total) = (0u32, 0u32, 0u32);
    for py in (sy..h).step_by(sy as usize) {
        for px in (sx..w).step_by(sx as usize) {
            let l = lum(px, py);
            bright += u32::from(l >= 86);
            texture += l.abs_diff(lum(px - sx, py));
            total += 1;
        }
    }
    if total == 0 {
        return 0.0;
    }
    let t = f64::from(total);
    (f64::from(bright) / t / 0.24).min(1.0) * 0.45 + (f64::from(texture) / t / 42.0).min(1.0) * 0.55
}

// shortcut: WFHelper reads up to 6 ranked layouts and keeps the one that matches the item DB;
// without item-DB matching here only the top-ranked layout is returned.
/// Card count guessed from how busy each card rect is, for frames whose card bars the counter
/// cannot find (1080x607 downscales, older UI): 0 when no layout is confident.
pub fn guess_cards(img: &RgbaImage, ui_scale: f64) -> usize {
    let (w, h) = img.dimensions();
    let mut layouts: Vec<(usize, f64)> = (1..=4)
        .filter_map(|count| {
            let acts: Vec<f64> = correct(FIXED_LAYOUTS[count - 1], w, h, ui_scale)
                .into_iter()
                .map(|r| activity(img, r))
                .collect();
            let active = acts.iter().filter(|&&a| a >= 0.22).count();
            let avg = acts.iter().sum::<f64>() / acts.len() as f64;
            let conf = (active as f64 / count as f64).min(1.0) * 0.58 + (avg / 0.7).min(1.0) * 0.42;
            (active >= count.min(2) || avg >= 0.28).then_some((count, conf))
        })
        .collect();
    layouts.sort_by(|a, b| {
        if (a.1 - b.1).abs() < 0.08 {
            b.0.cmp(&a.0)
        } else {
            b.1.total_cmp(&a.1)
        }
    });
    match layouts.first() {
        Some(&(1, c)) if c >= 0.55 => 1,
        Some(&(n, c)) if n >= 2 && c >= 0.38 => n,
        _ => 0,
    }
}
