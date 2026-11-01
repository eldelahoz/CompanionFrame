//! Captures the game's OutputDebugString stream through the DBWIN shared objects. A reader must
//! create them before a writer can emit, so they are only created while Warframe runs; the game
//! blocks on every line until it is acked, hence the high-priority thread and the early ack.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READWRITE,
    UnmapViewOfFile,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, SetEvent, SetThreadPriority, THREAD_PRIORITY_HIGHEST,
    WaitForSingleObject,
};
use windows::core::w;

use super::process::{is_warframe_pid, warframe_running};
use crate::services::eelog::{EeLogEvent, Source};

const BUFFER_SIZE: usize = 4096;
const WAIT_MS: u32 = 500;
const IDLE_POLL: Duration = Duration::from_secs(2);
const GAME_RECHECK: Duration = Duration::from_secs(2);

/// DBWIN names are session-global, so an e2e instance would read the player's real game.
fn disabled() -> bool {
    std::env::var("COMPANIONFRAME_DISABLE_DBWIN").is_ok_and(|v| v == "1")
}

/// Starts the capture thread; `active` is true while attached to a running game.
pub fn start(tx: broadcast::Sender<EeLogEvent>, active: Arc<AtomicBool>) {
    if disabled() {
        tracing::info!("dbwin disabled by COMPANIONFRAME_DISABLE_DBWIN");
        return;
    }
    // shortcut: the thread lives for the process; add a stop flag if the capture ever needs to pause.
    let spawned = std::thread::Builder::new()
        .name("eelog-dbwin".into())
        .spawn(move || run(&tx, &active));
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "dbwin thread failed to start");
    }
}

fn run(tx: &broadcast::Sender<EeLogEvent>, active: &AtomicBool) {
    // SAFETY: plain syscall on the current thread's pseudo handle.
    if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) }.is_err() {
        tracing::warn!("dbwin: SetThreadPriority failed");
    }
    loop {
        if !warframe_running() {
            std::thread::sleep(IDLE_POLL);
            continue;
        }
        match Session::open() {
            Ok(session) => {
                active.store(true, Ordering::Relaxed);
                session.pump(tx);
                active.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                tracing::warn!(error = %e, "dbwin: could not create the shared objects");
                std::thread::sleep(IDLE_POLL);
            }
        }
    }
}

/// The DBWIN_BUFFER mapping and its two events, closed on drop.
struct Session {
    map: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    ready: HANDLE,
    data: HANDLE,
}

impl Session {
    fn open() -> windows::core::Result<Self> {
        // SAFETY: every handle is owned by the returned Session, or closed on the error paths.
        unsafe {
            let map = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                BUFFER_SIZE as u32,
                w!("DBWIN_BUFFER"),
            )?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                tracing::info!("dbwin: another reader already owns DBWIN_BUFFER");
            }
            let view = MapViewOfFile(map, FILE_MAP_READ, 0, 0, 0);
            if view.Value.is_null() {
                let err = windows::core::Error::from_thread();
                let _ = CloseHandle(map);
                return Err(err);
            }
            // "ready to receive" starts signalled; "data ready" starts clear. Both auto-reset.
            let events = CreateEventW(None, false, true, w!("DBWIN_BUFFER_READY")).and_then(|r| {
                CreateEventW(None, false, false, w!("DBWIN_DATA_READY")).map(|d| (r, d))
            });
            match events {
                Ok((ready, data)) => Ok(Self {
                    map,
                    view,
                    ready,
                    data,
                }),
                Err(e) => {
                    let _ = UnmapViewOfFile(view);
                    let _ = CloseHandle(map);
                    Err(e)
                }
            }
        }
    }

    /// Forwards the game's lines until it exits.
    fn pump(&self, tx: &broadcast::Sender<EeLogEvent>) {
        let mut recheck = Instant::now() + GAME_RECHECK;
        let mut buf = [0u8; BUFFER_SIZE];
        loop {
            // SAFETY: the handles live as long as `self`; the view maps BUFFER_SIZE readable bytes.
            let signalled = unsafe { WaitForSingleObject(self.data, WAIT_MS) } == WAIT_OBJECT_0;
            if signalled {
                // The writer blocks until BUFFER_READY: one copy out of the view, then ack.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.view.Value.cast::<u8>(),
                        buf.as_mut_ptr(),
                        BUFFER_SIZE,
                    );
                    let _ = SetEvent(self.ready);
                }
            } else {
                // A ready signal lost mid-handshake would otherwise cost every later line the
                // writer's ten second timeout.
                unsafe {
                    let _ = SetEvent(self.ready);
                }
            }
            if Instant::now() > recheck {
                recheck = Instant::now() + GAME_RECHECK;
                if !warframe_running() {
                    return;
                }
            }
            if !signalled {
                continue;
            }
            if let Some((pid, text)) = decode(&buf)
                && is_warframe_pid(pid)
            {
                let _ = tx.send(EeLogEvent::Line {
                    text,
                    source: Source::Dbwin,
                });
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: each handle was created in `open` and is closed exactly once, here. A writer
        // already blocked in OutputDebugString needs BUFFER_READY signalled once more.
        unsafe {
            let _ = SetEvent(self.ready);
            let _ = UnmapViewOfFile(self.view);
            for h in [self.map, self.ready, self.data] {
                let _ = CloseHandle(h);
            }
        }
    }
}

/// The DBWIN record: a little-endian pid, then a NUL-terminated utf8 message.
fn decode(buf: &[u8]) -> Option<(u32, String)> {
    let pid = u32::from_le_bytes(buf.get(..4)?.try_into().ok()?);
    let body = &buf[4..];
    let end = body.iter().position(|&b| b == 0).unwrap_or(body.len());
    let text = String::from_utf8_lossy(&body[..end]);
    let text = text.trim_end_matches(['\r', '\n']);
    (!text.is_empty()).then(|| (pid, text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pid: u32, msg: &[u8]) -> Vec<u8> {
        let mut b = pid.to_le_bytes().to_vec();
        b.extend_from_slice(msg);
        b.resize(BUFFER_SIZE, 0);
        b
    }

    #[test]
    fn decodes_pid_and_utf8_message() {
        let buf = record(4242, "Sys [Info]: caf\u{e9} \u{e000}\n".as_bytes());
        assert_eq!(
            decode(&buf),
            Some((4242, "Sys [Info]: caf\u{e9} \u{e000}".into()))
        );
    }

    #[test]
    fn empty_message_is_dropped() {
        assert_eq!(decode(&record(1, b"")), None);
        assert_eq!(decode(&record(1, b"\r\n")), None);
    }
}
