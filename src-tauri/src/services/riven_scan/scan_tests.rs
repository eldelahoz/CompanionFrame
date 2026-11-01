use ::image::{Rgba, RgbaImage};

use super::detect::{self, LineBox};
use super::image::{self as img, Rect};
use super::lines::{self, LOW_CONFIDENCE_THRESHOLD};
use super::*;

const SCREEN_W: u32 = 2560;
const SCREEN_H: u32 = 1440;
const TEXT_W: u32 = 340;
// Stat text rows sit inside the stat band of the rough single-card crop (screen y 839..1163 at 1440p).
const TEXT_TOP: u32 = 880;
const TEXT_BOTTOM: u32 = 1160;

/// Dark screen with white "stat line" stripes starting at `text_left`.
fn screen(
    text_left: u32,
    (text_top, text_bottom): (u32, u32),
    (w, h): (u32, u32),
    text_w: u32,
    background: u8,
) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(w, h, Rgba([background, background, background, 255]));
    for y in text_top..text_bottom {
        if (y - text_top) % 28 >= 16 {
            continue; // 16 px text line, 12 px gap
        }
        for x in text_left..text_left + text_w {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }
    img
}

fn default_screen(text_left: u32) -> RgbaImage {
    screen(
        text_left,
        (TEXT_TOP, TEXT_BOTTOM),
        (SCREEN_W, SCREEN_H),
        TEXT_W,
        30,
    )
}

fn is_white(p: &Rgba<u8>) -> bool {
    p.0[..3].iter().all(|&c| c > 200)
}

/// Width of the white text span found in a crop (0 when absent).
fn white_span(img: &RgbaImage) -> u32 {
    let cols = img
        .enumerate_pixels()
        .filter(|(_, _, p)| is_white(p))
        .map(|(x, _, _)| x);
    let (min, max) = cols.fold((u32::MAX, 0), |(lo, hi), x| (lo.min(x), hi.max(x)));
    if min == u32::MAX { 0 } else { max - min + 1 }
}

fn white_row_groups(img: &RgbaImage) -> usize {
    let (mut groups, mut prev) = (0, false);
    for y in 0..img.height() {
        let row = (0..img.width()).any(|x| is_white(img.get_pixel(x, y)));
        groups += usize::from(row && !prev);
        prev = row;
    }
    groups
}

// ---- crops (ports of rivenScanImage.test.ts) ----

#[test]
fn keeps_the_full_text_column_when_the_card_is_centered() {
    let s = default_screen(SCREEN_W / 2 - TEXT_W / 2);
    assert!(white_span(&img::crop_stat_image(&s, SINGLE_CARD).stat) >= TEXT_W - 4);
}

#[test]
fn keeps_the_full_text_column_when_the_card_sits_150px_left_of_center() {
    // Letterbox shaving on dark scenes shifts the card within the rough crop.
    let s = default_screen(SCREEN_W / 2 - TEXT_W / 2 - 150);
    assert!(white_span(&img::crop_stat_image(&s, SINGLE_CARD).stat) >= TEXT_W - 4);
}

#[test]
fn falls_back_to_the_geometric_center_when_no_text_is_found() {
    let s = RgbaImage::from_pixel(SCREEN_W, SCREEN_H, Rgba([30, 30, 30, 255]));
    let stat = img::crop_stat_image(&s, SINGLE_CARD).stat;
    assert!(stat.width() > 100 && stat.height() > 100);
}

#[test]
fn recovers_an_off_center_card_in_the_widened_roll_crop() {
    let s = default_screen(SCREEN_W / 2 - TEXT_W / 2 - 120);
    assert!(white_span(&img::crop_stat_image(&s, ROLL_CARD).stat) >= TEXT_W - 4);
}

#[test]
fn keeps_every_stat_row_in_the_higher_chat_link_layout() {
    let s = screen(
        SCREEN_W / 2 - TEXT_W / 2,
        (
            (SCREEN_H as f32 * 0.53).round() as u32,
            (SCREEN_H as f32 * 0.62).round() as u32,
        ),
        (SCREEN_W, SCREEN_H),
        TEXT_W,
        30,
    );
    assert_eq!(
        white_row_groups(&img::crop_stat_image(&s, CHAT_CARD).stat),
        5
    );
    assert!(white_row_groups(&img::crop_stat_image(&s, SINGLE_CARD).stat) < 5);
}

