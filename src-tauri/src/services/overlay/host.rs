//! Tauri + Win32 [`Host`]: the four webview windows, built hidden once and reused.

use tauri::{
    AppHandle, Emitter, Manager as _, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use super::manager::{Foreground, Game, Host, OverlayKind, OverlayRect};
use crate::platform::windows::{overlay, process};
use crate::store::Store;

/// Transparent windows are unmapped on hide, as the prototype did and its manual test passed.
/// Flip to `true` only if a re-show turns out black (see docs/manual-tests/f6-t1-overlay-manager.md).
const KEEP_MAPPED: bool = false;

pub struct TauriHost {
    app: AppHandle,
    wins: Vec<WebviewWindow>,
    game_hwnd: Option<isize>,
}

impl TauriHost {
    pub fn build(app: &AppHandle) -> tauri::Result<Self> {
        let mut wins = Vec::new();
        for kind in OverlayKind::ALL {
            let url = format!("overlay.html?kind={}", kind.name());
            let win = WebviewWindowBuilder::new(
                app,
                format!("overlay-{}", kind.name()),
                WebviewUrl::App(url.into()),
            )
            .title(format!("CompanionFrame overlay ({})", kind.name()))
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focused(false)
            .visible(false)
            .build()?;
            overlay::setup(&win)?;
            overlay::set_click_through(&win, true)?;
            wins.push(win);
        }
        Ok(Self {
            app: app.clone(),
            wins,
            game_hwnd: None,
        })
    }

    fn win(&self, kind: OverlayKind) -> &WebviewWindow {
        &self.wins[kind as usize]
    }
}

fn rect((x, y, w, h): overlay::ScreenRect) -> OverlayRect {
    OverlayRect { x, y, w, h }
}

impl Host for TauriHost {
    fn show(&mut self, kind: OverlayKind) {
        let _ = overlay::set_visible(self.win(kind), true);
    }

    fn hide(&mut self, kind: OverlayKind) {
        let _ = overlay::set_visible(self.win(kind), false);
    }

    fn set_click_through(&mut self, kind: OverlayKind, on: bool) {
        let _ = overlay::set_click_through(self.win(kind), on);
    }

    fn place(&mut self, kind: OverlayKind, r: OverlayRect) {
        let w = self.win(kind);
        let _ = w.set_position(PhysicalPosition::new(r.x, r.y));
        let _ = w.set_size(PhysicalSize::new(r.w.max(1) as u32, r.h.max(1) as u32));
    }

    fn is_topmost(&self, kind: OverlayKind) -> bool {
        overlay::is_topmost(self.win(kind))
    }

    fn raise(&mut self, kind: OverlayKind) {
        overlay::raise(self.win(kind));
    }

    fn set_content_visible(&mut self, kind: OverlayKind, visible: bool) {
        let _ = self.win(kind).emit("overlay-content-visible", visible);
    }

    fn game(&mut self) -> Option<Game> {
        let pids = process::warframe_pids();
        let g = overlay::game_window(&pids)?;
        self.game_hwnd = Some(g.hwnd);
        let foreground = match overlay::foreground() {
            Some((hwnd, _)) if hwnd == g.hwnd => Foreground::Game,
            Some((hwnd, pid)) if pid == std::process::id() => {
                if self
                    .wins
                    .iter()
                    .any(|w| overlay::hwnd_value(w) == Some(hwnd))
                {
                    Foreground::Overlay
                } else {
                    Foreground::Own
                }
            }
            Some((_, pid)) if pids.contains(&pid) => Foreground::Game,
            _ => Foreground::Other,
        };
        Some(Game {
            monitor: rect(g.monitor),
            foreground,
        })
    }

    fn primary_monitor(&self) -> Option<OverlayRect> {
        let m = self.app.primary_monitor().ok()??;
        let (p, s) = (m.position(), m.size());
        Some(OverlayRect {
            x: p.x,
            y: p.y,
            w: s.width as i32,
            h: s.height as i32,
        })
    }

    fn cursor(&self) -> Option<(i32, i32)> {
        overlay::cursor()
    }

    fn set_activatable(&mut self, kind: OverlayKind, on: bool) {
        overlay::set_activatable(self.win(kind), on);
    }

    fn take_focus_for_move(&mut self, kind: OverlayKind) {
        overlay::grab(self.win(kind));
    }

    fn restore_game_focus(&mut self) {
        if let Some(hwnd) = self.game_hwnd {
            overlay::focus(hwnd);
        }
    }

    fn keep_mapped(&self) -> bool {
        KEEP_MAPPED
    }

    fn enabled(&self, kind: OverlayKind) -> bool {
        self.app
            .try_state::<Store>()
            .is_none_or(|s| super::settings::enabled(&s.get(), kind))
    }
}
