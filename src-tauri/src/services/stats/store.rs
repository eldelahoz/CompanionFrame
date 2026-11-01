//! `stats_daily`: one JSON row per local day, newest 365 kept.
use rusqlite::{Connection, params};

use super::tracker::DailyStatEntry;

pub const MAX_DAYS: i64 = 365;

pub fn load_all(conn: &Connection) -> rusqlite::Result<Vec<DailyStatEntry>> {
    let rows = conn
        .prepare("SELECT json FROM stats_daily ORDER BY date")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    // A row this build cannot read is dropped rather than failing the whole history.
    Ok(rows
        .iter()
        .filter_map(|j| serde_json::from_str(j).ok())
        .collect())
}

pub fn upsert(conn: &Connection, e: &DailyStatEntry) -> rusqlite::Result<()> {
    let json =
        serde_json::to_string(e).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    conn.execute(
        "INSERT INTO stats_daily (date, json) VALUES (?1, ?2) ON CONFLICT (date) DO UPDATE SET json = excluded.json",
        params![e.date, json],
    )?;
    conn.execute(
        "DELETE FROM stats_daily WHERE date NOT IN (SELECT date FROM stats_daily ORDER BY date DESC LIMIT ?1)",
        [MAX_DAYS],
    )?;
    Ok(())
}

/// Stores the imported days over the existing ones, skipping `today` (the live tracker owns it).
pub fn import(
    conn: &mut Connection,
    entries: &[DailyStatEntry],
    today: &str,
) -> rusqlite::Result<u32> {
    let tx = conn.transaction()?;
    let mut n = 0;
    for e in entries.iter().filter(|e| e.date != today) {
        upsert(&tx, e)?;
        n += 1;
    }
    tx.commit()?;
    Ok(n)
}

pub fn clear(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM stats_daily", [])?;
    Ok(())
}