#[test]
fn keeps_the_stat_column_in_every_layout_and_client_aspect() {
    let layouts = [
        ("single-card", SINGLE_CARD, 0.61, 0.78),
        ("chat-card", CHAT_CARD, 0.5, 0.64),
        ("roll-card", ROLL_CARD, 0.6, 0.75),
    ];
    let clients = [(1280, 720), (1280, 800), (1024, 768), (1720, 720)];
    for (name, rect, top, bottom) in layouts {
        for (w, h) in clients {
            let canvas_w = w.min((h as f32 * 16.0 / 9.0).round() as u32);
            let canvas_h = h.min((w as f32 * 9.0 / 16.0).round() as u32);
            let (cx, cy) = ((w - canvas_w) / 2, (h - canvas_h) / 2);
            let text_w = 120.max((canvas_w as f32 * 0.135).round() as u32);
            let left = cx + (canvas_w - text_w) / 2;
            let t = cy + (canvas_h as f32 * top).round() as u32;
            let b = cy + (canvas_h as f32 * bottom).round() as u32;
            let s = screen(left, (t, b), (w, h), text_w, 30);
            let stat = img::crop_stat_image(&s, rect).stat;
            assert!(white_span(&stat) >= text_w - 4, "{name} {w}x{h}");
            assert!(white_row_groups(&stat) >= 3, "{name} {w}x{h}");
        }
    }
}

#[test]
fn does_not_detect_black_bars_inside_a_dark_1280x960_client() {
    let (w, h, canvas_h, canvas_y, text_w) = (1280, 960, 720, 120, 190);
    let s = screen(
        (w - text_w) / 2,
        (
            canvas_y + (canvas_h as f32 * 0.61).round() as u32,
            canvas_y + (canvas_h as f32 * 0.78).round() as u32,
        ),
        (w, h),
        text_w,
        0,
    );
    assert_eq!(
        img::frame_canvas_content_rect(w, h),
        Rect {
            x: 0,
            y: 120,
            width: 1280,
            height: 720
        }
    );
    let stat = img::crop_stat_image(&s, SINGLE_CARD).stat;
    assert!(white_span(&stat) >= text_w - 4);
    assert!(white_row_groups(&stat) >= 4);
}

#[test]
fn fallback_retrims_a_half_size_card_and_asks_for_an_upscale() {
    // 50% interface scale: text half as wide, in a short band whose first rows sit above the fixed stat band.
    let small = TEXT_W / 2;
    let s = screen(
        SCREEN_W / 2 - small / 2,
        (780, 920),
        (SCREEN_W, SCREEN_H),
        small,
        30,
    );
    let crops = img::crop_stat_image(&s, SINGLE_CARD);
    // The fixed stat band clips part of the shrunken text column.
    assert!(white_row_groups(&crops.stat) < 5);

    let fallback = img::crop_stat_area_fallback(&crops.card).expect("text bounds");
    assert!(fallback.upscale_factor >= 2);
    assert!(white_span(&fallback.image) >= small - 8);
    assert_eq!(white_row_groups(&fallback.image), 5);
}

#[test]
fn fallback_asks_for_no_upscale_on_a_full_size_card() {
    let s = default_screen(SCREEN_W / 2 - TEXT_W / 2);
    let crops = img::crop_stat_image(&s, SINGLE_CARD);
    let fallback = img::crop_stat_area_fallback(&crops.card).expect("text bounds");
    assert_eq!(fallback.upscale_factor, 1);
    assert!(white_span(&fallback.image) >= TEXT_W - 8);
}

#[test]
fn fallback_is_none_when_the_crop_holds_no_text() {
    let s = screen(
        SCREEN_W / 2 - TEXT_W / 2,
        (0, 0),
        (SCREEN_W, SCREEN_H),
        TEXT_W,
        30,
    );
    let crops = img::crop_stat_image(&s, SINGLE_CARD);
    assert!(img::crop_stat_area_fallback(&crops.card).is_none());
}

