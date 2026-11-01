//! `WH_KEYBOARD_LL` hook on its own thread. All decisions live in `services::hotkeys::matcher`;
//! this file only feeds it key events and obeys the answer.
use std::panic::catch_unwind;
use std::sync::mpsc::Sender;
use std::sync::{LazyLock, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;

use tokio::sync::oneshot;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use super::process::foreground_is_game_or_own;
use crate::services::hotkeys::accelerator::{Accel, Mods};
use crate::services::hotkeys::matcher::{Action, Matcher};

pub enum HookEvent {
    Installed,
    Failed(String),
    Fired(String),
}

static MATCHER: LazyLock<Mutex<Matcher>> = LazyLock::new(Mutex::default);
static TX: OnceLock<Sender<HookEvent>> = OnceLock::new();

fn matcher() -> MutexGuard<'static, Matcher> {
    MATCHER.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn set_watch(watch: Vec<(String, Accel)>) {
    matcher().set_watch(watch);
}

pub fn arm_capture(tx: oneshot::Sender<Option<Accel>>) {
    matcher().arm_capture(tx);
}

pub fn cancel_capture() {
    matcher().cancel_capture();
}

/// Dev/test override: fire in any foreground app (never swallows).
fn any_window() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("COMPANIONFRAME_HOTKEY_ANY_WINDOW").is_some())
}

fn down(vk: u16) -> bool {
    // SAFETY: plain query, no pointers.
    (unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000) != 0
}

fn mods_now() -> Mods {
    Mods {
        ctrl: down(VK_CONTROL.0),
        alt: down(VK_MENU.0),
        shift: down(VK_SHIFT.0),
        win: down(VK_LWIN.0) || down(VK_RWIN.0),
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // A panic must not unwind into Windows.
    let swallow = catch_unwind(|| {
        if code != HC_ACTION as i32 {
            return false;
        }
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT.
        let vk = unsafe { (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode };
        let is_down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        match matcher().on_key(
            vk,
            is_down,
            mods_now,
            foreground_is_game_or_own,
            any_window(),
        ) {
            Action::Pass => false,
            Action::Swallow => true,
            Action::Fire { id, swallow } => {
                if let Some(tx) = TX.get() {
                    let _ = tx.send(HookEvent::Fired(id));
                }
                swallow
            }
        }
    })
    .unwrap_or(false);
    if swallow {
        return LRESULT(1);
    }
    // SAFETY: forwarding the unchanged arguments.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Installs the hook on a dedicated thread with its own message loop and reports the outcome
/// (`Installed` / `Failed`) and every match (`Fired`) on `tx`. Call once.
pub fn start(tx: Sender<HookEvent>) {
    let _ = TX.set(tx.clone());
    // Fill the pid list here so the hook never pays for the first snapshot.
    super::process::start_watch();
    thread::spawn(move || {
        // SAFETY: global LL hook; hook_proc has the HOOKPROC signature.
        let hook = unsafe {
            let hmod = GetModuleHandleW(None).ok();
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), hmod.map(Into::into), 0)
        };
        if let Err(e) = hook {
            let _ = tx.send(HookEvent::Failed(e.to_string()));
            return;
        }
        let _ = tx.send(HookEvent::Installed);
        let mut msg = MSG::default();
        // SAFETY: standard message loop; LL hooks are dispatched from GetMessage.
        // `> 0` also stops on the -1 error return instead of spinning.
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {}
        let _ = tx.send(HookEvent::Failed("keyboard hook message loop ended".into()));
    });
}
