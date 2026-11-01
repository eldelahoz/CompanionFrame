//! Inventory core. The inventory is an imported snapshot (an inventory JSON file or AlecaFrame's
//! cache, or in memory mode warframe-api-helper's fetch) plus the deltas applied since: exact
//! EE.log trades and estimated relic picks. Nothing here reads the game's process memory. Snapshot and deltas live in SQLite (`store`), so they
//! survive restarts; the AlecaFrame file is watched and re-imported when it changes.
//!
//! shortcut: the watcher polls (mtime+size, two equal polls = write finished) instead of using
//! `notify`; Wine/NTFS files don't emit reliable events anyway. Upgrade only if 500 ms is too slow.

pub mod aleca;
pub mod commands;
pub mod delta;
pub mod payload;
pub mod store;
pub mod sync_marker;

#[cfg(test)]
mod tests;

use crate::services::api_helper::download::sha256_hex;
use crate::services::clock::now_ms_f64;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use crate::db::Db;
use delta::{Delta, DeltaKind};
use payload::{has_inventory_shape, indent_json_text, unwrap_inventory_payload};

const MAX_INVENTORY_BYTES: u64 = 50 * 1024 * 1024;

/// Where the snapshot came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum InventorySource {
    #[default]
    None,
    /// An inventory JSON file picked or dropped by the user.
    Manual,
    Aleca,
    /// warframe-api-helper's fetch (memory mode only).
    Helper,
}

