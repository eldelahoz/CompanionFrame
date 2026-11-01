//! Era of the relic selection screen, read off a captured frame. Port of WFHelper's
//! `rewardScannerEra.ts`: the filter-tab label first (it is the only thing that tells an omnia
//! fissure from a Lith one), then the relic tiles, then the header band.

use std::time::{Duration, Instant};

use image::{RgbaImage, imageops};
use serde::Serialize;
use specta::Type;

use crate::services::reward_scan::RewardReader;
use crate::services::reward_scan::era::{
    EraHit, detect_from_band, detect_from_filter_label, detect_from_tile_label,
};
use crate::services::reward_scan::image::{Cut, Rect, binarize};

/// Candidate id of the filter-tab read; only this one may override a log tag.
pub const FILTER_LABEL: &str = "filter-label";

const FILTER_LABEL_RECT: Rect = Rect {
    x: 0.02,
    y: 0.085,
    w: 0.19,
    h: 0.058,
};
const TILE_Y: f64 = 0.5;
const TILE_H: f64 = 0.42;
const TILE_XS: [f64; 5] = [0.02, 0.2, 0.38, 0.56, 0.74];
const TILE_W: f64 = 0.18;
/// (top, height) of full-width bands.
const BANDS: [(f64, f64); 3] = [(0.12, 0.12), (0.16, 0.13), (0.2, 0.14)];
const CONFIDENT: f64 = 0.9;
/// WFHelper's floor: a second OCR pass needs at least this much budget left.
const MIN_ATTEMPT: Duration = Duration::from_millis(900);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EraDetection {
    pub era: Option<String>,
    pub confidence: f64,
    /// Which crop produced the read; [`FILTER_LABEL`] for the filter tab.
    pub candidate: Option<String>,
    pub text_preview: String,
}

fn crop(frame: &RgbaImage, r: Rect) -> Option<RgbaImage> {
    let (w, h) = frame.dimensions();
    let x = (w as f64 * r.x) as u32;
    let y = (h as f64 * r.y) as u32;
    let cw = ((w as f64 * r.w) as u32).min(w.saturating_sub(x));
    let ch = ((h as f64 * r.h) as u32).min(h.saturating_sub(y));
    (cw >= 8 && ch >= 8).then(|| imageops::crop_imm(frame, x, y, cw, ch).to_image())
}

struct Scan<'a> {
    reader: &'a dyn RewardReader,
    started: Instant,
    budget: Duration,
    attempts: u32,
}

impl Scan<'_> {
    fn may_run(&self) -> bool {
        let left = self.budget.saturating_sub(self.started.elapsed());
        !left.is_zero() && (self.attempts == 0 || left >= MIN_ATTEMPT)
    }

    /// Best read over the raw and the binarised crop; stops early on a certain hit.
    fn best(
        &mut self,
        crop: &RgbaImage,
        id: &str,
        detect: impl Fn(&str) -> EraHit,
    ) -> EraDetection {
        let mut best = EraDetection::default();
        for variant in [crop.clone(), binarize(crop, 0.0, 1.0, Cut::Otsu)] {
            if !self.may_run() {
                break;
            }
            self.attempts += 1;
            let text = self.reader.ocr_lines(&variant).join(" ");
            let hit = detect(&text);
            if hit.confidence > best.confidence {
                best = EraDetection {
                    era: hit.era.map(String::from),
                    confidence: hit.confidence,
                    candidate: Some(id.into()),
                    text_preview: text.split_whitespace().collect::<Vec<_>>().join(" "),
                };
            }
            if best.confidence >= 0.99 {
                break;
            }
        }
        best
    }
}

