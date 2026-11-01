//! Finds Warframe's window and its client rect in virtual-screen pixels. Win32 unsafe stays here.
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetClientRect, IsIconic};
use windows::core::w;

use super::capture::{MIN_H, MIN_W, Rect};

pub struct GameWindow {
    /// Client area (no title bar or borders) in physical pixels; assumes a DPI-aware process.
    pub client: Rect,
    pub dpi: u32,
}

/// WFHelper's lookup: top-level window titled exactly "Warframe", not minimised.
pub fn find() -> Option<GameWindow> {
    // SAFETY: plain queries on a handle just returned by FindWindowW; the out params are locals.
    unsafe {
        let hwnd: HWND = FindWindowW(None, w!("Warframe")).ok()?;
        if IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc).ok()?;
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w < MIN_W || h < MIN_H {
            return None;
        }
        let mut origin = POINT::default();
        if !ClientToScreen(hwnd, &mut origin).as_bool() {
            return None;
        }
        Some(GameWindow {
            client: Rect {
                x: origin.x,
                y: origin.y,
                w,
                h,
            },
            dpi: GetDpiForWindow(hwnd),
        })
    }
}
