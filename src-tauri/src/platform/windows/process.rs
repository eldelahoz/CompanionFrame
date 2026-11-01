//! The only `unsafe` for game-memory reading: a read-only handle to Warframe.x64.exe behind
//! `ProcessMemoryReader`. Never writes to the process.
use std::ffi::c_void;
use std::sync::{Once, PoisonError, RwLock};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Memory::{MEMORY_BASIC_INFORMATION, VirtualQueryEx};
use windows::Win32::System::ProcessStatus::{
    K32QueryWorkingSetEx, PSAPI_WORKING_SET_EX_INFORMATION,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::services::game_memory::{
    OpenFailure, PAGE_SIZE, ProcessMemoryReader, ReadOutcome, Region,
};

const EXE_NAME: &str = "warframe.x64.exe";

struct WinReader(HANDLE);

impl Drop for WinReader {
    fn drop(&mut self) {
        // SAFETY: the handle came from OpenProcess and is closed exactly once, here.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

impl ProcessMemoryReader for WinReader {
    fn query(&self, address: u64) -> Option<Region> {
        let mut mbi = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: `mbi` is a valid out buffer of the size passed.
        let written = unsafe {
            VirtualQueryEx(
                self.0,
                Some(address as usize as *const c_void),
                &mut mbi,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        (written != 0).then_some(Region {
            base: mbi.BaseAddress as usize as u64,
            size: mbi.RegionSize as u64,
            state: mbi.State.0,
            protect: mbi.Protect.0,
            kind: mbi.Type.0,
        })
    }

    fn read(&self, address: u64, out: &mut [u8]) -> ReadOutcome {
        let mut read = 0usize;
        // SAFETY: `out` is valid for `out.len()` bytes; the kernel reports how many it filled.
        let result = unsafe {
            ReadProcessMemory(
                self.0,
                address as usize as *const c_void,
                out.as_mut_ptr().cast(),
                out.len(),
                Some(&mut read),
            )
        };
        // A partial copy is an error that still hands over its bytes.
        ReadOutcome {
            bytes: read.min(out.len()),
            failed: result.is_err(),
        }
    }

    fn resident_pages(&self, first_page: u64, pages: usize) -> Option<Vec<bool>> {
        let mut entries: Vec<PSAPI_WORKING_SET_EX_INFORMATION> = (0..pages as u64)
            .map(|i| PSAPI_WORKING_SET_EX_INFORMATION {
                VirtualAddress: (first_page + i * PAGE_SIZE) as usize as *mut c_void,
                ..Default::default()
            })
            .collect();
        // SAFETY: `entries` is valid for the byte size passed.
        let ok = unsafe {
            K32QueryWorkingSetEx(
                self.0,
                entries.as_mut_ptr().cast(),
                (entries.len() * size_of::<PSAPI_WORKING_SET_EX_INFORMATION>()) as u32,
            )
        };
        // Bit 0 of the attributes is "Valid", i.e. resident.
        ok.as_bool().then(|| {
            entries
                .iter()
                .map(|e| unsafe { e.VirtualAttributes.Flags } & 1 == 1)
                .collect()
        })
    }
}

/// How often the shared watcher re-reads the process list: the worst-case game start/stop latency.
const WATCH_EVERY: Duration = Duration::from_secs(2);

static PIDS: RwLock<Vec<u32>> = RwLock::new(Vec::new());
static WATCHER: Once = Once::new();

/// Starts the one process watcher (idempotent). The first call fills the list before returning,
/// so call it off the keyboard hook thread.
pub fn start_watch() {
    WATCHER.call_once(|| {
        set_pids(snapshot_pids());
        let spawned = std::thread::Builder::new()
            .name("process-watch".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(WATCH_EVERY);
                    let now = snapshot_pids();
                    if *PIDS.read().unwrap_or_else(PoisonError::into_inner) != now {
                        set_pids(now);
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "process watcher failed to start");
        }
    });
}

fn set_pids(pids: Vec<u32>) {
    *PIDS.write().unwrap_or_else(PoisonError::into_inner) = pids;
}

/// Warframe's pids as of the last watcher pass (at most `WATCH_EVERY` old). Never snapshots.
pub fn warframe_pids() -> Vec<u32> {
    start_watch();
    PIDS.read().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Allocation-free check for `pid`; for hot paths such as the keyboard hook.
pub fn is_warframe_pid(pid: u32) -> bool {
    start_watch();
    PIDS.read()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&pid)
}

pub fn warframe_running() -> bool {
    start_watch();
    !PIDS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty()
}

fn snapshot_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    // SAFETY: the snapshot handle is closed below; the entry struct has its size set as required.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(EXE_NAME) {
                pids.push(entry.th32ProcessID);
            }
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    pids
}

/// True when the window in front belongs to Warframe. Runs inside the keyboard hook, only for
/// keys that match a binding.
pub fn foreground_is_warframe() -> bool {
    let mut pid = 0u32;
    // SAFETY: plain queries; `pid` is a valid out pointer.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    pid != 0 && is_warframe_pid(pid)
}

/// Like [`foreground_is_warframe`], but our own windows count too: in move mode an overlay holds
/// the foreground, and the hotkeys (F7 to leave) must keep working there.
pub fn foreground_is_game_or_own() -> bool {
    let mut pid = 0u32;
    // SAFETY: plain queries; `pid` is a valid out pointer.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    game_or_own(pid, std::process::id(), is_warframe_pid)
}

/// The hook's decision, split out so it can be tested without a window.
fn game_or_own(fg_pid: u32, own_pid: u32, is_game: impl FnOnce(u32) -> bool) -> bool {
    fg_pid != 0 && (fg_pid == own_pid || is_game(fg_pid))
}

/// Seconds since the last keyboard or mouse input; `None` when the query fails.
pub fn idle_seconds() -> Option<f64> {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    // SAFETY: `info` is a valid out pointer with `cbSize` set.
    unsafe { GetLastInputInfo(&mut info) }.as_bool().then(|| {
        // Both counters are 32-bit ms ticks; wrapping_sub survives the 49-day rollover.
        f64::from(unsafe { GetTickCount() }.wrapping_sub(info.dwTime)) / 1000.0
    })
}

/// Opens every running Warframe.x64.exe with query + read access only. Call it only through
/// `game_memory::open_game`, which checks `memory_gate`.
pub(crate) fn open_warframe() -> Result<Vec<Box<dyn ProcessMemoryReader>>, OpenFailure> {
    let pids = warframe_pids();
    if pids.is_empty() {
        return Err(OpenFailure::ProcessNotFound);
    }
    let readers: Vec<Box<dyn ProcessMemoryReader>> = pids
        .into_iter()
        // SAFETY: plain syscall; a returned handle is owned (and closed) by WinReader.
        .filter_map(|pid| unsafe {
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()
        })
        .map(|h| Box::new(WinReader(h)) as Box<dyn ProcessMemoryReader>)
        .collect();
    if readers.is_empty() {
        Err(OpenFailure::AccessDenied)
    } else {
        Ok(readers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_or_own_decision() {
        let game = |p| p == 7;
        assert!(game_or_own(7, 1, game));
        assert!(game_or_own(1, 1, game));
        assert!(!game_or_own(9, 1, game));
        assert!(!game_or_own(0, 0, |_| true));
    }

    /// The hook callback path (foreground query + cached-pid check) must stay far under the
    /// ~300 ms Windows allows; the budget here is 50 us per call.
    #[test]
    fn hook_decision_stays_under_50us() {
        start_watch();
        const N: u32 = 20_000;
        let t = std::time::Instant::now();
        for _ in 0..N {
            std::hint::black_box(foreground_is_game_or_own());
        }
        let per = t.elapsed() / N;
        let t = std::time::Instant::now();
        snapshot_pids();
        eprintln!("old per-key cost (one full snapshot): {:?}", t.elapsed());
        eprintln!("foreground_is_game_or_own: {per:?} per call");
        assert!(per < Duration::from_micros(50), "{per:?}");
    }
}
