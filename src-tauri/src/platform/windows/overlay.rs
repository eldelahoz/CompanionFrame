use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_MENU,
};
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor, EnumWindows, GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowThreadProcessId, HWND_TOPMOST, IsWindowVisible, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
};
use windows::core::BOOL;

fn hwnd_of(window: &tauri::WebviewWindow) -> tauri::Result<HWND> {
    // Rebuilt from the raw pointer so it does not depend on tauri's `windows` version.
    Ok(HWND(window.hwnd()?.0))
}

fn assert_topmost(hwnd: HWND) {
    // SAFETY: hwnd belongs to a live window of this process.
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// Makes the window a never-activating tool window; call once after creation.
pub fn setup(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let hwnd = hwnd_of(window)?;
    // SAFETY: hwnd belongs to a live window of this process.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
        );
    }
    assert_topmost(hwnd);
    Ok(())
}

/// Toggles click-through via the raw style. tao's `set_ignore_cursor_events` rewrites the
/// styles and hides windows it thinks are hidden (ours are shown with a raw ShowWindow).
pub fn set_click_through(window: &tauri::WebviewWindow, on: bool) -> tauri::Result<()> {
    let hwnd = hwnd_of(window)?;
    // SAFETY: hwnd belongs to a live window of this process.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let want = if on {
            ex | (WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize
        } else {
            // LAYERED must go too: without layered content (WebView2 draws via DirectComposition) Windows hit-tests it as fully transparent.
            ex & !((WS_EX_TRANSPARENT.0 | WS_EX_LAYERED.0) as isize)
        };
        if want != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, want);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
    Ok(())
}

/// Shows/hides without activation; topmost is re-asserted on every show.
pub fn set_visible(window: &tauri::WebviewWindow, visible: bool) -> tauri::Result<()> {
    let hwnd = hwnd_of(window)?;
    // SAFETY: hwnd belongs to a live window of this process.
    unsafe {
        let _ = ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
    if visible {
        assert_topmost(hwnd);
    }
    Ok(())
}

/// Screen rectangle in physical pixels: left, top, width, height.
pub type ScreenRect = (i32, i32, i32, i32);

/// The game's main window and the monitor it sits on.
pub struct GameWindow {
    pub hwnd: isize,
    pub monitor: ScreenRect,
}

pub fn is_topmost(window: &tauri::WebviewWindow) -> bool {
    let Ok(hwnd) = hwnd_of(window) else {
        return false;
    };
    // SAFETY: hwnd belongs to a live window of this process.
    let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    ex as u32 & WS_EX_TOPMOST.0 != 0
}

pub fn raise(window: &tauri::WebviewWindow) {
    if let Ok(hwnd) = hwnd_of(window) {
        assert_topmost(hwnd);
    }
}

pub fn hwnd_value(window: &tauri::WebviewWindow) -> Option<isize> {
    hwnd_of(window).ok().map(|h| h.0 as isize)
}

/// Window handle and process id of the foreground window.
pub fn foreground() -> Option<(isize, u32)> {
    // SAFETY: plain query; a null handle means no foreground window.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is a valid out pointer.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    Some((hwnd.0 as isize, pid))
}

pub fn cursor() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    // SAFETY: `p` is a valid out pointer.
    unsafe { GetCursorPos(&mut p) }.ok().map(|_| (p.x, p.y))
}

/// Takes the foreground for `hwnd`; the system may refuse, which is fine.
/// Foreground lock: the call usually works right after our keyboard hook saw a key; if it was
/// refused, a synthetic ALT tap makes this process the last input receiver and we retry once.
pub fn focus(hwnd: isize) {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: a stale handle just makes the calls fail.
    unsafe {
        if SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd {
            return;
        }
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    dwFlags: flags,
                    ..Default::default()
                },
            },
        };
        SendInput(
            &[key(Default::default()), key(KEYEVENTF_KEYUP)],
            size_of::<INPUT>() as i32,
        );
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Move mode: an activatable window takes the foreground when clicked instead of handing it
/// back to the game. Without the flag it is `WS_EX_NOACTIVATE` again.
pub fn set_activatable(window: &tauri::WebviewWindow, on: bool) {
    let Ok(hwnd) = hwnd_of(window) else { return };
    // SAFETY: hwnd belongs to a live window of this process.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let no = WS_EX_NOACTIVATE.0 as isize;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, if on { ex & !no } else { ex | no });
    }
}

/// Takes the foreground so the game, which hides and clips the cursor while focused, releases the mouse.
pub fn grab(window: &tauri::WebviewWindow) {
    let Ok(hwnd) = hwnd_of(window) else { return };
    // SAFETY: plain call.
    unsafe {
        let _ = ClipCursor(None);
    }
    focus(hwnd.0 as isize);
}

struct Search {
    pids: Vec<u32>,
    best: Option<(i64, HWND)>,
}

unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // A panic must not unwind across the FFI boundary.
    let _ = std::panic::catch_unwind(|| {
        // SAFETY: lparam is the `&mut Search` passed by `game_window`, alive for the whole call.
        let s = unsafe { &mut *(lparam.0 as *mut Search) };
        // SAFETY: plain queries on a handle EnumWindows just handed over.
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut r = RECT::default();
            if !s.pids.contains(&pid) || GetWindowRect(hwnd, &mut r).is_err() {
                return;
            }
            let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
            if s.best.is_none_or(|(a, _)| area > a) {
                s.best = Some((area, hwnd));
            }
        }
    });
    BOOL(1)
}

/// Biggest visible top-level window of the given processes, with its monitor's rectangle.
pub fn game_window(pids: &[u32]) -> Option<GameWindow> {
    if pids.is_empty() {
        return None;
    }
    let mut s = Search {
        pids: pids.to_vec(),
        best: None,
    };
    // SAFETY: `s` outlives the synchronous enumeration.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut s as *mut Search as isize)) };
    let (_, hwnd) = s.best?;
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `mi` has its size set as required.
    let ok = unsafe { GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut mi) };
    let r = mi.rcMonitor;
    ok.as_bool().then_some(GameWindow {
        hwnd: hwnd.0 as isize,
        monitor: (r.left, r.top, r.right - r.left, r.bottom - r.top),
    })
}
