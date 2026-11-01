//! Atomic JSON settings: write a temp file, then rename over the real one.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::services::atomic::write_atomic;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct Settings {
    /// The app's interface language.
    pub language: String,
    /// Warframe's client language for item names and OCR: `auto` (EE.log `-language:`) or a code.
    pub game_language: String,
    pub theme: String,
    /// Scans the reward screen (or closes the shown card). Was `overlay_hotkey`.
    #[serde(alias = "overlay_hotkey")]
    pub reward_scan_hotkey: String,
    /// Shows every enabled overlay with sample content to drag into place, and locks them again.
    /// Was `overlay_interaction_hotkey` (a reward-only click toggle); the key the user picked stays.
    #[serde(alias = "overlay_interaction_hotkey")]
    pub overlay_move_hotkey: String,
    pub riven_rescan_hotkey: String,
    /// Manual EE.log path; `None` autodetects.
    pub eelog_path: Option<String>,
    /// Overlay layout editor output (JSON text per key, see `services::overlay::layout`).
    pub overlay_layouts: BTreeMap<String, String>,
    /// Per overlay kind (`reward`, `riven`, `arbi`, `trade`); a missing kind is enabled.
    pub overlay_enabled: BTreeMap<String, bool>,
    /// Per overlay kind, percent 30..=100; a missing kind is fully opaque.
    pub overlay_opacity: BTreeMap<String, u8>,
    /// The appearance editor state as JSON text (`src/types/theme.ts`); empty means the default theme.
    pub theme_settings: String,
    /// Multiplies the display-derived zoom of the main window, 0.75..=1.5 (`services::zoom`).
    pub ui_scale: f64,
    /// Closing the main window hides it to the tray instead of quitting.
    pub keep_running_on_close: bool,
    /// Desktop toast for a new in-game whisper tab.
    pub message_notifications_enabled: bool,
    /// A tab opened while Warframe is focused is probably self-sent; notify anyway when set.
    pub message_notifications_while_focused: bool,
    /// Riven detail lists auction (not just direct-sell) listings among similar rivens.
    pub riven_similar_auctions_shown: bool,
    /// Where inventory data comes from; `memory` also runs warframe-api-helper (`memory_gate`).
    pub data_source_mode: DataSourceMode,
    /// Epoch ms when the user accepted the game-memory disclaimer.
    pub memory_consent_at: Option<f64>,
    /// Fetch the inventory in the background in memory mode.
    pub auto_inventory_sync: bool,
    /// Diagnostics: keep the last scan frames and OCR text in the log folder (`scan_captures`).
    pub save_scan_captures: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DataSourceMode {
    /// EE.log, OCR and imported files only. Nothing reads game memory.
    #[default]
    Passive,
    Memory,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "en".into(),
            game_language: crate::services::game_locale::AUTO.into(),
            theme: "dark".into(),
            reward_scan_hotkey: "F8".into(),
            overlay_move_hotkey: "F7".into(),
            riven_rescan_hotkey: "F6".into(),
            eelog_path: None,
            overlay_layouts: BTreeMap::new(),
            overlay_enabled: BTreeMap::new(),
            overlay_opacity: BTreeMap::new(),
            theme_settings: String::new(),
            ui_scale: 1.0,
            keep_running_on_close: false,
            message_notifications_enabled: true,
            message_notifications_while_focused: false,
            riven_similar_auctions_shown: true,
            data_source_mode: DataSourceMode::Passive,
            memory_consent_at: None,
            auto_inventory_sync: true,
            save_scan_captures: false,
        }
    }
}

pub struct Store {
    path: PathBuf,
    settings: Mutex<Settings>,
}

impl Store {
    /// A missing or corrupt file falls back to defaults.
    pub fn load(path: PathBuf) -> Self {
        let settings = fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            path,
            settings: Mutex::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Reads through the lock without cloning the whole `Settings`; keep `f` short.
    pub fn read<R>(&self, f: impl FnOnce(&Settings) -> R) -> R {
        f(&self.settings.lock().unwrap())
    }

    pub fn set(&self, new: Settings) -> io::Result<()> {
        let mut cur = self.settings.lock().unwrap();
        write_atomic(&self.path, &serde_json::to_vec_pretty(&new)?)?;
        *cur = new;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_file_defaults() {
        let dir = std::env::temp_dir().join(format!("cf-store-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        fs::write(&path, r#"{"theme":"light"}"#).unwrap();
        let store = Store::load(path.clone());
        assert_eq!(store.get().theme, "light");
        assert_eq!(store.get().language, "en");
        assert_eq!(store.get().game_language, "auto");
        assert!(store.get().riven_similar_auctions_shown);
        assert_eq!(store.get().data_source_mode, DataSourceMode::Passive);
        let s = Settings {
            language: "de".into(),
            ..store.get()
        };
        store.set(s.clone()).unwrap();
        assert_eq!(Store::load(path).get(), s);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_hotkey_names_keep_the_users_keys() {
        let s: Settings = serde_json::from_str(
            r#"{"overlay_hotkey":"Ctrl+F9","overlay_interaction_hotkey":"F10","riven_rescan_hotkey":"F11"}"#,
        )
        .unwrap();
        assert_eq!(
            (
                s.reward_scan_hotkey.as_str(),
                s.overlay_move_hotkey.as_str(),
                s.riven_rescan_hotkey.as_str()
            ),
            ("Ctrl+F9", "F10", "F11")
        );
        assert!(!s.save_scan_captures);
        let saved = serde_json::to_string(&s).unwrap();
        assert!(saved.contains("\"overlay_move_hotkey\":\"F10\""));
    }
}
