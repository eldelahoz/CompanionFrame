//! Ports of WFHelper's `rewardCardCounter`, `rewardScannerImage` and `rewardScannerSlotScan` logic
//! tests that do not need real screenshots (those run through `examples/ocr_fixtures.rs`).
use image::{Rgba, RgbaImage};

use super::REFERENCE_UI_SCALE;
use super::RewardReader;
use super::counter::{count_cards, find_cards, guess_cards};
use super::image::{Cut, binarize, title_rects};
use super::slots::{clean, join_lines, read_slot, similarity};

pub(super) struct Draw {
    pub(super) menu_scale: f64,
    pub(super) bar_top: Option<u32>,
    pub(super) bar_rows: Option<u32>,
    pub(super) legacy: bool,
}

pub(super) const PLAIN: Draw = Draw {
    menu_scale: 1.0,
    bar_top: None,
    bar_rows: None,
    legacy: false,
};

/// Dark noisy backdrop, each card a flat panel with the thin bright bar under its title.
pub(super) fn frame_with_cards(width: u32, height: u32, count: usize, d: &Draw) -> RgbaImage {
    let at = |ratio: f64| 0.5 + (ratio - 0.5) * d.menu_scale;
    let mut seed = 7u32;
    let mut img = RgbaImage::from_fn(width, height, |_, _| {
        seed = (seed.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fff_ffff;
        let n = (seed % 9) as u8;
        Rgba([10 + n, 12 + n, 14 + n, 255])
    });
    let pitch = 0.127;
    let first = 0.5 - (count as f64 * pitch - 0.005) / 2.0;
    for i in 0..count {
        let left = first + i as f64 * pitch;
        let x0 = (at(left) * width as f64).round() as u32;
        let x1 = (at(left + 0.122) * width as f64).round() as u32;
        for y in
            (at(0.225) * height as f64).round() as u32..(at(0.47) * height as f64).round() as u32
        {
            for x in x0..x1 {
                img.put_pixel(x, y, Rgba([30, 34, 38, 255]));
            }
        }
        let bar_top = d
            .bar_top
            .unwrap_or_else(|| (at(0.443) * height as f64).round() as u32);
        let rows = d
            .bar_rows
            .unwrap_or_else(|| ((0.004 * height as f64).round() as u32).max(2));
        for y in bar_top..bar_top + rows {
            for x in x0..x1 {
                let edge = ((x - x0).min(x1 - 1 - x)) as f64 / ((x1 - x0) as f64 / 8.0);
                let shade = if d.legacy {
                    0.6 + 0.4 * edge.min(1.0)
                } else {
                    1.0
                };
                let v = |c: f64| (c * shade).round() as u8;
                img.put_pixel(x, y, Rgba([v(190.0), v(200.0), v(210.0), 255]));
            }
        }
    }
    img
}

/// Legacy menu scale keeps a fixed pixel size: bar pitch about 202 px, 3 px bars 53 px above centre.
fn legacy_at(width: u32, height: u32) -> Draw {
    Draw {
        menu_scale: 202.0 / (0.127 * width as f64),
        bar_top: Some(height / 2 - 53 + u32::from(height % 2 == 1)),
        bar_rows: Some(3),
        legacy: true,
    }
}

#[test]
fn counts_synthetic_cards() {
    for count in 1..=4 {
        let f = frame_with_cards(1920, 1080, count, &PLAIN);
        assert_eq!(count_cards(&f, 1.0), count, "1080p x{count}");
    }
    assert_eq!(
        count_cards(&frame_with_cards(2560, 1440, 4, &PLAIN), 1.0),
        4
    );
    assert_eq!(
        count_cards(&frame_with_cards(1920, 1080, 0, &PLAIN), 1.0),
        0
    );
}

#[test]
fn find_keeps_the_configured_scale_when_its_bars_are_found() {
    let f = frame_with_cards(1920, 1080, 4, &PLAIN);
    assert_eq!(find_cards(&f, 1.0), (4, 1.0));
}

#[test]
fn finds_legacy_scale_cards() {
    for (count, w, h) in [
        (1, 1920, 1080),
        (4, 1920, 1080),
        (2, 2560, 1440),
        (3, 2560, 1440),
    ] {
        let d = legacy_at(w, h);
        let f = frame_with_cards(w, h, count, &d);
        assert_eq!(
            count_cards(&f, REFERENCE_UI_SCALE),
            0,
            "{count} at {w}x{h} misses at 0.99"
        );
        let (found, scale) = find_cards(&f, REFERENCE_UI_SCALE);
        assert_eq!(found, count, "{count} at {w}x{h}");
        assert!(
            (scale / REFERENCE_UI_SCALE - d.menu_scale).abs() < 0.03,
            "{scale} vs {}",
            d.menu_scale
        );
    }
}

#[test]
fn finds_custom_scale_and_too_low_slider() {
    let real = 0.78;
    let ms = real / REFERENCE_UI_SCALE;
    let d = Draw {
        menu_scale: ms,
        bar_top: Some(((0.5 + (0.443 - 0.5) * ms) * 1080.0).round() as u32),
        bar_rows: Some(4),
        legacy: false,
    };
    let (n, s) = find_cards(&frame_with_cards(1920, 1080, 2, &d), 0.5);
    assert_eq!(n, 2);
    assert!((s - real).abs() <= 0.02, "{s}");

    let (n, s) = find_cards(&frame_with_cards(1920, 1080, 3, &PLAIN), 0.62);
    assert_eq!(n, 3);
    assert!(s >= 0.95, "{s}");
}

#[test]
fn guess_rejects_a_blank_frame() {
    assert_eq!(
        guess_cards(&frame_with_cards(1920, 1080, 0, &PLAIN), 1.0),
        0
    );
}

#[test]
fn four_card_title_matches_wfhelper_crop() {
    let r = title_rects(4, 1920, 1080, REFERENCE_UI_SCALE);
    // WFHelper's slot-crop fixture is 470x413 234x44 (height differs by a rounding pixel).
    assert_eq!((r[0].0, r[0].1, r[0].2), (470, 413, 234));
    assert!(r[3].0.abs_diff(1202) <= 1);
    assert_eq!(title_rects(1, 1920, 1080, REFERENCE_UI_SCALE).len(), 1);
}

#[test]
fn wide_tall_and_scaled_frames_stay_inside() {
    for (w, h) in [(1899, 1199), (2560, 1080), (1080, 607), (800, 600)] {
        for ui in [0.3, REFERENCE_UI_SCALE, 1.0] {
            for count in 1..=4 {
                for (x, y, cw, ch) in title_rects(count, w, h, ui) {
                    assert!(x + cw <= w && y + ch <= h, "{w}x{h} ui {ui}");
                }
            }
        }
    }
}

#[test]
fn smaller_ui_scale_pulls_titles_toward_the_centre() {
    let full = title_rects(4, 1920, 1080, REFERENCE_UI_SCALE);
    let small = title_rects(4, 1920, 1080, 0.5);
    assert!(small[0].0 > full[0].0 && small[3].0 < full[3].0);
}

#[test]
fn text_helpers() {
    assert_eq!(similarity("Fang Prime Handle", "fang  prime handle."), 1.0);
    assert!(similarity("Fang Prime Handle", "Fang Prime Handel") > 0.86);
    assert_eq!(clean("2 X Forma & Blueprint |"), "2X Forma & Blueprint");
    assert_eq!(clean("2x Forma Blueprint"), "2x Forma Blueprint");
    assert_eq!(clean("7 Forma Blueprint"), "Forma Blueprint");
    assert_eq!(
        join_lines(&["Xaku Prime x", "", " Blueprint "]),
        "Xaku Prime Blueprint"
    );
    assert_eq!(join_lines::<&str>(&["", "|"]), "");
}

/// Dark card, bright text rows, with a mid-bright blob fused to the top line like card art.
fn title_strip() -> RgbaImage {
    RgbaImage::from_fn(120, 40, |x, y| {
        let text = (y % 20 > 5 && y % 20 < 14) && x > 10 && x < 110 && (x / 3) % 2 == 0;
        let blob = y < 20 && (45..75).contains(&x);
        Rgba(match (text, blob) {
            (true, _) => [230, 220, 190, 255],
            (_, true) => [150, 140, 120, 255],
            _ => [20, 20, 24, 255],
        })
    })
}

fn dark_pixels(img: &RgbaImage) -> usize {
    img.pixels().filter(|p| p[0] == 0).count()
}

#[test]
fn binarize_gives_dark_text_on_white_and_strict_drops_mid_bright_art() {
    let s = title_strip();
    let otsu = binarize(&s, 0.0, 0.5, Cut::Otsu);
    let strict = binarize(&s, 0.0, 0.5, Cut::Strict);
    assert!(otsu.pixels().all(|p| p[0] == 0 || p[0] == 255));
    assert!(
        dark_pixels(&otsu) * 2 < otsu.pixels().len(),
        "white background"
    );
    assert!(dark_pixels(&strict) <= dark_pixels(&otsu));
}

/// Reader that serves canned lines per call, in `read_slot` order.
struct Canned(std::cell::RefCell<Vec<Vec<&'static str>>>);

impl RewardReader for Canned {
    fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
        self.0
            .borrow_mut()
            .remove(0)
            .into_iter()
            .map(String::from)
            .collect()
    }
}

#[test]
fn read_slot_joins_wrapped_title_lines_and_dedups() {
    // bottom band, top band, whole strip, strict top band
    let r = Canned(
        vec![
            vec!["Blueprint"],
            vec!["Xaku Prime a Chassis"],
            vec!["Xaku Prime Chassis", "Blueprint"],
            vec!["Xaku Prime Chassis"],
        ]
        .into(),
    );
    let got = read_slot(&r, &title_strip());
    assert_eq!(got, ["Xaku Prime Chassis Blueprint"]);
}

#[test]
fn read_slot_keeps_distinct_reads_and_skips_empty_ones() {
    let r = Canned(vec![vec!["Handle"], vec!["Fang Prime"], vec![], vec!["x"]].into());
    assert_eq!(
        read_slot(&r, &title_strip()),
        ["Fang Prime Handle", "Handle"]
    );
}

#[test]
fn match_slot_gates_and_prefers_the_longer_exact_name() {
    use super::names::PoolItem;
    use super::slots::match_slot;
    let pool: Vec<PoolItem> = [
        "Xaku Prime Chassis Blueprint",
        "Fang Prime Handle",
        "Forma Blueprint",
    ]
    .map(|n| PoolItem::new(n, None))
    .to_vec();
    let got = match_slot(
        &["Xaku Prime Chassis Blueprint".into(), "Xaku Prime".into()],
        &pool,
    );
    assert_eq!(
        got[0].item.as_ref().unwrap().name,
        "Xaku Prime Chassis Blueprint"
    );
    assert!(match_slot(&["zzzz qqqq".into()], &pool).is_empty());
}

/// Windows-style bands read nothing; the raw-strip reader supplies the candidate.
struct RawOnly;

impl RewardReader for RawOnly {
    fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
        vec![]
    }
    fn raw_lines(&self, _: &RgbaImage) -> Vec<String> {
        vec!["Grendel Prime".into(), "Blueprint".into()]
    }
}

