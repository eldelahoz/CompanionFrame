//! Rivens seen whole on a single-card screen, kept in `kv_cache` (no migration) so the next compare
//! screen can take its Current card from here instead of reading the dimmed one. No account ids.
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::services::riven_scan::text::RivenStat;

const PREFIX: &str = "riven_card:";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RememberedCard {
    pub weapon: String,
    pub stats: Vec<RivenStat>,
    pub mr: Option<u32>,
    pub rolls: Option<u32>,
    pub grade: Option<String>,
    /// Unix seconds.
    pub read_at: i64,
}

/// A riven's name is derived from its stats, so weapon + stats is the same identity.
fn key(c: &RememberedCard) -> String {
    let stats: Vec<String> = c
        .stats
        .iter()
        .map(|s| format!("{}{}", s.name, s.value.unwrap_or_default()))
        .collect();
    format!("{PREFIX}{}|{}", c.weapon, stats.join(","))
}

pub fn remember(conn: &Connection, card: &RememberedCard) -> rusqlite::Result<()> {
    let value = serde_json::to_string(card)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    conn.execute(
        "INSERT OR REPLACE INTO kv_cache (key, value, updated_at) VALUES (?1, ?2, ?3)",
        params![key(card), value, card.read_at],
    )?;
    Ok(())
}

/// The newest remembered card of `weapon` with `rolls` cycles: the one the dimmed card shows.
pub fn recall(conn: &Connection, weapon: &str, rolls: u32) -> Option<RememberedCard> {
    let mut q = conn
        .prepare(
            "SELECT value FROM kv_cache WHERE key LIKE 'riven_card:%' ORDER BY updated_at DESC, rowid DESC",
        )
        .ok()?;
    let rows = q.query_map([], |r| r.get::<_, String>(0)).ok()?;
    rows.filter_map(Result::ok)
        .filter_map(|v| serde_json::from_str::<RememberedCard>(&v).ok())
        .find(|c| c.weapon == weapon && c.rolls == Some(rolls))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_db;

    fn card(weapon: &str, rolls: u32, v: f64, read_at: i64) -> RememberedCard {
        RememberedCard {
            weapon: weapon.into(),
            stats: vec![RivenStat {
                name: "Status Chance".into(),
                positive: true,
                display_positive: None,
                value: Some(v),
                multiplier: false,
            }],
            mr: Some(15),
            rolls: Some(rolls),
            grade: Some("A-".into()),
            read_at,
        }
    }

    #[test]
    fn a_card_round_trips_by_weapon_and_rolls_newest_first() {
        let db = mem_db();
        let conn = db.lock();
        assert_eq!(recall(&conn, "Staticor", 7), None);
        let (a, b) = (
            card("Staticor", 7, 94.2, 10),
            card("Staticor", 7, 146.5, 20),
        );
        remember(&conn, &a).unwrap();
        remember(&conn, &b).unwrap();
        remember(&conn, &card("Staticor", 4, 1.0, 30)).unwrap();
        assert_eq!(recall(&conn, "Staticor", 7), Some(b));
        assert_eq!(recall(&conn, "Staticor", 5), None);
        assert_eq!(recall(&conn, "Orthos", 7), None);
    }
}
