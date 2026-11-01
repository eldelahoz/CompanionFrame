//! ONNX Runtime plumbing shared by the reward and riven scanners.
//!
//! `ort` runs in `load-dynamic` mode: `onnxruntime.dll` ships as a Tauri resource (fetched by
//! `scripts/fetch-onnx.mjs`) and is loaded by absolute path, so an older copy in `System32` can
//! never win the search. A missing DLL or model leaves the scanners unavailable, never panicking.

mod det;
mod paddle;
mod session;

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;
use tauri::Manager;

pub use det::{Det, det};
pub use paddle::{LineRead, Paddle, PaddleModel, RgbCrop, paddle, paddle_model, set_language};

const DLL: &str = "onnxruntime/onnxruntime.dll";
const PADDLE_MODEL: &str = "riven-ocr/paddle/ch_PP-OCRv3_rec_infer.onnx";
const PADDLE_DICT: &str = "riven-ocr/paddle/ch_dict.txt";
/// RapidOCR's ONNX export of PaddleOCR's PP-OCRv5 Latin recognizer (accented Latin letters).
const LATIN_MODEL: &str = "riven-ocr/paddle/latin_PP-OCRv5_rec_mobile_infer.onnx";
const LATIN_DICT: &str = "riven-ocr/paddle/ppocrv5_latin_dict.txt";
/// RapidOCR's ONNX export of PaddleOCR's PP-OCRv5 mobile text detector: finds the riven stat lines.
const DET_MODEL: &str = "riven-ocr/paddle/ch_PP-OCRv5_mobile_det.onnx";

/// True when the DLL and the Paddle recognizer files are on disk (a cheap check; nothing is loaded).
pub fn reward_ocr_onnx_available(root: &Path) -> bool {
    [DLL, PADDLE_MODEL, PADDLE_DICT]
        .iter()
        .all(|p| root.join(p).is_file())
}

/// True when the DLL, the text detector and the Paddle recognizer are on disk (nothing is loaded).
pub fn riven_ocr_onnx_available(root: &Path) -> bool {
    reward_ocr_onnx_available(root) && root.join(DET_MODEL).is_file()
}

#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct OnnxStatus {
    pub runtime_present: bool,
    pub paddle_present: bool,
    pub detector_present: bool,
    /// The runtime and the Paddle session actually loaded.
    pub ready: bool,
    pub error: Option<String>,
}

pub fn status(root: &Path) -> OnnxStatus {
    let has = |p: &str| root.join(p).is_file();
    let (ready, error) = match session::environment(&root.join(DLL)).and_then(|_| paddle(root)) {
        Ok(_) => (true, None),
        Err(e) => (false, Some(e)),
    };
    OnnxStatus {
        runtime_present: has(DLL),
        paddle_present: has(PADDLE_MODEL) && has(PADDLE_DICT),
        detector_present: has(DET_MODEL),
        ready,
        error,
    }
}

/// Dev: next to the exe; installed: the install dir. Same resolution as `bundled_data`.
pub fn resource_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().resource_dir().map_err(|e| e.to_string())
}

/// Loads the runtime and the Paddle model (a second or two the first time).
#[tauri::command]
#[specta::specta]
pub async fn ocr_onnx_status(app: tauri::AppHandle) -> Result<OnnxStatus, String> {
    let root = resource_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || status(&root))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
