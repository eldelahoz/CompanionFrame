//! Reward screen: image pipeline (card count, slot geometry, strip binarisation, line joining) and
//! the text → item matcher it feeds. Pure: the OCR engine sits behind [`RewardReader`] so the
//! Windows engine and the ONNX one (`onnx_reader`) plug in from `platform/` and `services/`.

pub mod counter;
pub mod enrich;
pub mod era;
pub mod flow;
pub mod fold;
pub mod image;
pub mod matcher;
pub mod names;
pub mod onnx_reader;
pub mod payload;
pub mod slots;

use ::image::RgbaImage;

/// One OCR engine over an already binarised strip: returns the recognised text lines, top first.
pub trait RewardReader {
    fn ocr_lines(&self, strip: &RgbaImage) -> Vec<String>;

    /// Lines read from the raw (not binarised) title strip, for engines that do their own row
    /// finding. Empty for readers that only take binarised bands.
    fn raw_lines(&self, _strip: &RgbaImage) -> Vec<String> {
        Vec::new()
    }

    /// Whether a whole-band read may back up an empty slot scan. Off for engines that read the
    /// title strips themselves (ONNX): a band-wide read is out of their input distribution.
    fn text_fallback(&self) -> bool {
        true
    }
}

/// Interface scale WFHelper's slot ratios were measured at.
pub const REFERENCE_UI_SCALE: f64 = 0.99;

#[cfg(test)]
mod enrich_tests;
#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod scan_tests;
#[cfg(test)]
mod tests;