#[test]
fn text_metrics_score_a_card_and_not_a_blank_frame() {
    let card = default_screen(SCREEN_W / 2 - TEXT_W / 2);
    let crop = img::crop_stat_image(&card, SINGLE_CARD).card;
    let blank = RgbaImage::from_pixel(400, 300, Rgba([30, 30, 30, 255]));
    assert!(img::analyze_text_metrics(&crop, 0.2).score > 0.2);
    assert_eq!(img::analyze_text_metrics(&blank, 0.2).score, 0.0);
}

#[test]
fn upscale_factor_lifts_short_bands_only_up_to_3x() {
    assert_eq!(img::stat_crop_upscale_factor(0), 1);
    assert_eq!(img::stat_crop_upscale_factor(400), 1);
    assert_eq!(img::stat_crop_upscale_factor(162), 2);
    assert_eq!(img::stat_crop_upscale_factor(60), 3);
}

// ---- detector plumbing (no model) ----

fn boxy(x1: i32, y1: i32, x2: i32, y2: i32) -> LineBox {
    LineBox { x1, y1, x2, y2 }
}

/// A `w` x `h` probability map with `score` inside each `(x, y, w, h)` rectangle.
fn prob_map(w: usize, h: usize, rects: &[(usize, usize, usize, usize, f32)]) -> Vec<f32> {
    let mut m = vec![0.0; w * h];
    for &(rx, ry, rw, rh, score) in rects {
        for y in ry..ry + rh {
            m[y * w..][rx..rx + rw].fill(score);
        }
    }
    m
}

#[test]
fn prepare_lifts_the_short_side_to_736_in_multiples_of_32() {
    let p = detect::prepare(&RgbaImage::from_pixel(340, 280, Rgba([255, 0, 0, 255])));
    // 736 / 280 = 2.63: 340 -> 894 -> 896, 280 -> 736.
    assert_eq!((p.w, p.h), (896, 736));
    let plane = p.w * p.h;
    // Red pixel in BGR order, mapped to -1..1.
    assert_eq!(
        (p.blob[0], p.blob[plane], p.blob[2 * plane]),
        (-1.0, -1.0, 1.0)
    );
    let big = detect::prepare(&RgbaImage::from_pixel(4000, 800, Rgba([0, 0, 0, 255])));
    assert_eq!(big.w, 1920, "the long side is capped");
}

#[test]
fn boxes_from_map_grows_each_line_and_maps_back_to_image_pixels() {
    // Two lines on a 100x80 map shown on a 200x160 image; the weak blob (0.4 mean) is dropped.
    let map = prob_map(
        100,
        80,
        &[
            (10, 10, 60, 8, 0.9),
            (10, 40, 60, 8, 0.9),
            (10, 65, 20, 8, 0.4),
        ],
    );
    let boxes = detect::boxes_from_map(&[1, 1, 80, 100], &map, (200, 160));
    // unclip d = 60*8*1.6 / (2*68) = 5.65 map px, then x2.
    assert_eq!(boxes, vec![boxy(9, 9, 151, 47), boxy(9, 69, 151, 107)]);
}

#[test]
fn a_tall_edge_sliver_does_not_chain_the_lines_it_crosses() {
    // A 4x40 glow beside two stacked lines would overlap both rows and merge them into one box.
    let map = prob_map(
        100,
        80,
        &[
            (10, 10, 60, 8, 0.9),
            (10, 25, 60, 8, 0.9),
            (90, 5, 4, 40, 0.9),
        ],
    );
    assert_eq!(
        detect::boxes_from_map(&[1, 1, 80, 100], &map, (100, 80)).len(),
        2
    );
}

#[test]
fn boxes_from_map_drops_slivers_and_wrong_layouts() {
    let map = prob_map(100, 80, &[(10, 10, 60, 2, 0.9)]);
    assert!(detect::boxes_from_map(&[1, 1, 80, 100], &map, (100, 80)).is_empty());
    assert!(detect::boxes_from_map(&[1, 3, 80, 100], &map, (100, 80)).is_empty());
    assert!(detect::boxes_from_map(&[1, 1, 80, 100], &[0.9; 10], (100, 80)).is_empty());
}

