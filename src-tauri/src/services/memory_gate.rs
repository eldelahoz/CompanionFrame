//! The single switch for everything that reads game memory (warframe-api-helper, `game_memory`,
//! mission rewards). Off unless the user chose the "memory" data source AND ticked the consent.
//! Mirrored into an atomic so deep callers (the process opener, trackers) can check it cheaply.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::store::{DataSourceMode, Settings};

static ENABLED: AtomicBool = AtomicBool::new(false);

pub const OFF_ERROR: &str =
    "game-memory mode is off: choose it in Settings and accept the disclaimer first";

pub fn enabled_by(s: &Settings) -> bool {
    s.data_source_mode == DataSourceMode::Memory && s.memory_consent_at.is_some()
}

/// Called at startup and on every settings save.
pub fn apply(s: &Settings) {
    ENABLED.store(enabled_by(s), Ordering::SeqCst);
}

pub fn allowed() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

pub fn require() -> Result<(), String> {
    if allowed() {
        Ok(())
    } else {
        Err(OFF_ERROR.into())
    }
}

/// Settings may only ask for memory mode with a consent timestamp.
pub fn validate(s: &Settings) -> Result<(), String> {
    if s.data_source_mode == DataSourceMode::Memory && s.memory_consent_at.is_none() {
        return Err("memory mode needs the disclaimer accepted".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_without_consent_or_in_passive_mode() {
        let passive = Settings::default();
        assert_eq!(passive.data_source_mode, DataSourceMode::Passive);
        assert!(!enabled_by(&passive));

        let no_consent = Settings {
            data_source_mode: DataSourceMode::Memory,
            ..Settings::default()
        };
        assert!(!enabled_by(&no_consent));
        assert!(validate(&no_consent).is_err());

        let consent_but_passive = Settings {
            memory_consent_at: Some(1.0),
            ..Settings::default()
        };
        assert!(!enabled_by(&consent_but_passive));

        let on = Settings {
            data_source_mode: DataSourceMode::Memory,
            memory_consent_at: Some(1.0),
            ..Settings::default()
        };
        assert!(enabled_by(&on) && validate(&on).is_ok());

        apply(&no_consent);
        assert!(!allowed() && require().is_err());
        assert_eq!(
            crate::services::game_memory::read_game_authz().status,
            crate::services::game_memory::GameAuthzStatus::Unavailable
        );
    }
}