impl InventorySource {
    fn parse(raw: &str) -> Self {
        match raw {
            "manual" => Self::Manual,
            "aleca" => Self::Aleca,
            "helper" => Self::Helper,
            _ => Self::None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Manual => "manual",
            Self::Aleca => "aleca",
            Self::Helper => "helper",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ErrorKind {
    Parse,
    Read,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReadError {
    pub kind: ErrorKind,
    pub message: String,
    pub path: String,
    /// Epoch milliseconds.
    pub at: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InventoryStatus {
    pub path: Option<String>,
    pub found: bool,
    pub source: InventorySource,
    /// Epoch milliseconds of the file the snapshot came from.
    pub modified_at: Option<f64>,
    pub last_error: Option<ReadError>,
    /// Epoch milliseconds when the snapshot was imported.
    pub imported_at: Option<f64>,
    /// Distinct EE.log trades applied since the import.
    pub trades_applied: u32,
    /// Relic rewards applied since the import; these are estimates.
    pub picks_applied: u32,
    /// EE.log showed the game syncing its inventory after the import: re-import for exact counts.
    pub game_synced_since_import: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ExportError {
    NoInventory,
    WriteFailed,
}

/// Receives what the UI must hear about; the tauri impl emits events.
pub trait Sink: Send + Sync {
    fn updated(&self, data: Option<&Value>);
    fn status(&self, status: &InventoryStatus);
}

fn mtime_ms(t: SystemTime) -> f64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

type ReadFail = (ErrorKind, String);

fn io_fail(e: io::Error) -> ReadFail {
    (ErrorKind::Read, e.to_string())
}

fn read_checked(path: &Path, what: &str) -> Result<(Vec<u8>, f64), ReadFail> {
    let meta = fs::metadata(path).map_err(io_fail)?;
    if !meta.is_file() {
        return Err((ErrorKind::Read, format!("{what} path is not a file")));
    }
    if meta.len() > MAX_INVENTORY_BYTES {
        let message = format!("{what} file exceeds {MAX_INVENTORY_BYTES} byte limit");
        tracing::warn!("refusing inventory at {}: {message}", path.display());
        return Err((ErrorKind::Read, message));
    }
    let bytes = fs::read(path).map_err(io_fail)?;
    Ok((bytes, meta.modified().map_or(0.0, mtime_ms)))
}

fn parse_raw(raw: &str) -> Result<Value, ReadFail> {
    let parsed: Value = serde_json::from_str(raw).map_err(|e| (ErrorKind::Parse, e.to_string()))?;
    let data = unwrap_inventory_payload(parsed, None);
    if !has_inventory_shape(&data) {
        return Err((
            ErrorKind::Parse,
            "Inventory JSON does not contain expected inventory arrays".into(),
        ));
    }
    Ok(data)
}

/// (data, content hash, file mtime ms)
type Read = (Value, String, f64);

fn read_json(path: &Path) -> Result<Read, ReadFail> {
    let (bytes, modified) = read_checked(path, "Inventory")?;
    let raw = String::from_utf8_lossy(&bytes);
    Ok((parse_raw(&raw)?, sha256_hex(raw.as_bytes()), modified))
}

fn read_aleca(path: &Path) -> Result<Read, ReadFail> {
    let (bytes, modified) = read_checked(path, "AlecaFrame inventory")?;
    let data = aleca::parse_buffer(&bytes).map_err(|m| (ErrorKind::Parse, m))?;
    Ok((data, sha256_hex(&bytes), modified))
}

type Stamp = (SystemTime, u64);

fn stamp(path: &Path) -> io::Result<Stamp> {
    let m = fs::metadata(path)?;
    Ok((m.modified()?, m.len()))
}

struct Watch {
    path: PathBuf,
    seen: Option<Stamp>,
    pending: Option<Stamp>,
}

impl Watch {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            seen: stamp(path).ok(),
            pending: None,
        }
    }
}

#[derive(Default)]
struct Inner {
    source: InventorySource,
    hash: Option<String>,
    imported_at: Option<f64>,
    modified_at: Option<f64>,
    path: Option<PathBuf>,
    snapshot: Option<Arc<Value>>,
    /// Snapshot with the deltas applied; what consumers see.
    effective: Option<Arc<Value>>,
    deltas: Vec<Delta>,
    game_synced_at: Option<f64>,
    last_error: Option<ReadError>,
    watch: Option<Watch>,
}

impl Inner {
    fn status(&self) -> InventoryStatus {
        let groups = |kind| {
            let mut g: Vec<&str> = self
                .deltas
                .iter()
                .filter(|d| d.kind == kind)
                .map(|d| d.group.as_str())
                .collect();
            g.sort_unstable();
            g.dedup();
            g.len() as u32
        };
        InventoryStatus {
            path: self.path.as_ref().map(|p| p.display().to_string()),
            found: self.snapshot.is_some(),
            source: self.source,
            modified_at: self.modified_at,
            last_error: self.last_error.clone(),
            imported_at: self.imported_at,
            trades_applied: groups(DeltaKind::Trade),
            picks_applied: groups(DeltaKind::Relic),
            game_synced_since_import: matches!(
                (self.game_synced_at, self.imported_at),
                (Some(g), Some(i)) if g > i
            ),
        }
    }

    fn fail(&mut self, path: &Path, (kind, message): ReadFail) {
        tracing::error!("failed to read inventory at {}: {message}", path.display());
        self.last_error = Some(ReadError {
            kind,
            message,
            path: path.display().to_string(),
            at: now_ms_f64(),
        });
    }

    fn refresh(&mut self) {
        self.effective = self
            .snapshot
            .as_ref()
            .map(|s| Arc::new(delta::apply(s, &self.deltas)));
    }

    fn set_snapshot(&mut self, row: &store::Row, data: Value, deltas: Vec<Delta>) {
        self.source = InventorySource::parse(&row.source);
        self.hash = Some(row.hash.clone());
        self.imported_at = Some(row.imported_at);
        self.modified_at = row.file_modified_at;
        self.path = row.path.as_deref().map(PathBuf::from);
        self.snapshot = Some(Arc::new(data));
        self.deltas = deltas;
        self.last_error = None;
        self.refresh();
    }
}

/// A recorded EE.log trade moves the imported counts (exact: the game confirmed it).
pub fn apply_trade(app: &tauri::AppHandle, event: &crate::services::gdpr_import::TradeEvent) {
    use tauri::Manager;
    let deltas = delta::from_trade(event, now_ms_f64());
    app.state::<Arc<Inventory>>()
        .add_deltas(&app.state::<Db>(), deltas);
}

pub struct Inventory {
    sink: Arc<dyn Sink>,
    inner: Mutex<Inner>,
}

impl Inventory {
    pub fn new(sink: Arc<dyn Sink>) -> Self {
        Self {
            sink,
            inner: Mutex::default(),
        }
    }

    pub fn source(&self) -> InventorySource {
        self.inner.lock().unwrap().source
    }

    pub fn status(&self) -> InventoryStatus {
        self.inner.lock().unwrap().status()
    }

    /// The inventory consumers read: snapshot plus deltas.
    pub fn current(&self) -> Option<Arc<Value>> {
        self.inner.lock().unwrap().effective.clone()
    }

    pub fn get(&self) -> Option<Arc<Value>> {
        self.current()
    }

    /// Restores the persisted snapshot and deltas, and re-attaches the AlecaFrame watcher.
    pub fn load(&self, db: &Db) -> Option<Arc<Value>> {
        let (row, deltas) = {
            let conn = db.0.lock().unwrap();
            (
                store::load_snapshot(&conn).ok().flatten()?,
                store::load_deltas(&conn).unwrap_or_default(),
            )
        };
        let data = serde_json::from_str::<Value>(&row.json).ok()?;
        let mut i = self.inner.lock().unwrap();
        i.set_snapshot(&row, data, deltas);
        if i.source == InventorySource::Aleca {
            i.watch = i.path.as_deref().map(Watch::new);
        }
        self.sink.status(&i.status());
        i.effective.clone()
    }

    /// Imports `path` as the new snapshot: `.dat` is an AlecaFrame cache, anything else inventory JSON.
    pub fn import_path(&self, db: &Db, path: &Path) -> Option<Arc<Value>> {
        let aleca = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dat"));
        let read = if aleca {
            read_aleca(path)
        } else {
            read_json(path)
        };
        let mut i = self.inner.lock().unwrap();
        match read {
            Ok(r) => {
                let source = if aleca {
                    InventorySource::Aleca
                } else {
                    InventorySource::Manual
                };
                Some(self.commit(&mut i, db, source, path, r))
            }
            Err(f) => {
                i.fail(path, f);
                self.sink.status(&i.status());
                None
            }
        }
    }

    /// A fresh warframe-api-helper `inventory.json`; unchanged content keeps the deltas.
    pub fn import_helper(&self, db: &Db, path: &Path) {
        let read = read_json(path);
        let mut i = self.inner.lock().unwrap();
        match read {
            Ok(r) if i.hash.as_deref() == Some(&r.1) => {}
            Ok(r) => {
                self.commit(&mut i, db, InventorySource::Helper, path, r);
            }
            Err(f) => {
                i.fail(path, f);
                self.sink.status(&i.status());
            }
        }
    }

    fn commit(
        &self,
        i: &mut Inner,
        db: &Db,
        source: InventorySource,
        path: &Path,
        (data, hash, modified): Read,
    ) -> Arc<Value> {
        let row = store::Row {
            source: source.as_str().into(),
            imported_at: now_ms_f64(),
            file_modified_at: Some(modified),
            path: Some(path.display().to_string()),
            hash,
            json: data.to_string(),
        };
        if let Err(e) = store::save_snapshot(&mut db.0.lock().unwrap(), &row) {
            tracing::warn!("inventory snapshot not persisted: {e}");
        }
        i.set_snapshot(&row, data, Vec::new());
        i.watch = (source == InventorySource::Aleca).then(|| Watch::new(path));
        let d = i.effective.clone().expect("just set");
        self.sink.updated(Some(&d));
        self.sink.status(&i.status());
        d
    }

    /// Forgets the snapshot and its deltas.
    pub fn clear(&self, db: &Db) {
        if let Err(e) = store::clear(&mut db.0.lock().unwrap()) {
            tracing::warn!("inventory snapshot not cleared: {e}");
        }
        let mut i = self.inner.lock().unwrap();
        *i = Inner {
            game_synced_at: i.game_synced_at,
            ..Inner::default()
        };
        self.sink.updated(None);
        self.sink.status(&i.status());
    }

    /// Applies deltas on top of the snapshot. False when there is no snapshot to apply them to.
    pub fn add_deltas(&self, db: &Db, deltas: Vec<Delta>) -> bool {
        let mut i = self.inner.lock().unwrap();
        if i.snapshot.is_none() || deltas.is_empty() {
            return false;
        }
        if let Err(e) = store::insert_deltas(&mut db.0.lock().unwrap(), &deltas) {
            tracing::warn!("inventory deltas not persisted: {e}");
        }
        i.deltas.extend(deltas);
        i.refresh();
        self.sink.updated(i.effective.as_deref());
        self.sink.status(&i.status());
        true
    }

    /// EE.log showed the game fetching its inventory (`at` = epoch ms).
    pub fn note_game_sync(&self, at: f64) {
        let mut i = self.inner.lock().unwrap();
        let before = i.status();
        i.game_synced_at = Some(at);
        let after = i.status();
        if before != after {
            self.sink.status(&after);
        }
    }

    /// The effective inventory as indented JSON text.
    pub fn export_text(&self) -> Result<String, ExportError> {
        let data = self.current().ok_or(ExportError::NoInventory)?;
        let text = serde_json::to_string(&*data).map_err(|_| ExportError::WriteFailed)?;
        Ok(indent_json_text(&text))
    }

    /// One watcher tick for the AlecaFrame file; the app calls this every 500 ms.
    pub fn poll(&self, db: &Db) {
        let mut i = self.inner.lock().unwrap();
        let Some(w) = i.watch.as_mut() else { return };
        let path = w.path.clone();
        let cur = stamp(&path).ok();
        if cur == w.seen {
            w.pending = None;
            return;
        }
        // Same stamp on two polls in a row: the writer is done (chokidar's awaitWriteFinish).
        if cur.is_none() || w.pending != cur {
            w.pending = cur;
            if cur.is_none() {
                w.seen = None;
            }
            return;
        }
        w.seen = cur;
        w.pending = None;
        match read_aleca(&path) {
            Ok(r) if i.hash.as_deref() != Some(&r.1) => {
                self.commit(&mut i, db, InventorySource::Aleca, &path, r);
            }
            Ok((_, _, modified)) => i.modified_at = Some(modified),
            Err(f) => {
                i.fail(&path, f);
                self.sink.status(&i.status());
            }
        }
    }
}