#[test]
fn crops_are_padded_filtered_and_upscaled_together() {
    let frame = RgbaImage::from_pixel(400, 300, Rgba([200, 100, 50, 255]));
    let boxes = [
        boxy(50, 50, 150, 70),  // 116x36 with padding
        boxy(10, 10, 20, 12),   // tiny, but the padding lifts it over the minimum
        boxy(0, 100, 300, 250), // far taller than a stat line
    ];
    let crops = detect::extract_and_upscale_crops(&frame, &boxes);
    assert_eq!(crops.len(), 2, "the tall box is dropped");
    // Widest raw crop is 116 px, so everything scales by ceil(1200 / 116) = 11.
    assert_eq!((crops[0].width, crops[0].height), (116 * 11, 36 * 11));
    assert_eq!(
        crops[0].data.len(),
        (crops[0].width * crops[0].height * 3) as usize
    );
    assert!(detect::extract_and_upscale_crops(&frame, &[]).is_empty());
}

// ---- line clean-up (ports of rivenOcrOnnx.test.ts) ----

fn read(lines: &[(&str, f32)]) -> Vec<LineRead> {
    lines
        .iter()
        .map(|&(text, confidence)| LineRead {
            text: text.into(),
            confidence,
            char_confidences: Vec::new(),
        })
        .collect()
}

