//! App scale of the main window (port of WFHelper's `mainWindowZoom` + `config/runtime/uiScale`).
//! The base zoom comes from the display, the user's slider multiplies it.

use tauri::{AppHandle, Manager};

pub const UI_SCALE_MIN: f64 = 0.75;
pub const UI_SCALE_MAX: f64 = 1.5;

/// px sizes are tuned around 1080p; the shorter edge is orientation independent.
fn base_zoom(width: f64, height: f64) -> f64 {
    let edges = [width, height]
        .into_iter()
        .filter(|e| e.is_finite() && *e > 0.0);
    match edges.reduce(f64::min) {
        None => 1.0,
        Some(s) if s <= 720.0 => 0.8,
        Some(s) if s <= 900.0 => 0.9,
        Some(s) if s <= 1200.0 => 1.0,
        Some(s) if s <= 1600.0 => 1.15,
        Some(_) => 1.3,
    }
}

/// Display base zoom times the user's scale (clamped, non-finite means 1), rounded to 3 decimals.
pub fn zoom_factor(width: f64, height: f64, user_scale: f64) -> f64 {
    let scale = if user_scale.is_finite() {
        user_scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    } else {
        1.0
    };
    (base_zoom(width, height) * scale * 1000.0).round() / 1000.0
}

#[tauri::command]
#[specta::specta]
pub fn set_zoom(app: AppHandle, scale: f64) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no main window")?;
    // Work area in device-independent px, so it composes with Windows DPI scaling.
    let (w, h) = match win.current_monitor().map_err(|e| e.to_string())? {
        Some(m) => {
            let a = m.work_area();
            let f = m.scale_factor();
            (f64::from(a.size.width) / f, f64::from(a.size.height) / f)
        }
        None => (0.0, 0.0),
    };
    win.set_zoom(zoom_factor(w, h, scale))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_zoom_follows_the_shorter_edge() {
        assert_eq!(zoom_factor(1920.0, 1040.0, 1.0), 1.0);
        assert_eq!(zoom_factor(1040.0, 1920.0, 1.0), 1.0);
        assert_eq!(zoom_factor(1280.0, 720.0, 1.0), 0.8);
        assert_eq!(zoom_factor(1600.0, 900.0, 1.0), 0.9);
        assert_eq!(zoom_factor(2560.0, 1400.0, 1.0), 1.15);
        assert_eq!(zoom_factor(3840.0, 2100.0, 1.0), 1.3);
        assert_eq!(zoom_factor(0.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn user_scale_is_clamped_and_multiplies() {
        assert_eq!(zoom_factor(1920.0, 1080.0, 9.0), 1.5);
        assert_eq!(zoom_factor(1920.0, 1080.0, 0.1), 0.75);
        assert_eq!(zoom_factor(1920.0, 1080.0, f64::NAN), 1.0);
        assert_eq!(zoom_factor(3840.0, 2160.0, 1.5), 1.95);
    }
}
