//! Windows.Media.Ocr as a reward-scan reader; the pipeline itself is `services::reward_scan`.
use image::RgbaImage;

use super::ocr;
use crate::services::reward_scan::RewardReader;

pub struct WindowsReader;

impl RewardReader for WindowsReader {
    fn ocr_lines(&self, strip: &RgbaImage) -> Vec<String> {
        ocr::recognize(strip)
            .map(|lines| lines.into_iter().map(|l| l.text).collect())
            .unwrap_or_default()
    }
}