fn texts(lines: &[LineRead]) -> Vec<&str> {
    lines.iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn postprocess_fixes_known_misreads() {
    let fix = lines::postprocess_text;
    assert_eq!(fix("*-74,2%Impact"), "-74,2% Impact");
    assert_eq!(fix(">Impact"), "Impact");
    assert_eq!(fix("+60%Heat"), "+60% Heat");
    assert_eq!(fix("+12% Damageto Grineer"), "+12% Damage to Grineer");
    assert_eq!(fix("A1,6 Damage to Infested"), "x1,6 Damage to Infested");
    assert_eq!(fix("+10% Mmpact"), "+10% Impact");
    assert_eq!(fix("+i29,1% Heat"), "+129,1% Heat");
    assert_eq!(fix("xi,5 Damage"), "x1,5 Damage");
    assert_eq!(fix("197.,9%"), "197,9%");
    assert_eq!(fix("+1 56,2% Heat"), "+156,2% Heat");
    assert_eq!(fix("+10% Aditional Combo"), "+10% Additional Combo");
    assert_eq!(fix("+10% Fire Rate"), "+10% Fire Rate");
}

#[test]
fn has_low_confidence_line_ignores_a_garbled_mr_badge() {
    // "X(m R9" once gated a perfect four-stat read at 0.92+.
    let card = read(&[
        ("+72.7% Fire Rate (X2 for", 0.93),
        ("-66.2% Weapon Recoil", 0.92),
        ("+85.7% Multishot", 0.94),
        ("-65,1% Status Duration", 0.92),
        ("X(m R9", 0.57),
    ]);
    assert!(!has_low_confidence_line(&card));
}

#[test]
fn has_low_confidence_line_still_gates_stat_and_multiplier_lines() {
    assert!(has_low_confidence_line(&read(&[(
        "-66.2% Weapon Recoil",
        0.7
    )])));
    assert!(has_low_confidence_line(&read(&[(
        "x2 Combo Duration",
        0.7
    )])));
}

#[test]
fn merge_joins_a_wrapped_name_across_the_glyph_line_between_its_halves() {
    let heavy = read(&[
        ("Hate Ignisus", 0.9271),
        ("+173,4% ( slash", 0.8698),
        ("+132% WHeat", 0.9007),
        ("-41,2% Heavy Attack", 0.93),
        ("北", 0.0391),
        ("Efficiency", 0.9011),
    ]);
    let merged = merge_split_lines(heavy);
    assert_eq!(merged[3..].len(), 1);
    assert_eq!(merged[3].text, "-41,2% Heavy Attack Efficiency");
    assert_eq!(merged[3].confidence, 0.9011);

    let furis = merge_split_lines(read(&[
        ("Furis Heratin", 0.9219),
        ("+100,9% Zo0m", 0.9379),
        ("+60,2% Magazine", 0.955),
        ("和", 0.3573),
        ("Capacity", 0.9655),
    ]));
    assert_eq!(furis[2].text, "+60,2% Magazine Capacity");
    let boar = merge_split_lines(read(&[
        ("Boar Purabin", 0.9418),
        ("x1,63 Damage to Infested", 0.9652),
        ("+114,9% Ammo", 0.9064),
        ("江", 0.0653),
        ("Maximum", 0.8659),
    ]));
    assert_eq!(boar[2].text, "+114,9% Ammo Maximum");
}

#[test]
fn merge_keeps_a_glyph_line_that_splits_no_name() {
    let input = read(&[
        ("+104% Status Duration", 0.9864),
        ("北", 0.1157),
        ("x0,6 Damage to Corpus", 0.8994),
    ]);
    let merged = merge_split_lines(input.clone());
    assert_eq!(texts(&merged), texts(&input));
}

#[test]
fn merge_keeps_each_confidence_on_its_own_line() {
    // Per-box reads of two benchmark crops: the stray "1" box once lent its confidence to the merged
    // line it is a substring of, which failed the gate.
    let merged = merge_split_lines(read(&[
        ("Hate Locti-plecicta", 0.909),
        ("+89,6% Finisher Damage", 0.935),
        ("+102,4% Critical Chance", 0.941),
        ("for Slide Attack", 0.884),
        ("1", 0.118),
        ("+1,7Range", 0.865),
    ]));
    assert_eq!(merged[2].text, "+102,4% Critical Chance for Slide Attack");
    assert_eq!(merged[2].confidence, 0.884);
    assert_eq!(
        (merged[3].text.as_str(), merged[3].confidence),
        ("1", 0.118)
    );
    assert!(!has_low_confidence_line(&merged));

    let ammo = merge_split_lines(read(&[
        ("Boar Toxibin", 0.866),
        ("+108,4% Ammo", 0.935),
        ("Maximum", 0.957),
        ("1", 0.137),
        ("+108,5% :Toxin", 0.936),
    ]));
    assert!(!has_low_confidence_line(&ammo));
}

fn with_confs(text: &str, char_confidences: &[f32]) -> LineRead {
    LineRead {
        text: text.into(),
        confidence: 0.0,
        char_confidences: char_confidences.to_vec(),
    }
}

#[test]
fn trimmed_confidence_leaves_the_element_icon_after_the_value_out() {
    // Per-glyph confidences of a benchmark card (owner-checked +167.3 Impact).
    let impact = with_confs(
        "+167,3% 入lmpact ",
        &[
            0.7618, 0.5309, 0.8544, 0.992, 0.821, 0.7718, 0.9932, 0.6968, 0.595, 0.3912, 0.9983,
            0.998, 0.9983, 0.5947, 0.9896, 0.5037,
        ],
    );
    assert!(lines::trimmed_line_confidence(&impact) >= LOW_CONFIDENCE_THRESHOLD);
}

#[test]
fn trimmed_confidence_still_counts_glyphs_standing_in_for_letters() {
    let confs = [0.9, 0.9, 0.9, 0.9, 0.9, 0.2, 0.2, 0.9, 0.9, 0.9];
    let got = lines::trimmed_line_confidence(&with_confs("+45% 入入eat", &confs));
    assert!((got - 0.76).abs() < 1e-6, "{got}");
}

#[test]
fn trimmed_confidence_still_counts_a_glyph_read_inside_the_value() {
    let mut confs = [0.9f32; 14];
    confs[2] = 0.1;
    let got = lines::trimmed_line_confidence(&with_confs("+1入7,3% Impact", &confs));
    assert!((got - 11.8 / 14.0).abs() < 1e-6, "{got}");
}

#[test]
fn min_confidence_looks_at_stat_lines_only() {
    let card = read(&[
        ("Sobek Croni-zetican", 0.5),
        ("+72,7% Fire Rate", 0.9),
        ("-66,2% Weapon Recoil", 0.85),
        ("MR 9", 0.3),
    ]);
    assert_eq!(lines::min_confidence(&card), 0.85);
    assert_eq!(lines::min_confidence(&read(&[("MR 9", 0.3)])), 0.3);
    assert_eq!(lines::min_confidence(&[]), -1.0);
}

// ---- with the real runtime and models (resources/, fetched by `pnpm fetch-onnx`) ----

#[cfg(windows)]
fn bundled() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")
}

