//! SQLite behind one Mutex'd connection; migrations tracked by name in schema_migrations.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

/// SQLite error to the `String` a command returns.
pub fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
}

/// Append only, at the end. Applied by name, so order never decides *whether* an entry runs;
/// SQL must be idempotent so DBs from the old `user_version` scheme can re-run everything.
const MIGRATIONS: &[(&str, &str)] = &[
    ("meta",
     "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)"),
    ("http_kv_cache",
     "CREATE TABLE IF NOT EXISTS http_cache (url TEXT PRIMARY KEY, etag TEXT, fetched_at INTEGER NOT NULL, body BLOB NOT NULL);
     CREATE TABLE IF NOT EXISTS kv_cache (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL)"),
    ("items",
     "CREATE TABLE IF NOT EXISTS items (unique_name TEXT PRIMARY KEY, category TEXT NOT NULL, name TEXT NOT NULL, locale TEXT, raw TEXT NOT NULL);
     CREATE INDEX IF NOT EXISTS items_category ON items (category);
     CREATE TABLE IF NOT EXISTS item_components (item TEXT NOT NULL REFERENCES items (unique_name) ON DELETE CASCADE, component TEXT NOT NULL, count INTEGER NOT NULL, PRIMARY KEY (item, component))"),
    ("relics_drops",
     "CREATE TABLE IF NOT EXISTS relics (unique_name TEXT PRIMARY KEY, era TEXT NOT NULL, name TEXT NOT NULL, vaulted INTEGER NOT NULL DEFAULT 0);
     CREATE TABLE IF NOT EXISTS relic_rewards (relic TEXT NOT NULL REFERENCES relics (unique_name) ON DELETE CASCADE, item TEXT NOT NULL, rarity TEXT NOT NULL, PRIMARY KEY (relic, item));
     CREATE TABLE IF NOT EXISTS drop_sources (id INTEGER PRIMARY KEY, kind TEXT NOT NULL, name TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS drop_rows (source INTEGER NOT NULL REFERENCES drop_sources (id) ON DELETE CASCADE, item TEXT NOT NULL, rarity TEXT, chance REAL);
     CREATE INDEX IF NOT EXISTS drop_rows_item ON drop_rows (item);
     CREATE TABLE IF NOT EXISTS codex_scans (account TEXT NOT NULL, entry TEXT NOT NULL, scans INTEGER NOT NULL, PRIMARY KEY (account, entry))"),
    ("price_history",
     "CREATE TABLE IF NOT EXISTS price_history (slug TEXT NOT NULL, day TEXT NOT NULL, rank INTEGER NOT NULL DEFAULT -1, source TEXT NOT NULL, time REAL NOT NULL, volume REAL, median REAL NOT NULL, moving_avg REAL, avg_price REAL, open_price REAL, closed_price REAL, min_price REAL, max_price REAL, donch_top REAL, donch_bot REAL, PRIMARY KEY (slug, day, rank)) WITHOUT ROWID"),
    ("trade_events",
     "CREATE TABLE IF NOT EXISTS trade_events (id TEXT PRIMARY KEY, ts TEXT NOT NULL, day_key TEXT NOT NULL, kind TEXT NOT NULL, direction TEXT NOT NULL, partner TEXT, platinum INTEGER NOT NULL, source TEXT, batch_id TEXT, json TEXT NOT NULL);
     CREATE INDEX IF NOT EXISTS trade_events_day ON trade_events (day_key);
     CREATE INDEX IF NOT EXISTS trade_events_batch ON trade_events (batch_id)"),
    ("runs",
     "CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, kind TEXT NOT NULL, started_at INTEGER NOT NULL, ended_at INTEGER NOT NULL, end_reason TEXT NOT NULL, source TEXT NOT NULL, summary TEXT NOT NULL, notes TEXT, tags TEXT NOT NULL DEFAULT '[]', duplicate_of TEXT, log_size INTEGER NOT NULL DEFAULT 0);
     CREATE INDEX IF NOT EXISTS runs_kind_started ON runs (kind, started_at)"),
    ("run_events",
     "CREATE TABLE IF NOT EXISTS run_events (run_id TEXT PRIMARY KEY, log BLOB NOT NULL)"),
    ("notification_log",
     "CREATE TABLE IF NOT EXISTS notification_log (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, at TEXT NOT NULL, kind TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL)"),
    // Market alerts: rules (JSON), dedup keys, owned-count snapshots, hit history.
    ("market_alerts",
     "CREATE TABLE IF NOT EXISTS market_alerts (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, rule TEXT NOT NULL, native INTEGER NOT NULL DEFAULT 1);
     CREATE TABLE IF NOT EXISTS market_alert_owned (slug TEXT PRIMARY KEY, count INTEGER NOT NULL);
     CREATE TABLE IF NOT EXISTS market_alert_seen (rule_id TEXT NOT NULL, key TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (rule_id, key)) WITHOUT ROWID;
     CREATE TABLE IF NOT EXISTS alert_events (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, rule_id TEXT NOT NULL, at TEXT NOT NULL, hit TEXT NOT NULL)"),
    // Workbench: write-ahead journal of bulk-sell intents, override audit acks, pre-run order ids.
    ("workbench_journal",
     "CREATE TABLE IF NOT EXISTS workbench_intents (seq INTEGER PRIMARY KEY AUTOINCREMENT, intent_id TEXT NOT NULL UNIQUE, plan_id TEXT NOT NULL, entry TEXT NOT NULL, settled INTEGER NOT NULL DEFAULT 0);
     CREATE TABLE IF NOT EXISTS workbench_overrides (seq INTEGER PRIMARY KEY AUTOINCREMENT, ack TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS workbench_preexisting (plan_id TEXT PRIMARY KEY, order_ids TEXT NOT NULL)"),
    // Live riven roll sessions.
    ("riven_sessions",
     "CREATE TABLE IF NOT EXISTS riven_sessions (id TEXT PRIMARY KEY, weapon TEXT NOT NULL, rolls INTEGER NOT NULL, kuva_spent INTEGER NOT NULL, started_at INTEGER NOT NULL, ended_at INTEGER NOT NULL, notes TEXT, pinned INTEGER NOT NULL DEFAULT 0)"),
    // Mission rewards: one row per recorded mission, its gained items alongside.
    ("mission_rewards",
     "CREATE TABLE IF NOT EXISTS mission_rewards (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, ended_at REAL NOT NULL, read_at REAL NOT NULL, mission_count INTEGER NOT NULL, mission_type TEXT, node TEXT, credits REAL NOT NULL, endo REAL NOT NULL, baseline_at REAL);
     CREATE TABLE IF NOT EXISTS mission_reward_items (mission_id TEXT NOT NULL, unique_name TEXT NOT NULL, count INTEGER NOT NULL, PRIMARY KEY (mission_id, unique_name)) WITHOUT ROWID;
     CREATE INDEX IF NOT EXISTS mission_reward_items_name ON mission_reward_items (unique_name)"),
    // Daily stats: one JSON entry per local day.
    ("stats_daily",
     "CREATE TABLE IF NOT EXISTS stats_daily (date TEXT PRIMARY KEY, json TEXT NOT NULL)"),
    ("inventory_snapshot",
     "CREATE TABLE IF NOT EXISTS inventory_snapshot (id INTEGER PRIMARY KEY CHECK (id = 1), source TEXT NOT NULL, imported_at REAL NOT NULL, file_modified_at REAL, path TEXT, hash TEXT NOT NULL, json TEXT NOT NULL);
     CREATE TABLE IF NOT EXISTS inventory_deltas (seq INTEGER PRIMARY KEY AUTOINCREMENT, at REAL NOT NULL, kind TEXT NOT NULL, group_id TEXT NOT NULL, unique_name TEXT, label TEXT NOT NULL, count INTEGER NOT NULL, platinum INTEGER NOT NULL DEFAULT 0, estimated INTEGER NOT NULL DEFAULT 0)"),
    ("drop_mission_rewards",
     "DROP TABLE IF EXISTS mission_reward_items; DROP TABLE IF EXISTS mission_rewards"),
    // Mission rewards came back as part of the opt-in game-memory mode.
    ("mission_rewards_restore",
     "CREATE TABLE IF NOT EXISTS mission_rewards (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL UNIQUE, ended_at REAL NOT NULL, read_at REAL NOT NULL, mission_count INTEGER NOT NULL, mission_type TEXT, node TEXT, credits REAL NOT NULL, endo REAL NOT NULL, baseline_at REAL);
     CREATE TABLE IF NOT EXISTS mission_reward_items (mission_id TEXT NOT NULL, unique_name TEXT NOT NULL, count INTEGER NOT NULL, PRIMARY KEY (mission_id, unique_name)) WITHOUT ROWID;
     CREATE INDEX IF NOT EXISTS mission_reward_items_name ON mission_reward_items (unique_name)"),
];

/// Readers beside the writer: WAL lets them run while a write is in flight.
const READERS: usize = 3;
const BACKUPS_KEPT: usize = 3;
/// `kv_cache` price entries and `http_cache` rows older than this go at startup.
const PRUNE_AFTER_SECS: i64 = 30 * 86_400;
const PRUNE_EVERY_SECS: i64 = 86_400;

/// One writer connection plus a few read-only ones. Without readers (in-memory test DBs, whose
/// connections cannot share data) `read` hands out the writer.
pub struct Db {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    next: AtomicUsize,
}

impl Db {
    pub fn new(writer: Connection) -> Self {
        Self::with_readers(writer, Vec::new())
    }

    fn with_readers(writer: Connection, readers: Vec<Connection>) -> Self {
        Db {
            writer: Mutex::new(writer),
            readers: readers.into_iter().map(Mutex::new).collect(),
            next: AtomicUsize::new(0),
        }
    }

    /// The writer (it reads too). Panics on a poisoned lock, as every call site did before.
    pub fn lock(&self) -> MutexGuard<'_, Connection> {
        self.writer.lock().unwrap()
    }

    pub fn writer(&self) -> &Mutex<Connection> {
        &self.writer
    }

    /// A free reader, trying each once from a rotating start, else waiting on one. Read-only paths only:
    /// a reader never sees an uncommitted write, so nothing that reads then writes belongs here.
    pub fn read(&self) -> MutexGuard<'_, Connection> {
        let n = self.readers.len();
        if n == 0 {
            return self.lock();
        }
        let start = self.next.fetch_add(1, Ordering::Relaxed) % n;
        for i in 0..n {
            match self.readers[(start + i) % n].try_lock() {
                Ok(g) => return g,
                // A reader holds no state worth losing: take it over.
                Err(TryLockError::Poisoned(p)) => return p.into_inner(),
                Err(TryLockError::WouldBlock) => {}
            }
        }
        self.readers[start]
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

pub fn open(path: &Path) -> rusqlite::Result<Db> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000; PRAGMA temp_store=MEMORY;",
    )?;
    backup_before_migrating(&conn, path)?;
    migrate(&mut conn)?;
    if let Err(e) = prune(&conn, now_secs()) {
        tracing::warn!(error = %e, "db prune failed");
    }
    // Opened after the migrations so they see the final schema.
    let readers = (0..READERS)
        .map(|_| {
            let r = Connection::open(path)?;
            r.execute_batch("PRAGMA busy_timeout=5000; PRAGMA query_only=ON;")?;
            Ok(r)
        })
        .collect::<rusqlite::Result<_>>()?;
    Ok(Db::with_readers(conn, readers))
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Whether a migration is waiting on a file that already holds tables.
fn migration_pending_on_data(conn: &Connection) -> rusqlite::Result<bool> {
    let has_data: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%')",
        [],
        |r| r.get(0),
    )?;
    if !has_data {
        return Ok(false);
    }
    let tracked: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'schema_migrations')",
        [],
        |r| r.get(0),
    )?;
    if !tracked {
        return Ok(true);
    }
    // Names, not a count: an entry missing from the middle still runs.
    let mut stmt = conn.prepare("SELECT 1 FROM schema_migrations WHERE name = ?1")?;
    for (name, _) in MIGRATIONS {
        if !stmt.exists([name])? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `VACUUM INTO` a fresh file under `<data dir>/backups` before any pending migration touches existing data;
/// the 3 newest stay. A failure aborts the start: never migrate without a backup.
fn backup_before_migrating(conn: &Connection, db_path: &Path) -> rusqlite::Result<()> {
    if !migration_pending_on_data(conn)? {
        return Ok(());
    }
    let fail = |m: String| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(m),
        )
    };
    let dir = db_path.parent().unwrap_or(Path::new(".")).join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| {
        fail(format!(
            "cannot create the backup folder before migrating: {e}"
        ))
    })?;
    let target = dir.join(format!("companionframe-{}.db", now_secs()));
    conn.execute("VACUUM INTO ?1", [target.to_string_lossy()])
        .map_err(|e| fail(format!("backup before migrating failed: {e}")))?;
    // Oldest first by the timestamp in the name (numerically: the digit count may differ).
    let mut old: Vec<(i64, std::path::PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| fail(e.to_string()))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let ts = name
                .strip_prefix("companionframe-")?
                .strip_suffix(".db")?
                .parse()
                .ok()?;
            Some((ts, e.path()))
        })
        .collect();
    old.sort();
    for (_, p) in old.iter().take(old.len().saturating_sub(BACKUPS_KEPT)) {
        let _ = std::fs::remove_file(p);
    }
    tracing::info!(backup = %target.display(), "db backed up before migrating");
    Ok(())
}

/// Once a day: old price cache entries and HTTP cache rows. User data is never touched.
pub(crate) fn prune(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    let last: i64 = conn
        .query_row("SELECT value FROM meta WHERE key = 'last_prune'", [], |r| {
            r.get::<_, String>(0)
        })
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if now - last < PRUNE_EVERY_SECS {
        return Ok(());
    }
    let cutoff = now - PRUNE_AFTER_SECS;
    let prices = conn.execute(
        "DELETE FROM kv_cache WHERE key LIKE 'wfm:price:%' AND updated_at < ?1",
        [cutoff],
    )?;
    let http = conn.execute("DELETE FROM http_cache WHERE fetched_at < ?1", [cutoff])?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('last_prune', ?1)",
        [now.to_string()],
    )?;
    conn.execute_batch("PRAGMA optimize")?;
    tracing::info!(prices, http, "db pruned");
    Ok(())
}

pub(crate) fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY, applied_at INTEGER NOT NULL)",
    )?;
    for (name, sql) in MIGRATIONS {
        let done: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM schema_migrations WHERE name = ?1)",
            [name],
            |r| r.get(0),
        )?;
        if done {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_migrations VALUES (?1, unixepoch())",
            [name],
        )?;
        tx.commit()?;
    }
    Ok(())
}

