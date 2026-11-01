//! EE.log infrastructure: locates and tails the game log (and, on Windows, captures its
//! OutputDebugString stream) and broadcasts the lines. Parsers subscribe; nothing here knows
//! what a line means.

pub mod locate;
pub mod tail;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::broadcast;

use crate::store::Store;
use tail::{Out, Tail};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Re-run discovery while no log is attached (the game may be installed or launched later).
const RESOLVE_MISS_INTERVAL: Duration = Duration::from_secs(5);
/// Even with a log attached, in case the player moved launchers.
const RESOLVE_INTERVAL: Duration = Duration::from_secs(60);
const STATUS_BYTES_INTERVAL: Duration = Duration::from_secs(1);
const CHANNEL_CAPACITY: usize = 4096;

/// Where a line came from. The file lags the live debug stream by seconds, so the same line can
/// arrive twice; parsers that care dedupe on this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    File,
    Dbwin,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EeLogEvent {
    Line {
        text: String,
        source: Source,
    },
    /// The log restarted or switched file: drop per-session state.
    Reset,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
pub struct EeLogStatus {
    pub path: Option<String>,
    /// The log file exists and is being tailed.
    pub active: bool,
    /// Bytes of the log consumed since the app started (saturates at 4 GiB).
    pub bytes_read: u32,
    /// The live OutputDebugString capture is attached to a running game.
    pub dbwin_active: bool,
}

/// Managed state. `subscribe()` is the parsers' entry point.
pub struct EeLog {
    tx: broadcast::Sender<EeLogEvent>,
    status: Mutex<EeLogStatus>,
}

impl EeLog {
    pub fn subscribe(&self) -> broadcast::Receiver<EeLogEvent> {
        self.tx.subscribe()
    }

    pub fn status(&self) -> EeLogStatus {
        self.status.lock().unwrap().clone()
    }
}

#[tauri::command]
#[specta::specta]
pub fn eelog_status(eelog: State<EeLog>) -> EeLogStatus {
    eelog.status()
}

/// Path choice plus tail, advanced by `tick`; no threads or Tauri, so tests drive it directly.
struct Core {
    tx: broadcast::Sender<EeLogEvent>,
    tail: Option<Tail>,
    manual: String,
    resolved_at: Option<Instant>,
}

impl Core {
    fn new(tx: broadcast::Sender<EeLogEvent>) -> Self {
        Self {
            tx,
            tail: None,
            manual: String::new(),
            resolved_at: None,
        }
    }

    fn send(tx: &broadcast::Sender<EeLogEvent>, out: Out) {
        // No subscribers is normal until a parser exists.
        let _ = tx.send(match out {
            Out::Line(text) => EeLogEvent::Line {
                text,
                source: Source::File,
            },
            Out::Reset => EeLogEvent::Reset,
        });
    }

    fn tick(&mut self, manual: &str, dbwin_active: bool) -> EeLogStatus {
        let stale = self.resolved_at.is_none_or(|at| {
            at.elapsed()
                >= if self.active() {
                    RESOLVE_INTERVAL
                } else {
                    RESOLVE_MISS_INTERVAL
                }
        });
        if self.manual != manual || stale {
            self.manual = manual.to_string();
            self.resolved_at = Some(Instant::now());
            self.retarget(locate::resolve(manual));
        }
        let mut active = false;
        let mut bytes_read = 0;
        if let Some(tail) = &mut self.tail {
            let tx = &self.tx;
            active = tail.poll(|o| Self::send(tx, o));
            bytes_read = tail.bytes_read;
        }
        EeLogStatus {
            path: self.tail.as_ref().map(|t| t.path().display().to_string()),
            active,
            bytes_read: u32::try_from(bytes_read).unwrap_or(u32::MAX),
            dbwin_active,
        }
    }

    fn active(&self) -> bool {
        self.tail.as_ref().is_some_and(|t| t.path().exists())
    }

    fn retarget(&mut self, path: Option<PathBuf>) {
        if self.tail.as_ref().map(|t| t.path()) == path.as_deref() {
            return;
        }
        if self.tail.is_some() {
            let _ = self.tx.send(EeLogEvent::Reset);
        }
        // An existing log is history; one that shows up later is the game starting.
        self.tail = path.map(|p| {
            let skip = p.exists();
            tracing::info!(path = %p.display(), "ee.log {}", if skip { "found" } else { "not found yet" });
            Tail::new(p, skip)
        });
    }
}

/// Starts the tail thread (and DBWIN on Windows) and manages the `EeLog` state.
pub fn start(app: &AppHandle) {
    let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
    app.manage(EeLog {
        tx: tx.clone(),
        status: Mutex::default(),
    });
    let dbwin = Arc::new(AtomicBool::new(false));
    #[cfg(windows)]
    crate::platform::windows::dbwin::start(tx.clone(), dbwin.clone());
    let app = app.clone();
    // shortcut: the thread lives for the process; add a stop flag if the tail ever needs to pause.
    std::thread::spawn(move || run(app, tx, dbwin));
}

fn run(app: AppHandle, tx: broadcast::Sender<EeLogEvent>, dbwin: Arc<AtomicBool>) {
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let mut core = Core::new(tx);
    let mut watcher: Option<(notify::RecommendedWatcher, PathBuf)> = None;
    let mut last_emit = (EeLogStatus::default(), Instant::now());
    let mut language_read: Option<String> = None;
    let mut manual = String::new();
    loop {
        // Allocates only when the user changes the path.
        app.state::<Store>().read(|s| {
            let now = s.eelog_path.as_deref().unwrap_or_default();
            if now != manual {
                manual = now.to_owned();
            }
        });
        let status = core.tick(&manual, dbwin.load(Ordering::Relaxed));
        // The tail skips a log that already exists, so its first line is read here once.
        if status.path.is_some() && status.path != language_read {
            language_read = status.path.clone();
            if let Some(code) = status.path.as_deref().and_then(head_language) {
                crate::services::item_db::set_log_language(&app, &code);
            }
        }

        // The directory is watched, not the file, so a log created later is noticed too. Wine and
        // NTFS mounts may never send events; the poll interval is the fallback.
        let dir = status
            .path
            .as_ref()
            .and_then(|p| PathBuf::from(p).parent().map(PathBuf::from));
        if watcher.as_ref().map(|(_, d)| d) != dir.as_ref() {
            watcher = dir.and_then(|d| watch_dir(d, wake_tx.clone()));
        }

        let (prev, at) = &last_emit;
        let only_bytes = EeLogStatus {
            bytes_read: prev.bytes_read,
            ..status.clone()
        } == *prev;
        if status != *prev && (!only_bytes || at.elapsed() >= STATUS_BYTES_INTERVAL) {
            *app.state::<EeLog>().status.lock().unwrap() = status.clone();
            let _ = app.emit("eelog-status-updated", &status);
            last_emit = (status, Instant::now());
        }

        let _ = wake_rx.recv_timeout(POLL_INTERVAL);
        while wake_rx.try_recv().is_ok() {}
    }
}

/// `-language:` from the first lines of an EE.log.
fn head_language(path: &str) -> Option<String> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(8 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    String::from_utf8_lossy(&head)
        .lines()
        .find_map(crate::services::eelog_parsers::lines::command_line_language)
}

fn watch_dir(
    dir: PathBuf,
    wake: mpsc::Sender<()>,
) -> Option<(notify::RecommendedWatcher, PathBuf)> {
    use notify::Watcher;
    let mut w = notify::recommended_watcher(move |_| {
        let _ = wake.send(());
    })
    .ok()?;
    w.watch(&dir, notify::RecursiveMode::NonRecursive).ok()?;
    Some((w, dir))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicU32;

    /// A fresh empty directory under the system temp dir.
    pub fn tmp_dir(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "cf-eelog-{}-{}-{tag}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn drain(rx: &mut broadcast::Receiver<EeLogEvent>) -> Vec<EeLogEvent> {
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    fn file_line(s: &str) -> EeLogEvent {
        EeLogEvent::Line {
            text: s.into(),
            source: Source::File,
        }
    }

    #[test]
    fn core_follows_the_manual_path_and_switches_files() {
        let dir = tmp_dir("core");
        let (a, b) = (dir.join("a.log"), dir.join("b.log"));
        fs::write(&a, "history\n").unwrap();
        let (tx, mut rx) = broadcast::channel(16);
        let mut core = Core::new(tx);

        let st = core.tick(a.to_str().unwrap(), false);
        assert!(st.active && st.path.as_deref() == a.to_str());
        assert!(
            drain(&mut rx).is_empty(),
            "existing content is not replayed"
        );

        fs::write(&a, "history\nlive\n").unwrap();
        let st = core.tick(a.to_str().unwrap(), true);
        assert_eq!((st.bytes_read, st.dbwin_active), (13 - 8, true));
        assert_eq!(drain(&mut rx), [file_line("live")]);

        // Pointing at a log that does not exist yet: inactive, then read from the top.
        let st = core.tick(b.to_str().unwrap(), false);
        assert!(!st.active);
        assert_eq!(drain(&mut rx), [EeLogEvent::Reset]);
        fs::write(&b, "boot\n").unwrap();
        assert!(core.tick(b.to_str().unwrap(), false).active);
        assert_eq!(drain(&mut rx), [file_line("boot")]);
    }

    #[test]
    fn status_serializes_for_the_frontend() {
        let json = serde_json::to_value(EeLogStatus::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"path": null, "active": false, "bytes_read": 0, "dbwin_active": false})
        );
    }
}
