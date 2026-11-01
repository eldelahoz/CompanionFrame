//! Main-window shell: custom titlebar commands, position/size persistence, keep-running-on-close.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, Window, WindowEvent,
};

pub const MIN_SIZE: (u32, u32) = (900, 600);
const SAVE_EVERY: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct State {
    #[serde(flatten)]
    rect: Rect,
    maximized: bool,
}

fn overlap(a: Rect, b: Rect) -> i64 {
    let w = (a.x as i64 + a.width as i64).min(b.x as i64 + b.width as i64) - a.x.max(b.x) as i64;
    let h = (a.y as i64 + a.height as i64).min(b.y as i64 + b.height as i64) - a.y.max(b.y) as i64;
    if w > 0 && h > 0 { w * h } else { 0 }
}

/// Clamp onto the work area the bounds overlap most; `None` when no display holds any of them,
/// so a detached monitor cannot strand the window offscreen.
pub fn fit_bounds(bounds: Rect, areas: &[Rect], min: (u32, u32)) -> Option<Rect> {
    let best = areas
        .iter()
        .map(|a| (overlap(bounds, *a), *a))
        .filter(|(o, _)| *o > 0)
        .max_by_key(|(o, _)| *o)?
        .1;
    let width = bounds.width.min(best.width).max(min.0);
    let height = bounds.height.min(best.height).max(min.1);
    let place = |pos: i32, start: i32, size: u32, span: u32| {
        if size > span {
            start
        } else {
            pos.clamp(start, start + (span - size) as i32)
        }
    };
    Some(Rect {
        x: place(bounds.x, best.x, width, best.width),
        y: place(bounds.y, best.y, height, best.height),
        width,
        height,
    })
}

pub struct WindowState {
    path: PathBuf,
    last_save: Mutex<Option<Instant>>,
}

impl WindowState {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("main-window-state.json"),
            last_save: Mutex::new(None),
        }
    }

    fn read(&self) -> Option<State> {
        serde_json::from_slice(&std::fs::read(&self.path).ok()?).ok()
    }

    /// `force` skips the throttle (close); moves and resizes fire in bursts.
    fn save(&self, w: &Window, force: bool) {
        let mut last = self.last_save.lock().unwrap();
        if !force && last.is_some_and(|t| t.elapsed() < SAVE_EVERY) {
            return;
        }
        let Some(state) = self.snapshot(w) else {
            return;
        };
        *last = Some(Instant::now());
        if let Ok(bytes) = serde_json::to_vec(&state)
            && let Err(e) = crate::services::atomic::write_atomic(&self.path, &bytes)
        {
            tracing::warn!(error = %e, "window state save failed");
        }
    }

    fn snapshot(&self, w: &Window) -> Option<State> {
        let maximized = w.is_maximized().ok()?;
        // Maximised or minimised keeps the last normal bounds as the restore position.
        if maximized || w.is_minimized().ok()? {
            return Some(State {
                maximized,
                ..self.read()?
            });
        }
        let (p, s) = (w.outer_position().ok()?, w.inner_size().ok()?);
        Some(State {
            rect: Rect {
                x: p.x,
                y: p.y,
                width: s.width,
                height: s.height,
            },
            maximized,
        })
    }
}

/// Reads one small JSON file; cheap enough for `setup`.
pub fn restore<R: Runtime>(app: &AppHandle<R>) {
    let Some(w) = app.get_webview_window("main") else {
        return;
    };
    let _ = w.set_min_size(Some(PhysicalSize::new(MIN_SIZE.0, MIN_SIZE.1)));
    let Some(state) = app.state::<WindowState>().read() else {
        return;
    };
    let areas: Vec<Rect> = w
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Rect {
                x: a.position.x,
                y: a.position.y,
                width: a.size.width,
                height: a.size.height,
            }
        })
        .collect();
    if let Some(r) = fit_bounds(state.rect, &areas, MIN_SIZE) {
        let _ = w.set_size(PhysicalSize::new(r.width, r.height));
        let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
        if state.maximized {
            let _ = w.maximize();
        }
    }
}

/// Main-window events: persist bounds; hide instead of closing when the user asked for it.
pub fn on_event(window: &Window, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    let app = window.app_handle();
    let Some(ws) = app.try_state::<WindowState>() else {
        return;
    };
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => ws.save(window, false),
        WindowEvent::CloseRequested { api, .. } => {
            ws.save(window, true);
            let keep = app
                .try_state::<crate::store::Store>()
                .is_some_and(|s| s.get().keep_running_on_close);
            if keep {
                api.prevent_close();
                let _ = window.hide();
            } else {
                // Hidden overlay windows would keep the process (and tray icon) alive.
                app.exit(0);
            }
        }
        _ => {}
    }
}

#[tauri::command]
#[specta::specta]
pub fn window_minimize(w: WebviewWindow) {
    let _ = w.minimize();
}

#[tauri::command]
#[specta::specta]
pub fn window_toggle_maximize(w: WebviewWindow) {
    if w.is_maximized().unwrap_or(false) {
        let _ = w.unmaximize();
    } else {
        let _ = w.maximize();
    }
}

#[tauri::command]
#[specta::specta]
pub fn window_close(w: WebviewWindow) {
    let _ = w.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
    const PRIMARY: Rect = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1040,
    };
    const SECOND: Rect = Rect {
        x: 1920,
        y: 0,
        width: 1280,
        height: 720,
    };
    const MIN: (u32, u32) = (900, 600);

    #[test]
    fn keeps_bounds_that_fit_and_the_display_they_are_on() {
        let b = r(100, 80, 1280, 820);
        assert_eq!(fit_bounds(b, &[PRIMARY, SECOND], MIN), Some(b));
        let c = r(2000, 40, 1000, 640);
        assert_eq!(fit_bounds(c, &[PRIMARY, SECOND], MIN), Some(c));
    }

    #[test]
    fn pulls_a_mostly_offscreen_window_back() {
        assert_eq!(
            fit_bounds(r(1850, 0, 1280, 820), &[PRIMARY], MIN),
            Some(r(640, 0, 1280, 820))
        );
    }

    #[test]
    fn shrinks_oversized_and_enforces_minimum() {
        assert_eq!(
            fit_bounds(r(1920, 0, 2560, 1440), &[SECOND], MIN),
            Some(SECOND)
        );
        assert_eq!(
            fit_bounds(r(1920, 0, 200, 100), &[SECOND], MIN),
            Some(r(1920, 0, 900, 600))
        );
        let compact = r(0, 0, 800, 500);
        assert_eq!(
            fit_bounds(r(100, 80, 1280, 820), &[compact], MIN),
            Some(r(0, 0, 900, 600))
        );
    }

    #[test]
    fn none_when_the_display_is_gone_and_prefers_the_larger_overlap() {
        assert_eq!(fit_bounds(r(4000, 0, 1280, 820), &[PRIMARY], MIN), None);
        assert_eq!(fit_bounds(r(0, 0, 1280, 820), &[], MIN), None);
        assert_eq!(
            fit_bounds(r(1700, 0, 1000, 640), &[PRIMARY, SECOND], MIN),
            Some(r(1920, 0, 1000, 640))
        );
    }
}
