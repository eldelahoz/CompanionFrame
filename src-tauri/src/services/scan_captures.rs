//! Opt-in troubleshooting ("Save scan captures"): each reward/riven scan's frame as a PNG plus the
//! raw OCR text, in `<logs>/scans/`, newest `KEEP` kept. Local files only; nothing is uploaded.

use crate::services::sync::LockExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use image::RgbaImage;

use crate::store::Settings;

const KEEP: usize = 20;

static ENABLED: AtomicBool = AtomicBool::new(false);
static DIR: OnceLock<PathBuf> = OnceLock::new();
/// The last frame a reward attempt captured, written when the round ends.
static KEPT: Mutex<Option<RgbaImage>> = Mutex::new(None);

pub fn init(log_dir: &Path, s: &Settings) {
    let _ = DIR.set(log_dir.join("scans"));
    apply(s);
}

pub fn apply(s: &Settings) {
    ENABLED.store(s.save_scan_captures, Ordering::Relaxed);
    if !s.save_scan_captures {
        *KEPT.lock_any() = None;
    }
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Holds a copy of the frame for [`save_kept`]; free when the setting is off.
pub fn keep(img: &RgbaImage) {
    if enabled() {
        *KEPT.lock_any() = Some(img.clone());
    }
}

/// Writes the kept frame and `text`. Blocking (PNG encode).
pub fn save_kept(kind: &str, text: &str) {
    let img = KEPT.lock_any().take();
    save(kind, img.as_ref(), text);
}

/// Writes `<ms>-<kind>.png` and `.txt`, then drops all but the newest captures. Blocking.
pub fn save(kind: &str, img: Option<&RgbaImage>, text: &str) {
    let (true, Some(dir)) = (enabled(), DIR.get()) else {
        return;
    };
    if let Err(e) = write(dir, kind, img, text) {
        tracing::warn!("scan capture not saved: {e}");
    }
}

fn write(dir: &Path, kind: &str, img: Option<&RgbaImage>, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let stem = dir.join(format!("{ms}-{kind}"));
    if let Some(img) = img {
        img.save(stem.with_extension("png"))
            .map_err(|e| e.to_string())?;
    }
    std::fs::write(stem.with_extension("txt"), text).map_err(|e| e.to_string())?;
    prune(dir, KEEP);
    Ok(())
}

/// Keeps the `keep` newest captures (a capture = the files sharing a `<ms>-<kind>` stem).
fn prune(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    let mut stems: Vec<String> = files
        .iter()
        .filter_map(|p| p.file_stem()?.to_str().map(str::to_owned))
        .collect();
    // Stems start with the epoch ms, all the same width for centuries.
    stems.sort_unstable_by(|a, b| b.cmp(a));
    stems.dedup();
    for old in stems.iter().skip(keep) {
        for f in files
            .iter()
            .filter(|p| p.file_stem().and_then(|s| s.to_str()) == Some(old))
        {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_the_newest_captures_with_both_files() {
        let dir = std::env::temp_dir().join(format!("cf-scans-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for ms in 1_000_000..1_000_025u64 {
            std::fs::write(dir.join(format!("{ms}-reward.txt")), "x").unwrap();
            std::fs::write(dir.join(format!("{ms}-reward.png")), "x").unwrap();
        }
        prune(&dir, 20);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), 40);
        assert_eq!(left[0], "1000005-reward.png");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
