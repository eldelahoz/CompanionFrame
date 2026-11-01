//! Where the user parks each overlay, per game resolution, as fractions of the monitor (so 1080p and 4K
//! setups each keep their own spot). The setup wizard's placement step writes it; the overlay windows
//! read it (OverlayShell) and, since they span the whole monitor, a fraction maps straight to a position.
use crate::services::sync::LockExt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter as _, Manager as _};

use super::{OverlayKind, OverlayRect, with};
use crate::services::atomic::write_atomic;

const FILE: &str = "overlay-placement.json";
const SCALE_MIN: f64 = 0.6;
const SCALE_MAX: f64 = 1.6;
/// A real monitor is far below this; it bounds the map a bad caller could grow.
const MAX_DIM: u32 = 16_384;
const MAX_RESOLUTIONS: usize = 32;

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub struct Placement {
    pub x_frac: f64,
    pub y_frac: f64,
    pub scale: f64,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            x_frac: 0.0,
            y_frac: 0.0,
            scale: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct PlacementEntry {
    pub kind: OverlayKind,
    pub placement: Placement,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct MonitorInfo {
    pub name: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
    pub primary: bool,
}

/// `resolution -> overlay kind name -> placement`.
type Saved = BTreeMap<String, BTreeMap<String, Placement>>;

fn key(width: u32, height: u32) -> Result<String, String> {
    if !(1..=MAX_DIM).contains(&width) || !(1..=MAX_DIM).contains(&height) {
        return Err("invalid resolution".into());
    }
    Ok(format!("{width}x{height}"))
}

fn load(path: &Path) -> Saved {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Read-modify-write one resolution's entry for `kind`.
fn edit(
    path: &Path,
    width: u32,
    height: u32,
    kind: OverlayKind,
    f: impl FnOnce(&mut Placement),
) -> Result<(), String> {
    let k = key(width, height)?;
    let _g = LOCK.lock_any();
    let mut all = load(path);
    if !all.contains_key(&k) && all.len() >= MAX_RESOLUTIONS {
        return Err("too many saved resolutions".into());
    }
    f(all
        .entry(k)
        .or_default()
        .entry(kind.name().into())
        .or_default());
    let bytes = serde_json::to_vec_pretty(&all).map_err(|e| e.to_string())?;
    write_atomic(path, &bytes).map_err(|e| e.to_string())
}

pub(crate) fn get(path: &Path, width: u32, height: u32) -> Result<Vec<PlacementEntry>, String> {
    let k = key(width, height)?;
    let _g = LOCK.lock_any();
    let mut all = load(path);
    // innerWidth * devicePixelRatio can round a pixel off the monitor size Setup saved under.
    let saved = all
        .remove(&k)
        .or_else(|| {
            let near = all.keys().find(|s| {
                s.split_once('x')
                    .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
                    .is_some_and(|(w, h)| w.abs_diff(width) <= 2 && h.abs_diff(height) <= 2)
            })?;
            all.remove(&near.clone())
        })
        .unwrap_or_default();
    Ok(OverlayKind::ALL
        .into_iter()
        .filter_map(|kind| {
            saved.get(kind.name()).map(|p| PlacementEntry {
                kind,
                placement: *p,
            })
        })
        .collect())
}

pub(crate) fn save_position(
    path: &Path,
    width: u32,
    height: u32,
    kind: OverlayKind,
    x_frac: f64,
    y_frac: f64,
) -> Result<(), String> {
    if !(0.0..=1.0).contains(&x_frac) || !(0.0..=1.0).contains(&y_frac) {
        return Err("position must be within 0..=1".into());
    }
    edit(path, width, height, kind, |p| {
        p.x_frac = x_frac;
        p.y_frac = y_frac;
    })
}

/// The trade toast has a fixed size: it keeps a position but never a scale.
pub(crate) fn save_scale(
    path: &Path,
    width: u32,
    height: u32,
    kind: OverlayKind,
    scale: f64,
) -> Result<(), String> {
    if kind == OverlayKind::Trade || !(SCALE_MIN..=SCALE_MAX).contains(&scale) {
        return Err("scale not allowed".into());
    }
    edit(path, width, height, kind, |p| {
        p.scale = (scale * 100.0).round() / 100.0;
    })
}

pub(crate) fn reset(path: &Path, width: u32, height: u32) -> Result<(), String> {
    let k = key(width, height)?;
    let _g = LOCK.lock_any();
    let mut all = load(path);
    all.remove(&k);
    let bytes = serde_json::to_vec_pretty(&all).map_err(|e| e.to_string())?;
    write_atomic(path, &bytes).map_err(|e| e.to_string())
}

/// Open overlay windows re-read their placement on the overlay settings event.
fn notify(app: &AppHandle, r: Result<(), String>) -> Result<(), String> {
    if r.is_ok() {
        let _ = app.emit(super::settings::CHANGED_EVENT, ());
    }
    r
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(FILE))
}

/// Every connected monitor, for the placement step's picker.
#[tauri::command]
#[specta::specta]
pub async fn overlay_monitors(app: AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let primary = app.primary_monitor().ok().flatten();
    let list = app.available_monitors().map_err(|e| e.to_string())?;
    Ok(list
        .iter()
        .map(|m| MonitorInfo {
            name: m.name().cloned(),
            x: m.position().x,
            y: m.position().y,
            width: m.size().width,
            height: m.size().height,
            scale_factor: m.scale_factor(),
            primary: primary
                .as_ref()
                .is_some_and(|p| p.position() == m.position() && p.size() == m.size()),
        })
        .collect())
}

/// The monitor overlays are drawn on right now (the game's when it runs, else the primary one);
/// `None` before the overlay windows exist or off Windows.
#[tauri::command]
#[specta::specta]
pub async fn overlay_placement_detect(app: AppHandle) -> Result<Option<OverlayRect>, String> {
    Ok(with(&app, |m| m.target_monitor()).unwrap_or(None))
}

#[tauri::command]
#[specta::specta]
pub async fn overlay_placement_get(
    app: AppHandle,
    width: u32,
    height: u32,
) -> Result<Vec<PlacementEntry>, String> {
    get(&path(&app)?, width, height)
}

#[tauri::command]
#[specta::specta]
pub async fn overlay_placement_save(
    app: AppHandle,
    width: u32,
    height: u32,
    kind: OverlayKind,
    x_frac: f64,
    y_frac: f64,
) -> Result<(), String> {
    notify(
        &app,
        save_position(&path(&app)?, width, height, kind, x_frac, y_frac),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn overlay_placement_save_scale(
    app: AppHandle,
    width: u32,
    height: u32,
    kind: OverlayKind,
    scale: f64,
) -> Result<(), String> {
    notify(&app, save_scale(&path(&app)?, width, height, kind, scale))
}

#[tauri::command]
#[specta::specta]
pub async fn overlay_placement_reset(
    app: AppHandle,
    width: u32,
    height: u32,
) -> Result<(), String> {
    notify(&app, reset(&path(&app)?, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cf-place-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(FILE)
    }

    #[test]
    fn positions_are_kept_per_resolution() {
        let f = file("res");
        save_position(&f, 1920, 1080, OverlayKind::Reward, 0.25, 0.5).unwrap();
        save_scale(&f, 1920, 1080, OverlayKind::Reward, 1.234).unwrap();
        save_position(&f, 3840, 2160, OverlayKind::Reward, 0.1, 0.2).unwrap();
        let hd = get(&f, 1920, 1080).unwrap();
        assert_eq!(hd.len(), 1);
        assert_eq!(
            hd[0].placement,
            Placement {
                x_frac: 0.25,
                y_frac: 0.5,
                scale: 1.23
            }
        );
        assert_eq!(get(&f, 3840, 2160).unwrap()[0].placement.scale, 1.0);
        assert!(get(&f, 2560, 1440).unwrap().is_empty());
        // A rounding pixel off still finds the monitor's spot; a different monitor does not.
        assert_eq!(get(&f, 1919, 1080).unwrap().len(), 1);
        assert!(get(&f, 1916, 1080).unwrap().is_empty());
        reset(&f, 1920, 1080).unwrap();
        assert!(get(&f, 1920, 1080).unwrap().is_empty());
        assert_eq!(get(&f, 3840, 2160).unwrap().len(), 1);
    }

    #[test]
    fn rejects_bad_input_and_corrupt_file_falls_back() {
        let f = file("bad");
        assert!(save_position(&f, 0, 1080, OverlayKind::Riven, 0.5, 0.5).is_err());
        assert!(save_position(&f, 1920, 1080, OverlayKind::Riven, 1.5, 0.5).is_err());
        assert!(save_position(&f, 1920, 1080, OverlayKind::Riven, f64::NAN, 0.5).is_err());
        assert!(save_scale(&f, 1920, 1080, OverlayKind::Trade, 1.0).is_err());
        assert!(save_scale(&f, 1920, 1080, OverlayKind::Arbi, 2.0).is_err());
        std::fs::write(&f, b"not json").unwrap();
        assert!(get(&f, 1920, 1080).unwrap().is_empty());
        save_position(&f, 1920, 1080, OverlayKind::Arbi, 0.0, 1.0).unwrap();
        assert_eq!(get(&f, 1920, 1080).unwrap().len(), 1);
    }

    #[test]
    fn resolution_count_is_bounded() {
        let f = file("cap");
        for w in 1..=MAX_RESOLUTIONS as u32 {
            save_position(&f, w, 100, OverlayKind::Arbi, 0.0, 0.0).unwrap();
        }
        assert!(save_position(&f, 9999, 100, OverlayKind::Arbi, 0.0, 0.0).is_err());
        assert!(save_position(&f, 1, 100, OverlayKind::Arbi, 0.5, 0.0).is_ok());
    }
}
