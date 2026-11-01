//! First-run state that must survive a WebView storage wipe: whether the setup wizard was finished and
//! whether the feature tour was seen. Replaces the old `localStorage` flag; a missing or corrupt file means
//! "not done", so the wizard simply shows again.
use crate::services::sync::LockExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager as _};

use crate::services::atomic::write_atomic;

const FILE: &str = "onboarding.json";

static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct OnboardingState {
    pub setup_done: bool,
    pub tour_seen: bool,
}

fn load(path: &Path) -> OnboardingState {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn update(path: &Path, f: impl FnOnce(&mut OnboardingState)) -> Result<OnboardingState, String> {
    let _g = LOCK.lock_any();
    let mut s = load(path);
    f(&mut s);
    let bytes = serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?;
    write_atomic(path, &bytes).map_err(|e| e.to_string())?;
    Ok(s)
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(FILE))
}

#[tauri::command]
#[specta::specta]
pub fn onboarding_get(app: AppHandle) -> Result<OnboardingState, String> {
    let _g = LOCK.lock_any();
    Ok(load(&path(&app)?))
}

#[tauri::command]
#[specta::specta]
pub fn onboarding_set_setup_done(app: AppHandle, done: bool) -> Result<OnboardingState, String> {
    update(&path(&app)?, |s| s.setup_done = done)
}

/// `false` restarts the tour ("Show feature tour" in Settings).
#[tauri::command]
#[specta::specta]
pub fn onboarding_set_tour_seen(app: AppHandle, seen: bool) -> Result<OnboardingState, String> {
    update(&path(&app)?, |s| s.tour_seen = seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_persist_independently_and_corrupt_file_resets() {
        let dir = std::env::temp_dir().join(format!("cf-onboard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join(FILE);
        assert_eq!(load(&f), OnboardingState::default());
        update(&f, |s| s.setup_done = true).unwrap();
        let s = update(&f, |s| s.tour_seen = true).unwrap();
        assert_eq!(load(&f), s);
        assert!(s.setup_done && s.tour_seen);
        update(&f, |s| s.tour_seen = false).unwrap();
        assert_eq!(
            load(&f),
            OnboardingState {
                setup_done: true,
                tour_seen: false
            }
        );
        std::fs::write(&f, b"{").unwrap();
        assert_eq!(load(&f), OnboardingState::default());
    }
}
