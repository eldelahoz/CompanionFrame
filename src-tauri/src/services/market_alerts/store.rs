//! SQLite persistence for rules, dedup keys, owned-count snapshots and the hit history.
//! Tables come from the `market_alerts` migration (`market_alerts`, `market_alert_owned`, `market_alert_seen`,
//! `alert_events`).

use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, params};

use super::rules::{HISTORY_MAX, MAX_RULES, MarketAlertBinding, MarketAlertHit, MarketAlertRule};

const SEEN_TTL_MS: i64 = 7 * 24 * 60 * 60_000;
const SEEN_MAX_PER_RULE: i64 = 500;

/// Rules in creation order. A stored row that no longer validates is skipped, not fatal.
pub fn load(conn: &Connection) -> rusqlite::Result<Vec<(MarketAlertRule, MarketAlertBinding)>> {
    let mut stmt = conn.prepare("SELECT rule, native FROM market_alerts ORDER BY seq")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?;
    let mut out = vec![];
    for row in rows {
        let (json, native) = row?;
        let Ok(mut rule) = serde_json::from_str::<MarketAlertRule>(&json) else {
            continue;
        };
        if rule.validate().is_ok() {
            out.push((rule, MarketAlertBinding { native }));
        }
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: &str) -> rusqlite::Result<Option<MarketAlertRule>> {
    let json: Option<String> = conn
        .query_row("SELECT rule FROM market_alerts WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// Inserts or replaces by id (an empty id gets a fresh one). `Err(String)`: limit or db failure.
pub fn save(
    conn: &Connection,
    mut rule: MarketAlertRule,
    binding: MarketAlertBinding,
) -> Result<MarketAlertRule, String> {
    if rule.id.is_empty() {
        rule.id = uuid::Uuid::new_v4().to_string();
    }
    let err = |e: rusqlite::Error| e.to_string();
    let exists = get(conn, &rule.id).map_err(err)?.is_some();
    if !exists {
        let n: i64 = conn
            .query_row("SELECT count(*) FROM market_alerts", [], |r| r.get(0))
            .map_err(err)?;
        if n >= MAX_RULES as i64 {
            return Err("rule limit reached".into());
        }
    }
    let json = serde_json::to_string(&rule).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO market_alerts (id, rule, native) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET rule = ?2, native = ?3",
        params![rule.id, json, binding.native],
    )
    .map_err(err)?;
    Ok(rule)
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> rusqlite::Result<bool> {
    let Some(mut rule) = get(conn, id)? else {
        return Ok(false);
    };
    rule.enabled = enabled;
    let json = serde_json::to_string(&rule).unwrap_or_default();
    conn.execute(
        "UPDATE market_alerts SET rule = ?2 WHERE id = ?1",
        params![id, json],
    )?;
    Ok(true)
}

/// Also drops the rule's dedup keys, and an owned-count snapshot no other rule uses.
pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    let slug = get(conn, id)?.and_then(|r| r.item.map(|i| i.item_url_name));
    let n = conn.execute("DELETE FROM market_alerts WHERE id = ?1", [id])?;
    conn.execute("DELETE FROM market_alert_seen WHERE rule_id = ?1", [id])?;
    if let Some(slug) = slug
        && !load(conn)?
            .iter()
            .any(|(r, _)| r.item.as_ref().is_some_and(|i| i.item_url_name == slug))
    {
        conn.execute("DELETE FROM market_alert_owned WHERE slug = ?1", [slug])?;
    }
    Ok(n > 0)
}

pub fn set_owned(conn: &Connection, slug: &str, count: u32) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO market_alert_owned (slug, count) VALUES (?1, ?2)
         ON CONFLICT(slug) DO UPDATE SET count = ?2",
        params![slug, count],
    )
    .map(drop)
}

pub fn owned(conn: &Connection, slug: &str) -> Option<u32> {
    conn.query_row(
        "SELECT count FROM market_alert_owned WHERE slug = ?1",
        [slug],
        |r| r.get(0),
    )
    .ok()
}

pub fn seen(conn: &Connection, rule_id: &str) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT key FROM market_alert_seen WHERE rule_id = ?1")?;
    stmt.query_map([rule_id], |r| r.get(0))?.collect()
}

/// Marks keys seen, then drops entries past the TTL and the oldest beyond the per-rule cap.
pub fn mark_seen(
    conn: &Connection,
    rule_id: &str,
    keys: &[String],
    now_ms: i64,
) -> rusqlite::Result<()> {
    for k in keys {
        conn.execute(
            "INSERT INTO market_alert_seen (rule_id, key, at) VALUES (?1, ?2, ?3)
             ON CONFLICT(rule_id, key) DO UPDATE SET at = ?3",
            params![rule_id, k, now_ms],
        )?;
    }
    conn.execute(
        "DELETE FROM market_alert_seen WHERE rule_id = ?1 AND at < ?2",
        params![rule_id, now_ms - SEEN_TTL_MS],
    )?;
    conn.execute(
        "DELETE FROM market_alert_seen WHERE rule_id = ?1 AND key NOT IN
         (SELECT key FROM market_alert_seen WHERE rule_id = ?1 ORDER BY at DESC LIMIT ?2)",
        params![rule_id, SEEN_MAX_PER_RULE],
    )?;
    Ok(())
}

pub fn record_hits(conn: &Connection, hits: &[MarketAlertHit]) -> rusqlite::Result<()> {
    for h in hits {
        conn.execute(
            "INSERT INTO alert_events (id, rule_id, at, hit) VALUES (?1, ?2, ?3, ?4)",
            params![
                h.id,
                h.rule_id,
                h.at,
                serde_json::to_string(h).unwrap_or_default()
            ],
        )?;
    }
    conn.execute(
        "DELETE FROM alert_events WHERE seq <= (SELECT max(seq) FROM alert_events) - ?1",
        [HISTORY_MAX as i64],
    )?;
    Ok(())
}

/// Newest first; a row that no longer parses is dropped.
pub fn hits(conn: &Connection) -> rusqlite::Result<Vec<MarketAlertHit>> {
    let mut stmt = conn.prepare("SELECT hit FROM alert_events ORDER BY seq DESC")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = vec![];
    for row in rows {
        if let Ok(h) = serde_json::from_str(&row?) {
            out.push(h);
        }
    }
    Ok(out)
}

pub fn clear_hits(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM alert_events", []).map(drop)
}
