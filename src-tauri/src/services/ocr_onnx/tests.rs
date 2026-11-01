use std::path::Path;

use super::paddle::{self, Paddle, RgbCrop};
use super::*;

#[cfg(windows)]
fn bundled() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")
}

fn dict() -> Vec<String> {
    ["blank", "a", "b", "ab"].map(String::from).to_vec()
}

/// Row-major [seq][classes] logits that argmax to `path`.
fn logits(path: &[usize], classes: usize) -> Vec<f32> {
    path.iter()
        .flat_map(|&p| (0..classes).map(move |c| if c == p { 0.9 } else { 0.01 }))
        .collect()
}

#[test]
fn ctc_collapses_repeats_and_drops_blanks() {
    // a a blank a b b  ->  "aab"; the blank between the two `a` runs keeps both.
    let read = paddle::ctc_greedy_decode(&logits(&[1, 1, 0, 1, 2, 2], 4), 0, 6, 4, &dict());
    assert_eq!(read.text, "aab");
    assert_eq!(read.char_confidences.len(), 3);
    assert!((read.confidence - 0.9).abs() < 1e-6);
}

#[test]
fn ctc_empty_and_out_of_dict() {
    let empty = paddle::ctc_greedy_decode(&logits(&[0, 0, 0], 4), 0, 3, 4, &dict());
    assert_eq!((empty.text.as_str(), empty.confidence), ("", 0.0));
    // class 5 has no dictionary entry (the model has more classes than the dict file).
    assert_eq!(
        paddle::ctc_greedy_decode(&logits(&[5, 1], 6), 0, 2, 6, &dict()).text,
        "a"
    );
}

#[test]
fn ctc_multichar_entry_repeats_its_confidence_per_char() {
    let read = paddle::ctc_greedy_decode(&logits(&[3], 4), 0, 1, 4, &dict());
    assert_eq!((read.text.as_str(), read.char_confidences.len()), ("ab", 2));
}

#[test]
fn ctc_reads_the_requested_batch_item() {
    let mut preds = logits(&[1, 1], 4);
    preds.extend(logits(&[2, 2], 4));
    assert_eq!(
        paddle::ctc_greedy_decode(&preds, 1, 2, 4, &dict()).text,
        "b"
    );
}

#[test]
fn chunks_group_by_width_and_drop_budget_busters() {
    // widths: three similar lines, a much wider one, and a rule far over the decode budget.
    let widths = [300, 310, 305, 900, 1_000_000];
    let chunks = paddle::recognition_chunks(&widths, 6625);
    assert_eq!(chunks, vec![vec![0, 2, 1], vec![3]]);
}

#[test]
fn chunks_cap_batch_size() {
    let chunks = paddle::recognition_chunks(&[200; 13], 6625);
    assert_eq!(
        chunks.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![6, 6, 1]
    );
}

#[test]
fn crop_width_keeps_aspect_at_48px() {
    let crop = RgbCrop {
        width: 420,
        height: 64,
        data: vec![],
    };
    assert_eq!(paddle::width_at_48(&crop), 315);
}

#[test]
fn missing_runtime_reports_unavailable_without_panicking() {
    let empty = std::env::temp_dir().join("cf-onnx-missing");
    assert!(!reward_ocr_onnx_available(&empty));
    let st = status(&empty);
    assert!(!st.ready && !st.runtime_present);
    assert!(st.error.unwrap().contains("onnxruntime.dll not found"));
}

#[test]
fn relative_dll_path_is_rejected() {
    assert!(Paddle::load(Path::new("resources")).is_err());
}

/// Needs `pnpm fetch-onnx` (CI runs it); skipped on a checkout without the binaries.
// The runtime is the Windows DLL until a Linux .so is added.
#[cfg(windows)]
#[test]
fn loads_bundled_dll_and_reads_a_text_strip() {
    let root = bundled();
    if !reward_ocr_onnx_available(&root) {
        eprintln!("skipped: run `pnpm fetch-onnx`");
        return;
    }
    let paddle = Paddle::load(&root).expect("bundled onnxruntime + paddle model load");
    let img = image::open(root.join("../tests/fixtures/ocr_onnx/strip.png"))
        .unwrap()
        .to_rgb8();
    let crop = RgbCrop {
        width: img.width(),
        height: img.height(),
        data: img.into_raw(),
    };
    let reads = paddle.recognize(&[crop]).unwrap();
    // The model drops spaces at word boundaries (WFHelper re-inserts them from the casing).
    assert_eq!(reads[0].text.replace(' ', ""), "BroncoPrimeReceiver");
    assert!(
        reads[0].confidence > 0.8,
        "confidence {}",
        reads[0].confidence
    );
    assert_eq!(
        reads[0].char_confidences.len(),
        reads[0].text.chars().count()
    );
}

#[test]
fn dict_gets_one_space_class_and_no_trailing_empty_entry() {
    // The Latin dict file has no space line; the CH one ends with it.
    assert_eq!(paddle::parse_dict("a\nb\n"), ["blank", "a", "b", " "]);
    assert_eq!(paddle::parse_dict("a\r\n \n"), ["blank", "a", " "]);
}

#[test]
fn latin_languages_get_the_latin_model() {
    assert_eq!(PaddleModel::for_language("fr"), PaddleModel::Latin);
    assert_eq!(PaddleModel::for_language("en"), PaddleModel::Latin);
    assert_eq!(PaddleModel::for_language("de"), PaddleModel::Latin);
    assert_eq!(PaddleModel::for_language("zh"), PaddleModel::Ch);
    assert_eq!(PaddleModel::for_language("ru"), PaddleModel::Ch);
}

#[cfg(windows)]
fn read_strip(model: PaddleModel, file: &str) -> Option<String> {
    let root = bundled();
    let Ok(paddle) = Paddle::load_model(&root, model) else {
        eprintln!("skipped: run `pnpm fetch-onnx`");
        return None;
    };
    let img = image::open(root.join("../tests/fixtures/ocr_onnx").join(file))
        .unwrap()
        .to_rgb8();
    let crop = RgbCrop {
        width: img.width(),
        height: img.height(),
        data: img.into_raw(),
    };
    Some(paddle.recognize(&[crop]).unwrap().remove(0).text)
}

#[cfg(windows)]
#[test]
fn latin_model_keeps_accents() {
    if let Some(fr) = read_strip(PaddleModel::Latin, "strip_fr.png") {
        assert_eq!(fr.trim_end_matches(['_', ' ']), "Volt Prime Châssis Schéma");
    }
}
