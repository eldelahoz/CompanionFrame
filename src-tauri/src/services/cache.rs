//! SQLite-backed `kv_cache` (small JSON blobs).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, params};

use crate::db::Db;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Running totals of `kv_get` calls and microseconds (lock wait included); callers diff two snapshots for the timing logs.
static READS: AtomicU64 = AtomicU64::new(0);
static READ_US: AtomicU64 = AtomicU64::new(0);

/// `(calls, microseconds)` spent in `kv_get` so far.
pub fn kv_read_totals() -> (u64, u64) {
    (
        READS.load(Ordering::Relaxed),
        READ_US.load(Ordering::Relaxed),
    )
}

pub fn kv_get(db: &Db, key: &str) -> rusqlite::Result<Option<String>> {
    let t = Instant::now();
    let r = db
        .read()
        .query_row("SELECT value FROM kv_cache WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional();
    READS.fetch_add(1, Ordering::Relaxed);
    READ_US.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
    r
}

/// SQLite's default limit is 999 bound variables; stay well under it.
const CHUNK: usize = 500;

/// Many keys in one pass (a missing key is absent from the map); counted as one read in the timing totals.
pub fn kv_get_many(db: &Db, keys: &[String]) -> rusqlite::Result<HashMap<String, String>> {
    let t = Instant::now();
    let mut out = HashMap::with_capacity(keys.len());
    {
        let conn = db.read();
        for chunk in keys.chunks(CHUNK) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = conn.prepare_cached(&format!(
                "SELECT key, value FROM kv_cache WHERE key IN ({marks})"
            ))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            for row in rows {
                let (k, v) = row?;
                out.insert(k, v);
            }
        }
    }
    READS.fetch_add(1, Ordering::Relaxed);
    READ_US.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
    Ok(out)
}

pub fn kv_put(db: &Db, key: &str, value: &str) -> rusqlite::Result<()> {
    db.lock().execute(
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

    #[test]
    fn kv_get_many_chunks_and_skips_missing() {
        let db = mem_db();
        let keys: Vec<String> = (0..1200).map(|i| format!("k{i}")).collect();
        for k in keys.iter().step_by(2) {
            kv_put(&db, k, &format!("v-{k}")).unwrap();
        }
        let got = kv_get_many(&db, &keys).unwrap();
        assert_eq!(got.len(), 600);
        assert_eq!(got["k1198"], "v-k1198");
        assert!(!got.contains_key("k1"));
        assert!(kv_get_many(&db, &[]).unwrap().is_empty());
    }
}