#[cfg(windows)]
fn fixture(name: &str) -> RgbaImage {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/riven")
        .join(name);
    ::image::open(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .to_rgba8()
}

/// A saved card crop put back where it was cut from, in a black frame of the size it was cut from.
#[cfg(windows)]
fn frame_with(card: &RgbaImage, rect: CropRect, (w, h): (u32, u32)) -> RgbaImage {
    let mut frame = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    let at = img::rect_pixels(rect, img::frame_canvas_content_rect(w, h), (w, h));
    assert_eq!(
        (at.width, at.height),
        card.dimensions(),
        "fixture matches the crop rect"
    );
    ::image::imageops::replace(&mut frame, card, at.x as i64, at.y as i64);
    frame
}

#[cfg(windows)]
fn flat(read: &CardRead) -> String {
    read.result.text.to_lowercase().replace(',', ".")
}

// The runtime is the Windows DLL until a Linux .so is added.
#[cfg(windows)]
#[test]
fn scan_reads_a_roll_card_with_a_trait_lock_and_a_wrapped_name() {
    let frame = frame_with(
        &fixture("roll-card-corufell-trait-locked.png"),
        ROLL_CARD,
        (1920, 1080),
    );
    let card = scan_card(&bundled(), &frame, ROLL_CARD, &Default::default()).expect("scan");
    let text = flat(&card);
    // The three-line "Critical Chance (x2 for Heavy Attacks)" stays split here; joining it is the parser's (T14).
    for want in [
        "+110.1% finisher damage",
        "+88.1%",
        "+166.4% critical",
        "-101.5%",
    ] {
        assert!(text.contains(want), "missing {want:?} in {text:?}");
    }
    assert_eq!(lines::stat_line_count(&card.result.lines), 4);
    assert_eq!(card.stats.len(), 4, "{:?}", card.stats);
    assert!(!card.low_confidence, "{:?}", card.result.lines);
}

// The runtime is the Windows DLL until a Linux .so is added.
#[cfg(windows)]
#[test]
fn scan_reads_a_small_ui_card_through_the_retry_crop() {
    let frame = frame_with(
        &fixture("initial-card-sobek-small-ui.png"),
        SINGLE_CARD,
        (1808, 1017),
    );
    let card = scan_card(&bundled(), &frame, SINGLE_CARD, &Default::default()).expect("scan");
    let text = flat(&card);
    for want in [
        "+72.7%",
        "fire rate",
        "-66.2% weapon recoil",
        "+85.7% multishot",
        "-65.1% status duration",
    ] {
        assert!(
            text.contains(want),
            "missing {want:?} in {text:?} (attempts {})",
            card.attempts
        );
    }
    assert_eq!(lines::stat_line_count(&card.result.lines), 4);
    assert_eq!(card.stats.len(), 4, "{:?}", card.stats);
    assert!(!card.incomplete);
}

// The runtime is the Windows DLL until a Linux .so is added.
#[cfg(windows)]
#[test]
fn scan_of_a_blank_frame_reads_no_lines() {
    let frame = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
    let card = scan_card(&bundled(), &frame, SINGLE_CARD, &Default::default()).expect("scan");
    assert!(card.result.lines.is_empty());
    assert_eq!(card.result.min_confidence, -1.0);
}

#[test]
fn scan_reports_unavailable_without_the_runtime_or_models() {
    let nowhere = std::env::temp_dir().join("companionframe-no-onnx");
    let frame = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 255]));
    assert!(matches!(
        scan_card(&nowhere, &frame, SINGLE_CARD, &Default::default()),
        Err(ScanError::Unavailable)
    ));
}

