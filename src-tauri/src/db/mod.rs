//! SQLite behind one Mutex'd connection; migrations tracked by name in schema_migrations.

use std::path::Path;
use std::sync::Mutex;

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

pub struct Db(pub Mutex<Connection>);

pub fn open(path: &Path) -> rusqlite::Result<Db> {
    let mut conn = Connection::open(path)?;
    migrate(&mut conn)?;
    Ok(Db(Mutex::new(conn)))
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
    Db(Mutex::new(mem_conn()))
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
}
