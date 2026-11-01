//! The riven text-line detector: PaddleOCR's PP-OCRv5 mobile DBNet (`[1, 3, H, W]` in, a `[1, 1, H, W]`
//! text-probability map out; H and W are free, multiples of 32).

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ort::session::Session;
use ort::value::Tensor;

use super::{DET_MODEL, DLL, session};

pub struct Det {
    session: Mutex<Session>,
}

static DET: OnceLock<Det> = OnceLock::new();

/// The process-wide detector, loaded on first use. Failures are not cached.
pub fn det(root: &Path) -> Result<&'static Det, String> {
    if let Some(d) = DET.get() {
        return Ok(d);
    }
    let env = session::environment(&root.join(DLL))?;
    let session = session::load_model(env, &root.join(DET_MODEL))?;
    Ok(DET.get_or_init(|| Det {
        session: Mutex::new(session),
    }))
}

impl Det {
    /// Runs one `[1, 3, h, w]` CHW blob; returns the first output's dims and data.
    pub fn run(&self, blob: Vec<f32>, h: usize, w: usize) -> Result<(Vec<i64>, Vec<f32>), String> {
        let tensor = Tensor::from_array(([1, 3, h, w], blob)).map_err(|e| e.to_string())?;
        let mut session = self.session.lock().map_err(|_| "det session poisoned")?;
        let input = session.inputs()[0].name().to_string();
        let outputs = session
            .run(ort::inputs![input => tensor])
            .map_err(|e| e.to_string())?;
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        Ok((shape.iter().copied().collect(), data.to_vec()))
    }
}
