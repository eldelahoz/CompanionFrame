//! SQLite-backed `kv_cache` (small JSON blobs).

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, params};

use crate::db::Db;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn kv_get(db: &Db, key: &str) -> rusqlite::Result<Option<String>> {
    db.0.lock()
        .unwrap()
        .query_row("SELECT value FROM kv_cache WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()
}

pub fn kv_put(db: &Db, key: &str, value: &str) -> rusqlite::Result<()> {
    db.0.lock().unwrap().execute(
        "INSERT OR REPLACE INTO kv_cache (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params![key, value, now()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_db;

    #[test]
    fn kv_roundtrip_and_overwrite() {
        let db = mem_db();
        assert_eq!(kv_get(&db, "k").unwrap(), None);
        kv_put(&db, "k", "1").unwrap();
        kv_put(&db, "k", "2").unwrap();
        assert_eq!(kv_get(&db, "k").unwrap().as_deref(), Some("2"));
    }
}