/// A migrated in-memory database, for tests.
#[cfg(test)]
pub fn mem_conn() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&mut conn).unwrap();
    conn
}

#[cfg(test)]
pub fn mem_db() -> Db {
    Db::new(mem_conn())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(conn: &Connection) -> usize {
        count(conn, "schema_migrations") as usize
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(applied(&conn), MIGRATIONS.len());
        for t in [
            "meta",
            "http_cache",
            "kv_cache",
            "items",
            "item_components",
            "relics",
            "relic_rewards",
            "drop_sources",
            "drop_rows",
            "codex_scans",
            "price_history",
            "trade_events",
            "runs",
            "run_events",
            "notification_log",
            "riven_sessions",
        ] {
            count(&conn, t);
        }
    }

    #[test]
    fn adopts_old_user_version_db() {
        // Old scheme at version 13 that skipped the runs tables.
        let mut conn = Connection::open_in_memory().unwrap();
        for (name, sql) in MIGRATIONS {
            if !name.starts_with("run") {
                conn.execute_batch(sql).unwrap();
            }
        }
        conn.pragma_update(None, "user_version", 13).unwrap();
        conn.execute("INSERT INTO meta VALUES ('k', 'v')", [])
            .unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(applied(&conn), MIGRATIONS.len());
        assert_eq!(count(&conn, "runs"), 0);
        assert_eq!(count(&conn, "meta"), 1);
    }

    #[test]
    fn applies_entry_missing_from_the_middle() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.execute("DELETE FROM schema_migrations WHERE name = 'runs'", [])
            .unwrap();
        conn.execute_batch("DROP TABLE runs").unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(count(&conn, "runs"), 0);
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!(
            "cf-db-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn backups(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(dir.join("backups"))
            .map(|r| {
                r.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    #[test]
    fn file_db_uses_wal_and_readers_see_commits() {
        let dir = temp_dir("wal");
        let db = open(&dir.join("t.db")).unwrap();
        let mode: String = db
            .lock()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
        let ro: i64 = db
            .read()
            .query_row("PRAGMA query_only", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ro, 1);
        assert!(
            db.read()
                .execute("INSERT INTO meta VALUES ('x','y')", [])
                .is_err()
        );
        db.lock()
            .execute("INSERT INTO meta VALUES ('k','v')", [])
            .unwrap();
        let got: String = db
            .read()
            .query_row("SELECT value FROM meta WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(got, "v");
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn readers_are_distinct_connections_and_memory_falls_back_to_the_writer() {
        let dir = temp_dir("pool");
        let db = open(&dir.join("t.db")).unwrap();
        let a = db.read();
        let b = db.read();
        let c = db.read();
        let id = |g: &MutexGuard<Connection>| g.path().map(str::to_owned);
        assert_eq!(id(&a), id(&b));
        // Three held at once: the pool really has three separate guards, and a fourth waits for one to drop.
        assert!(db.readers.iter().all(|r| r.try_lock().is_err()));
        drop((a, b, c));
        assert!(db.readers.iter().all(|r| r.try_lock().is_ok()));
        drop(db);
        let _ = std::fs::remove_dir_all(dir);

        let mem = mem_db();
        mem.lock()
            .execute("INSERT INTO meta VALUES ('k','v')", [])
            .unwrap();
        let n: i64 = mem
            .read()
            .query_row("SELECT count(*) FROM meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn backup_only_when_a_migration_is_pending_on_existing_data() {
        let dir = temp_dir("backup");
        let path = dir.join("t.db");
        drop(open(&path).unwrap());
        assert!(
            backups(&dir).is_empty(),
            "a fresh file has nothing to back up"
        );
        drop(open(&path).unwrap());
        assert!(backups(&dir).is_empty(), "nothing pending");

        let c = Connection::open(&path).unwrap();
        c.execute(
            "DELETE FROM schema_migrations WHERE name = 'stats_daily'",
            [],
        )
        .unwrap();
        c.execute_batch("DROP TABLE stats_daily").unwrap();
        drop(c);
        drop(open(&path).unwrap());
        assert_eq!(backups(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn backups_keep_the_three_newest() {
        let dir = temp_dir("trim");
        let path = dir.join("t.db");
        std::fs::create_dir_all(dir.join("backups")).unwrap();
        for ts in [100, 200, 300, 400] {
            std::fs::write(
                dir.join("backups").join(format!("companionframe-{ts}.db")),
                b"x",
            )
            .unwrap();
        }
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        backup_before_migrating(&conn, &path).unwrap();
        let left = backups(&dir);
        assert_eq!(left.len(), 3);
        assert!(!left.contains(&"companionframe-100.db".to_owned()));
        assert!(!left.contains(&"companionframe-200.db".to_owned()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_drops_old_price_cache_only_and_runs_once_a_day() {
        let conn = mem_conn();
        let now = 100 * 86_400;
        let old = now - PRUNE_AFTER_SECS - 1;
        for (k, at) in [
            ("wfm:price:old", old),
            ("wfm:price:new", now - 10),
            ("other:old", old),
        ] {
            conn.execute(
                "INSERT INTO kv_cache VALUES (?1, '{}', ?2)",
                rusqlite::params![k, at],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO http_cache VALUES ('u1', NULL, ?1, x'00'), ('u2', NULL, ?2, x'00')",
            [old, now],
        )
        .unwrap();
        conn.execute("INSERT INTO trade_events VALUES ('e', '2020-01-01', '2020-01-01', 'k', 'sell', NULL, 1, NULL, NULL, '{}')", []).unwrap();
        prune(&conn, now).unwrap();
        let keys: Vec<String> = conn
            .prepare("SELECT key FROM kv_cache ORDER BY key")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(keys, ["other:old", "wfm:price:new"]);
        assert_eq!(count(&conn, "http_cache"), 1);
        assert_eq!(count(&conn, "trade_events"), 1);
        // Within a day nothing runs again.
        conn.execute(
            "INSERT INTO kv_cache VALUES ('wfm:price:older', '{}', ?1)",
            [old],
        )
        .unwrap();
        prune(&conn, now + 60).unwrap();
        assert_eq!(count(&conn, "kv_cache"), 3);
        prune(&conn, now + PRUNE_EVERY_SECS).unwrap();
        assert_eq!(count(&conn, "kv_cache"), 2);
    }
}
