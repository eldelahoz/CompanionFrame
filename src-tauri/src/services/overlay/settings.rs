//! Per-overlay switches and opacity kept in the settings `Store`; validated before they are saved.

use super::OverlayKind;
use crate::store::Settings;

/// Same bounds as `config/shared/overlayOpacity`.
const OPACITY_PERCENT: std::ops::RangeInclusive<u8> = 30..=100;

/// Event the overlay windows listen to: they re-read the settings (theme, opacity).
pub const CHANGED_EVENT: &str = "overlay-settings";

pub fn enabled(s: &Settings, kind: OverlayKind) -> bool {
    s.overlay_enabled.get(kind.name()).copied().unwrap_or(true)
}

pub fn validate(s: &Settings) -> Result<(), String> {
    let known = |k: &String| OverlayKind::ALL.iter().any(|o| o.name() == k);
    if let Some(k) = s.overlay_enabled.keys().find(|k| !known(k)) {
        return Err(format!("unknown overlay {k}"));
    }
    if let Some(k) = s.overlay_opacity.keys().find(|k| !known(k)) {
        return Err(format!("unknown overlay {k}"));
    }
    match s
        .overlay_opacity
        .iter()
        .find(|(_, p)| !OPACITY_PERCENT.contains(p))
    {
        Some((k, p)) => Err(format!("opacity of {k} must be 30-100, got {p}")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_kind_is_enabled_and_bad_values_are_rejected() {
        let mut s = Settings::default();
        assert!(enabled(&s, OverlayKind::Arbi));
        s.overlay_enabled.insert("arbi".into(), false);
        assert!(!enabled(&s, OverlayKind::Arbi));
        assert!(validate(&s).is_ok());

        s.overlay_opacity.insert("reward".into(), 29);
        assert!(validate(&s).is_err());
        s.overlay_opacity.insert("reward".into(), 30);
        assert!(validate(&s).is_ok());
        s.overlay_opacity.insert("reward".into(), 101);
        assert!(validate(&s).is_err());

        let mut s = Settings::default();
        s.overlay_enabled.insert("nope".into(), true);
        assert!(validate(&s).is_err());
    }
}