fn screen_fixture(name: &str) -> RgbaImage {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/riven/screens")
        .join(name);
    ::image::open(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .to_rgba8()
}

/// Centre of the first located card as a fraction of the canvas width.
fn located_centre(frame: &RgbaImage) -> f32 {
    let canvas = img::frame_canvas_content_rect(frame.width(), frame.height());
    let cards = img::locate_cards(frame);
    let c = cards.first().unwrap_or_else(|| panic!("no card found"));
    ((c.x - canvas.x) as f32 + c.width as f32 / 2.0) / canvas.width as f32
}

#[test]
fn the_locator_finds_the_new_card_of_the_compare_screen_not_the_dimmed_one() {
    // stats_box centre in expected.json: x 582 of 1539
    let c = located_centre(&screen_fixture("compare-orthos-fr.png"));
    assert!((c - 0.378).abs() < 0.03, "centre {c}");
}

#[test]
fn the_locator_finds_the_card_left_of_centre_on_the_accepted_screen() {
    // stats_box centre in expected.json: x 357 of 1300
    let c = located_centre(&screen_fixture("accepted-orthos-fr.png"));
    assert!((c - 0.275).abs() < 0.03, "centre {c}");
}

#[test]
fn the_locator_finds_a_centred_card() {
    let card = screen_fixture("../initial-card-orthos-fr.png");
    let mut frame = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
    ::image::imageops::replace(&mut frame, &card, (1920 - card.width() as i64) / 2, 400);
    let c = located_centre(&frame);
    assert!((c - 0.5).abs() < 0.04, "centre {c}");
}

#[test]
fn a_frame_without_a_card_leaves_the_rect_alone() {
    let blank = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
    assert_eq!(img::align_rect(&blank, SINGLE_CARD), SINGLE_CARD);
}

#[test]
fn the_locator_box_spans_title_to_footer_on_the_staticor_card() {
    // Title top ~525, "PM 15" pill bottom ~740 in compare-staticor-fr.png (expected.json).
    let c = img::locate_cards(&screen_fixture("compare-staticor-fr.png"))[0];
    assert!(c.y <= 530 && c.y + c.height >= 740, "{c:?}");
    assert!(c.height as f32 >= c.width as f32 * 0.65, "{c:?}");
}

#[test]
fn a_panel_that_lost_its_title_to_glare_grows_back_up_to_the_card_proportions() {
    // Blank the title and first stat (rows 520-600): the dark panel keeps only the lower rows.
    let mut f = screen_fixture("compare-staticor-fr.png");
    for y in 520..600 {
        for x in 400..720 {
            f.put_pixel(x, y, Rgba([0, 0, 0, 255]));
        }
    }
    let c = img::locate_cards(&f)[0];
    assert!(c.height as f32 >= c.width as f32 * 0.65, "{c:?}");
    assert!(c.y <= 560, "{c:?}");
}

#[test]
fn taller_grows_about_the_centre_and_stays_on_the_canvas() {
    let r = img::taller(
        img::CropRect {
            x: 0.2,
            y: 0.3,
            width: 0.3,
            height: 0.4,
        },
        1.5,
    );
    assert!((r.y - 0.2).abs() < 1e-6 && (r.height - 0.6).abs() < 1e-6 && r.x == 0.2);
    let r = img::taller(
        img::CropRect {
            x: 0.2,
            y: 0.1,
            width: 0.3,
            height: 0.9,
        },
        2.0,
    );
    assert_eq!((r.y, r.height), (0.0, 1.0));
}

#[test]
fn only_a_narrow_card_counts_as_small() {
    let f = screen_fixture("compare-staticor-fr.png");
    let card = img::locate_cards(&f)[0];
    assert!(!img::card_is_small(&f, card));
    assert!(img::card_is_small(&f, img::Rect { width: 150, ..card }));
}

/// A dark text panel `w` wide at `x` with stat-line bars of brightness `text`.
fn paint_panel(frame: &mut RgbaImage, x: u32, w: u32, text: u8) {
    for y in 500..900 {
        for xx in x..x + w {
            let bar = (640..860).contains(&y)
                && (y / 12) % 3 != 0
                && xx > x + 20
                && xx < x + w - 20
                && (xx / 3) % 3 != 0;
            let v = if bar { text } else { 20 };
            frame.put_pixel(xx, y, Rgba([v, v, v, 255]));
        }
    }
}

#[test]
fn a_dimmed_card_a_hair_from_the_bright_one_is_located_on_its_own() {
    // The compare screen at 1920x1080: the greyed Current card sits ~35 px left of the new one, and
    // the locator's smoothing used to fuse them into one run too wide for a card.
    let mut frame = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
    paint_panel(&mut frame, 480, 270, 135);
    paint_panel(&mut frame, 790, 290, 220);
    frame
        .save(std::env::var("DBG_OUT").unwrap_or_default() + "/syn.png")
        .ok();
    let bright = img::locate_cards(&frame);
    let old = img::locate_dimmed(&frame, &bright).expect("dimmed card located");
    assert!(old.x + old.width <= bright[0].x, "{old:?} vs {bright:?}");
}

#[test]
fn glow_beside_a_card_does_not_stretch_its_box_or_make_a_second_one() {
    // The accepted screen: bright crystal fragments hug the card, light and unsaturated like text.
    let mut frame = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
    for y in 500..900 {
        for x in 560..820 {
            let v = if (x / 7 + y / 5) % 2 == 0 { 200 } else { 40 };
            frame.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    paint_panel(&mut frame, 820, 280, 220);
    let cards = img::locate_cards(&frame);
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert!(cards[0].x >= 800 && cards[0].width <= 320, "{cards:?}");
}

#[test]
fn a_grid_of_tiles_is_not_a_card() {
    // Relic screen: small bright labels over mid-grey tiles, no centred stat lines.
    let mut frame = RgbaImage::from_pixel(1920, 1080, Rgba([30, 30, 30, 255]));
    for y in (600..760).step_by(40) {
        for x in 10..120 {
            frame.put_pixel(x, y, Rgba([230, 230, 230, 255]));
        }
    }
    assert!(img::locate_cards(&frame).is_empty());
}

#[test]
fn nothing_is_dimmed_without_a_bright_card() {
    let mut frame = RgbaImage::from_pixel(1920, 1080, Rgba([0, 0, 0, 255]));
    paint_panel(&mut frame, 480, 270, 135);
    assert!(img::locate_dimmed(&frame, &[]).is_none());
}

#[test]
fn the_locator_splits_a_three_stat_new_card_from_the_dimmed_old_one() {
    let f = screen_fixture("compare-staticor-hexatron-fr.png");
    let new = img::locate_cards(&f);
    assert_eq!(new.len(), 1, "{new:?}");
    // Title ~510 to the "PM 15" pill ~725 of the bright card (x 510..830), left of the overlay panel.
    assert!(
        new[0].y <= 515 && new[0].y + new[0].height >= 720,
        "{new:?}"
    );
    assert!(new[0].x >= 470 && new[0].x + new[0].width <= 840, "{new:?}");
    let old = img::locate_dimmed(&f, &new).expect("old card");
    assert!(old.x + old.width <= new[0].x && old.y >= 360, "{old:?}");
}

// The runtime is the Windows DLL until a Linux .so is added.
#[cfg(windows)]
#[test]
fn scan_reads_the_whole_three_stat_new_card_of_the_compare_screen() {
    let f = screen_fixture("compare-staticor-hexatron-fr.png");
    let new = img::locate_cards(&f)[0];
    let rect = img::align_rect_to(&f, img::ROLL_CARD, new);
    let card = scan_card(&bundled(), &f, rect, &Default::default()).expect("scan");
    let text = flat(&card);
    for want in [
        "staticor hexatron",
        "146.5%",
        "x1.66",
        "corpus",
        "+51.2%",
        "recul",
    ] {
        assert!(
            text.to_lowercase().contains(want),
            "missing {want:?} in {text:?}"
        );
    }
    assert!(!text.contains("0.62"), "old card's line leaked: {text:?}");
}