/// `label_only` rechecks the filter tab alone (a log tag already named the era, and tile or band
/// text must not pass for a label). `budget` bounds when new OCR passes may start, not one pass.
pub fn detect(
    reader: &dyn RewardReader,
    frame: &RgbaImage,
    label_only: bool,
    budget: Duration,
) -> EraDetection {
    let mut scan = Scan {
        reader,
        started: Instant::now(),
        budget,
        attempts: 0,
    };
    let mut best = crop(frame, FILTER_LABEL_RECT)
        .map(|c| scan.best(&c, FILTER_LABEL, detect_from_filter_label))
        .unwrap_or_default();
    // A filter tab that names an era is the screen's own word: the tiles under it never overrule it.
    if label_only || best.era.is_some() {
        return best;
    }
    for (i, x) in TILE_XS.iter().enumerate() {
        let r = Rect {
            x: *x,
            y: TILE_Y,
            w: TILE_W,
            h: TILE_H,
        };
        let Some(c) = crop(frame, r) else { continue };
        let hit = scan.best(&c, &format!("tile-slot-{}", i + 1), detect_from_tile_label);
        if hit.confidence > best.confidence {
            best = hit;
        }
        if best.confidence >= 0.99 {
            return best;
        }
    }
    if best.confidence >= CONFIDENT {
        return best;
    }
    for (top, height) in BANDS {
        let r = Rect {
            x: 0.0,
            y: top,
            w: 1.0,
            h: height,
        };
        let Some(c) = crop(frame, r) else { continue };
        let hit = scan.best(&c, "header-band", detect_from_band);
        if hit.confidence > best.confidence {
            best = hit;
        }
        if best.confidence >= 0.99 {
            break;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers by call order, recording how many strips it was shown.
    struct Script(RefCell<Vec<&'static str>>, RefCell<u32>);
    impl RewardReader for Script {
        fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
            *self.1.borrow_mut() += 1;
            let mut q = self.0.borrow_mut();
            let t = if q.is_empty() { "" } else { q.remove(0) };
            vec![t.into()]
        }
    }
    fn script(t: &[&'static str]) -> Script {
        Script(RefCell::new(t.to_vec()), RefCell::new(0))
    }
    fn frame() -> RgbaImage {
        RgbaImage::from_pixel(1920, 1080, image::Rgba([40, 40, 40, 255]))
    }
    const LONG: Duration = Duration::from_secs(60);

    #[test]
    fn a_localized_all_tab_is_omnia_and_the_tiles_never_override_it() {
        let r = script(&["TOUT", "Relique Neo V4"]);
        let d = detect(&r, &frame(), false, LONG);
        assert_eq!(d.era.as_deref(), Some("omnia"));
        assert_eq!(*r.1.borrow(), 1);
    }

    #[test]
    fn filter_label_is_final_and_reads_omnia() {
        let r = script(&["ALL"]);
        let d = detect(&r, &frame(), false, LONG);
        assert_eq!(d.era.as_deref(), Some("omnia"));
        assert_eq!(d.candidate.as_deref(), Some(FILTER_LABEL));
        assert_eq!(*r.1.borrow(), 1, "a certain label read stops the scan");
    }

    #[test]
    fn falls_back_to_a_tile_when_the_label_is_unreadable() {
        // Two label variants read nothing, then slot 1 reads "MESO RELIC".
        let r = script(&["", "", "MESO RELIC"]);
        let d = detect(&r, &frame(), false, LONG);
        assert_eq!(d.era.as_deref(), Some("meso"));
        assert_eq!(d.candidate.as_deref(), Some("tile-slot-1"));
    }

    #[test]
    fn label_only_never_reads_tiles_or_bands() {
        let r = script(&["", "", "LITH RELIC"]);
        let d = detect(&r, &frame(), true, LONG);
        assert_eq!(d.era, None);
        assert_eq!(*r.1.borrow(), 2);
    }

    #[test]
    fn a_screen_listing_two_eras_reads_as_no_era() {
        let r = script(&["", "", "LITH RELIC MESO RELIC"]);
        assert_eq!(detect(&r, &frame(), false, LONG).era, None);
    }

    #[test]
    fn spent_budget_stops_new_passes_after_the_first() {
        let r = script(&[]);
        detect(&r, &frame(), false, Duration::ZERO);
        assert_eq!(*r.1.borrow(), 0, "no budget, no OCR");
        let r = script(&[]);
        detect(&r, &frame(), false, Duration::from_millis(500));
        assert_eq!(
            *r.1.borrow(),
            1,
            "the first pass runs, later ones need 900 ms spare"
        );
    }

    #[test]
    fn tiny_frames_degrade_to_no_era() {
        let r = script(&["LITH"]);
        let d = detect(&r, &RgbaImage::new(40, 40), false, LONG);
        assert_eq!(d, EraDetection::default());
    }
}
