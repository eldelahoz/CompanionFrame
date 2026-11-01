//! PaddleOCR recognizer: batches text-line crops through the model and CTC-decodes the output.
//! Port of the recognizer half of WFHelper's `rivenOcrOnnx.ts`; the reward and riven scanners share it.
//! Two models: CH v3 (Chinese + English, no accents) and the multilingual Latin v5 (keeps accents).

use crate::services::sync::LockExt;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use image::RgbImage;
use image::imageops::{self, FilterType};
use ort::session::Session;
use ort::value::Tensor;

use super::{DLL, LATIN_DICT, LATIN_MODEL, PADDLE_DICT, PADDLE_MODEL, session};

/// A raw RGB crop (`data.len() == width * height * 3`).
pub struct RgbCrop {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LineRead {
    pub text: String,
    /// Mean softmax confidence of the decoded characters (0 when nothing decoded).
    pub confidence: f32,
    /// One entry per `char` of `text`.
    pub char_confidences: Vec<f32>,
}

// PP-OCRv3 pads every crop in a batch to the widest one, so an 80:1 panel rule beside nine text
// lines costs hundreds of MB. Crops are grouped by width and budget-busting singles are dropped.
const IMG_H: u32 = 48;
const MAX_CHUNK: usize = 6;
const OUTPUT_BUDGET: usize = 48 * 1024 * 1024;
const MAX_PAD_STRETCH: u32 = 2;

pub struct Paddle {
    session: Mutex<Session>,
    /// Index 0 is the CTC blank, as in the reference.
    dict: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaddleModel {
    Ch,
    Latin,
}

impl PaddleModel {
    fn files(self) -> (&'static str, &'static str) {
        match self {
            Self::Ch => (PADDLE_MODEL, PADDLE_DICT),
            Self::Latin => (LATIN_MODEL, LATIN_DICT),
        }
    }

    /// The model for a game locale code: Latin-script languages read with the Latin model
    /// (accents kept, and English reads better: docs/ocr-fixtures.md); the rest (zh, ja, ru...) with CH.
    pub fn for_language(code: &str) -> Self {
        match code {
            "en" | "de" | "es" | "fr" | "it" | "pl" | "pt" | "tr" => Self::Latin,
            _ => Self::Ch,
        }
    }
}

static CH: OnceLock<Paddle> = OnceLock::new();
static LATIN: OnceLock<Paddle> = OnceLock::new();
/// Game locale code the cards are printed in.
static LANGUAGE: Mutex<&'static str> = Mutex::new("en");

/// Reads with this game language's model from the next scan on.
pub fn set_language(code: &'static str) {
    *LANGUAGE.lock_any() = code;
}

/// The process-wide recognizer for the game language, loaded on first use; CH when the Latin
/// model is missing. Failures are not cached, so a DLL dropped in later is picked up.
pub fn paddle(root: &Path) -> Result<&'static Paddle, String> {
    let lang = *LANGUAGE.lock_any();
    match PaddleModel::for_language(lang) {
        PaddleModel::Latin => {
            paddle_model(root, PaddleModel::Latin).or_else(|_| paddle_model(root, PaddleModel::Ch))
        }
        PaddleModel::Ch => paddle_model(root, PaddleModel::Ch),
    }
}

/// One model's process-wide recognizer.
pub fn paddle_model(root: &Path, model: PaddleModel) -> Result<&'static Paddle, String> {
    let cell = match model {
        PaddleModel::Ch => &CH,
        PaddleModel::Latin => &LATIN,
    };
    if let Some(p) = cell.get() {
        return Ok(p);
    }
    let p = Paddle::load_model(root, model)?;
    Ok(cell.get_or_init(|| p))
}

/// Index 0 is the CTC blank; PaddleOCR appends the space class when the dict file lacks it.
pub(super) fn parse_dict(text: &str) -> Vec<String> {
    // A stray `\r` on a character would corrupt the decoding.
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    if !lines.iter().any(|l| l == " ") {
        lines.push(" ".into());
    }
    std::iter::once("blank".to_string()).chain(lines).collect()
}

impl Paddle {
    pub fn load(root: &Path) -> Result<Self, String> {
        Self::load_model(root, PaddleModel::Ch)
    }

    pub fn load_model(root: &Path, model: PaddleModel) -> Result<Self, String> {
        let (model_file, dict_file) = model.files();
        let env = session::environment(&root.join(DLL))?;
        let session = session::load_model(env, &root.join(model_file))?;
        let dict_path = root.join(dict_file);
        let dict = std::fs::read_to_string(&dict_path)
            .map_err(|e| format!("{}: {e}", dict_path.display()))?;
        Ok(Self {
            session: Mutex::new(session),
            dict: parse_dict(&dict),
        })
    }