#[test]
fn read_slot_adds_the_raw_strip_read_as_a_candidate() {
    assert_eq!(
        read_slot(&RawOnly, &title_strip()),
        ["Grendel Prime Blueprint"]
    );
}

#[test]
fn onnx_row_splitting_and_text_repair() {
    use super::onnx_reader::{clean_row_text, split_rows};
    // two 12-row lines with a 5-row gap, plus a 3-row speck that is too short
    let mut ink = vec![0u32; 50];
    ink[2..14].fill(40);
    ink[19..31].fill(40);
    ink[40..43].fill(40);
    assert_eq!(split_rows(&ink, 400), [(2, 13), (19, 30)]);
    // one 100-row blob with a thin waist is cut there
    let mut ink = vec![40u32; 100];
    ink[50] = 2;
    assert_eq!(split_rows(&ink, 400), [(0, 49), (51, 99)]);
    // an over-tall block with no waist stays whole
    assert_eq!(split_rows(&vec![40u32; 100], 400), [(0, 99)]);

    assert_eq!(
        clean_row_text("KhoraPrime Systems.Blueprint"),
        "Khora Prime Systems Blueprint"
    );
    assert_eq!(
        clean_row_text("Cobra&Crane  Prime_Set"),
        "Cobra&Crane Prime Set"
    );
    assert_eq!(
        clean_row_text("Plan du Châssis de Khora Prime"),
        "Plan du Châssis de Khora Prime"
    );
    // A zh card keeps its characters instead of turning to spaces.
    assert_eq!(clean_row_text("Volt Prime 机体"), "Volt Prime 机体");
}