    /// One result per crop, in the caller's order; a skipped crop (over the decode budget) reads empty.
    pub fn recognize(&self, crops: &[RgbCrop]) -> Result<Vec<LineRead>, String> {
        let mut out = vec![LineRead::default(); crops.len()];
        let widths: Vec<u32> = crops.iter().map(width_at_48).collect();
        for chunk in recognition_chunks(&widths, self.dict.len()) {
            let reads =
                self.recognize_chunk(&chunk.iter().map(|&i| &crops[i]).collect::<Vec<_>>())?;
            for (i, read) in chunk.into_iter().zip(reads) {
                out[i] = read;
            }
        }
        Ok(out)
    }

    fn recognize_chunk(&self, crops: &[&RgbCrop]) -> Result<Vec<LineRead>, String> {
        let img_w = crops.iter().map(|c| width_at_48(c)).max().unwrap_or(1) as usize;
        let plane = IMG_H as usize * img_w;
        // Zero padding matches the reference's `np.zeros`.
        let mut batch = vec![0f32; crops.len() * 3 * plane];
        for (b, crop) in crops.iter().enumerate() {
            let src = RgbImage::from_raw(crop.width, crop.height, crop.data.clone())
                .ok_or("crop buffer does not match its size")?;
            let resized = imageops::resize(
                &src,
                width_at_48(crop).min(img_w as u32),
                IMG_H,
                FilterType::Triangle,
            );
            for (x, y, px) in resized.enumerate_pixels() {
                for c in 0..3 {
                    batch[b * 3 * plane + c * plane + y as usize * img_w + x as usize] =
                        px[c] as f32 / 127.5 - 1.0;
                }
            }
        }
        let tensor = Tensor::from_array(([crops.len(), 3, IMG_H as usize, img_w], batch))
            .map_err(|e| e.to_string())?;
        let mut session = self.session.lock().map_err(|_| "paddle session poisoned")?;
        let input = session.inputs()[0].name().to_string();
        let outputs = session
            .run(ort::inputs![input => tensor])
            .map_err(|e| e.to_string())?;
        let (shape, preds) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        // [batch, seq_len, num_classes]
        if shape.len() != 3 || shape[0] as usize != crops.len() {
            return Err(format!("unexpected recognizer output shape {shape}"));
        }
        let (seq, classes) = (shape[1] as usize, shape[2] as usize);
        Ok((0..crops.len())
            .map(|b| ctc_greedy_decode(preds, b, seq, classes, &self.dict))
            .collect())
    }
}

pub(super) fn width_at_48(crop: &RgbCrop) -> u32 {
    (IMG_H as f32 * crop.width as f32 / crop.height.max(1) as f32)
        .ceil()
        .max(1.0) as u32
}

fn output_bytes(count: usize, img_w: u32, classes: usize) -> usize {
    count * img_w.div_ceil(8) as usize * classes.max(1) * 4
}

/// Crop indices (by ascending width) in groups small enough to decode together.
pub(super) fn recognition_chunks(widths: &[u32], classes: usize) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..widths.len()).collect();
    order.sort_by_key(|&i| widths[i]);
    let mut chunks = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut cur_w = 0;
    for i in order {
        let w = widths[i];
        // Real stat lines top out near 59:1: a crop over budget on its own is a detector artifact.
        if output_bytes(1, w, classes) > OUTPUT_BUDGET {
            continue;
        }
        let merged = cur_w.max(w);
        let fits = cur.len() < MAX_CHUNK
            && cur
                .first()
                .is_none_or(|&f| merged <= widths[f] * MAX_PAD_STRETCH)
            && output_bytes(cur.len() + 1, merged, classes) <= OUTPUT_BUDGET;
        if !cur.is_empty() && !fits {
            chunks.push(std::mem::take(&mut cur));
            cur_w = 0;
        }
        cur.push(i);
        cur_w = cur_w.max(w);
    }
    if !cur.is_empty() {
        chunks.push(cur);
    }
    chunks
}

/// Greedy CTC: argmax per step, collapse repeats, drop blanks (index 0).
pub(super) fn ctc_greedy_decode(
    preds: &[f32],
    batch: usize,
    seq: usize,
    classes: usize,
    dict: &[String],
) -> LineRead {
    let base = batch * seq * classes;
    let (mut text, mut confs, mut prev) = (String::new(), Vec::new(), 0usize);
    for t in 0..seq {
        let row = &preds[base + t * classes..base + (t + 1) * classes];
        let (best, val) =
            row.iter().enumerate().fold(
                (0, row[0]),
                |acc, (i, &v)| if v > acc.1 { (i, v) } else { acc },
            );
        if best != 0 && best != prev && best < dict.len() {
            text.push_str(&dict[best]);
            // One confidence per char, so a multi-char entry repeats it.
            confs.extend(std::iter::repeat_n(val, dict[best].chars().count()));
        }
        prev = best;
    }
    let confidence = if confs.is_empty() {
        0.0
    } else {
        confs.iter().sum::<f32>() / confs.len() as f32
    };
    LineRead {
        text,
        confidence,
        char_confidences: confs,
    }
}
