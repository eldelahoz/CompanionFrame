//! Daily price history per item (port of WFHelper's `marketStats` / `marketStatsHistory`), kept in
//! SQLite `price_history`. Rows come from the 90-day closed statistics of `stats_v1`.

use std::collections::BTreeMap;

use chrono::DateTime;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::stats_v1::{num, rows, time_ms};
use crate::db::Db;

/// A year of every mod rank warframe.market reports per day fits well inside this.
pub const MAX_POINTS: usize = 8000;
const MAX_DAYS: f64 = 365.0;
const DAY_MS: f64 = 86_400_000.0;
const MAX_RANK: u32 = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StatSource {
    Wfm,
    /// Rows from a backend price archive; nothing produces them until one is configured.
    Archive,
}

impl StatSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Wfm => "wfm",
            Self::Archive => "archive",
        }
    }
}

/// One closed-sales day of warframe.market statistics, as the Browse chart plots it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketStatPoint {
    pub source: StatSource,
    /// Epoch ms of the day (UTC).
    pub time: f64,
    pub volume: Option<f64>,
    pub median: f64,
    pub moving_avg: Option<f64>,
    pub avg_price: Option<f64>,
    pub open_price: Option<f64>,
    pub closed_price: Option<f64>,
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub donch_top: Option<f64>,
    pub donch_bot: Option<f64>,
    pub rank: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MergeMode {
    Replace,
    /// Refreshes archive rows and missing days while preserving WFM measurements.
    Fill,
}

fn day_key(time_ms: f64) -> String {
    DateTime::from_timestamp_millis(time_ms as i64)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Rejects what `normalizeMarketStatPoint` rejects: bad times, ranks and negative prices.
pub fn is_valid(p: &MarketStatPoint, now_ms: f64) -> bool {
    let finite = |v: f64| v.is_finite();
    let price = |v: Option<f64>| v.is_none_or(|n| finite(n) && n >= 0.0);
    finite(p.median)
        && p.median >= 0.0
        && finite(p.time)
        && p.time > 0.0
        && p.time <= now_ms + DAY_MS
        && p.rank.is_none_or(|r| r <= MAX_RANK)
        && p.moving_avg.is_none_or(finite)
        && [
            p.volume,
            p.avg_price,
            p.open_price,
            p.closed_price,
            p.min_price,
            p.max_price,
            p.donch_top,
            p.donch_bot,
        ]
        .into_iter()
        .all(price)
}

/// Merges fresh rows into the stored ones; the result keeps a year from its newest day.
pub fn merge(
    stored: Vec<MarketStatPoint>,
    fresh: Vec<MarketStatPoint>,
    mode: MergeMode,
) -> Vec<MarketStatPoint> {
    let key = |p: &MarketStatPoint| (day_key(p.time), p.rank.map_or(-1, |r| r as i32));
    let mut by_key: BTreeMap<_, _> = stored.into_iter().map(|p| (key(&p), p)).collect();
    for p in fresh {
        let k = key(&p);
        if mode == MergeMode::Fill && by_key.get(&k).is_some_and(|s| s.source == StatSource::Wfm) {
            continue;
        }
        by_key.insert(k, p);
    }
    let mut merged: Vec<_> = by_key.into_values().collect();
    merged.sort_by(|a, b| a.time.total_cmp(&b.time));
    let newest = merged.last().map_or(0.0, |p| p.time);
    merged.retain(|p| p.time >= newest - MAX_DAYS * DAY_MS);
    if merged.len() > MAX_POINTS {
        merged.drain(..merged.len() - MAX_POINTS);
    }
    merged
}

/// The 90-day closed rows of a statistics response as WFM points; malformed rows are dropped.
pub fn points_from_payload(body: &Value, now_ms: f64) -> Vec<MarketStatPoint> {
    let closed = body.pointer("/payload/statistics_closed");
    rows(closed, &["90days", "90_days"])
        .iter()
        .filter_map(|r| {
            let field = |k: &str| r.get(k).and_then(num);
            let rank = match r.get("mod_rank").filter(|v| !v.is_null()) {
                None => None,
                Some(v) => Some(u32::try_from(num(v)? as i64).ok()?),
            };
            let p = MarketStatPoint {
                source: StatSource::Wfm,
                time: time_ms(r)? as f64,
                volume: field("volume"),
                median: field("median")?,
                moving_avg: field("moving_avg"),
                avg_price: field("avg_price"),
                open_price: field("open_price"),
                closed_price: field("closed_price"),
                min_price: field("min_price"),
                max_price: field("max_price"),
                donch_top: field("donch_top"),
                donch_bot: field("donch_bot"),
                rank,
            };
            is_valid(&p, now_ms).then_some(p)
        })
        .collect()
}

pub fn load(db: &Db, slug: &str) -> rusqlite::Result<Vec<MarketStatPoint>> {
    let conn = db.read();
    let mut stmt = conn.prepare(
        "SELECT source, time, volume, median, moving_avg, avg_price, open_price, closed_price,
                min_price, max_price, donch_top, donch_bot, rank
         FROM price_history WHERE slug = ?1 ORDER BY time, rank",
    )?;
    stmt.query_map([slug], |r| {
        let rank: i64 = r.get(12)?;
        Ok(MarketStatPoint {
            source: if r.get::<_, String>(0)? == "archive" {
                StatSource::Archive
            } else {
                StatSource::Wfm
            },
            time: r.get(1)?,
            volume: r.get(2)?,
            median: r.get(3)?,
            moving_avg: r.get(4)?,
            avg_price: r.get(5)?,
            open_price: r.get(6)?,
            closed_price: r.get(7)?,
            min_price: r.get(8)?,
            max_price: r.get(9)?,
            donch_top: r.get(10)?,
            donch_bot: r.get(11)?,
            rank: u32::try_from(rank).ok(),
        })
    })?
    .collect()
}

/// Merges `fresh` into what is stored for `slug` and rewrites it. Nothing fresh: nothing written.
pub fn record(
    db: &Db,
    slug: &str,
    fresh: Vec<MarketStatPoint>,
    mode: MergeMode,
) -> rusqlite::Result<Vec<MarketStatPoint>> {
    let stored = load(db, slug)?;
    if fresh.is_empty() {
        return Ok(stored);
    }
    let merged = merge(stored, fresh, mode);
    let mut conn = db.lock();
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM price_history WHERE slug = ?1", [slug])?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO price_history (slug, day, rank, source, time, volume, median, moving_avg,
                avg_price, open_price, closed_price, min_price, max_price, donch_top, donch_bot)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        )?;
        for p in &merged {
            ins.execute(params![
                slug,
                day_key(p.time),
                p.rank.map_or(-1, i64::from),
                p.source.as_str(),
                p.time,
                p.volume,
                p.median,
                p.moving_avg,
                p.avg_price,
                p.open_price,
                p.closed_price,
                p.min_price,
                p.max_price,
                p.donch_top,
                p.donch_bot
            ])?;
        }
    }
    tx.commit()?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use crate::db::mem_db;

    use serde_json::json;

    use super::*;

    const NOW: f64 = 1_792_900_000_000.0;

    fn pt(days_ago: f64, median: f64, source: StatSource, rank: Option<u32>) -> MarketStatPoint {
        MarketStatPoint {
            source,
            time: (NOW / DAY_MS).floor() * DAY_MS - days_ago * DAY_MS,
            volume: Some(3.0),
            median,
            moving_avg: None,
            avg_price: None,
            open_price: None,
            closed_price: None,
            min_price: None,
            max_price: None,
            donch_top: None,
            donch_bot: None,
            rank,
        }
    }
    use StatSource::{Archive, Wfm};

    #[test]
    fn replace_overwrites_the_same_day_and_rank_only() {
        let stored = vec![pt(2.0, 10.0, Wfm, None), pt(1.0, 11.0, Wfm, Some(0))];
        let fresh = vec![pt(2.0, 99.0, Archive, None), pt(1.0, 12.0, Wfm, Some(5))];
        let m = merge(stored, fresh, MergeMode::Replace);
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].median, 99.0);
        assert_eq!(m[0].source, Archive);
    }

    #[test]
    fn fill_keeps_wfm_rows_but_refreshes_archive_and_adds_missing_days() {
        let stored = vec![pt(3.0, 10.0, Wfm, None), pt(2.0, 11.0, Archive, None)];
        let fresh = vec![
            pt(3.0, 99.0, Archive, None),
            pt(2.0, 98.0, Archive, None),
            pt(1.0, 97.0, Archive, None),
        ];
        let m = merge(stored, fresh, MergeMode::Fill);
        assert_eq!(
            m.iter().map(|p| p.median).collect::<Vec<_>>(),
            [10.0, 98.0, 97.0]
        );
    }

    #[test]
    fn keeps_a_year_from_the_newest_day_and_caps_the_count() {
        let m = merge(
            vec![pt(400.0, 1.0, Wfm, None), pt(364.0, 2.0, Wfm, None)],
            vec![pt(0.0, 3.0, Wfm, None)],
            MergeMode::Replace,
        );
        assert_eq!(m.iter().map(|p| p.median).collect::<Vec<_>>(), [2.0, 3.0]);
        // 365 days x 21 ranks would pass the cap; the newest survive.
        let many: Vec<_> = (0..MAX_POINTS as u32 + 5)
            .map(|i| pt(0.0, i as f64, Wfm, Some(i % 21)))
            .collect();
        assert!(merge(vec![], many, MergeMode::Replace).len() <= 21);
    }

    #[test]
    fn validation_matches_the_shared_rules() {
        let ok = pt(1.0, 5.0, Wfm, Some(20));
        assert!(is_valid(&ok, NOW));
        assert!(!is_valid(&pt(1.0, -1.0, Wfm, None), NOW), "negative median");
        assert!(
            !is_valid(&pt(1.0, 5.0, Wfm, Some(21)), NOW),
            "rank above 20"
        );
        assert!(
            !is_valid(&pt(-2.0, 5.0, Wfm, None), NOW),
            "future beyond a day"
        );
        let mut p = ok.clone();
        p.volume = Some(f64::NAN);
        assert!(!is_valid(&p, NOW));
        p.volume = Some(-1.0);
        assert!(!is_valid(&p, NOW));
    }

    #[test]
    fn parses_the_90_day_rows_and_drops_malformed_ones() {
        let body = json!({"payload": {"statistics_closed": {"90days": [
            {"datetime": "2026-10-20T00:00:00.000+00:00", "volume": 7, "median": 64.0,
             "moving_avg": 63.5, "avg_price": 64.0, "min_price": 60.0, "max_price": 70.0,
             "open_price": 62.0, "closed_price": 66.0, "donch_top": 70.0, "donch_bot": 60.0,
             "mod_rank": 3},
            {"datetime": "2026-10-21T00:00:00.000+00:00", "median": 50.0},
            {"datetime": "bad", "median": 50.0},
            {"datetime": "2026-10-22T00:00:00.000+00:00"},
            {"datetime": "2026-10-23T00:00:00.000+00:00", "median": 5.0, "mod_rank": 99}
        ]}}});
        let p = points_from_payload(&body, NOW);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].rank, Some(3));
        assert_eq!(p[0].volume, Some(7.0));
        assert_eq!(p[1].rank, None);
        assert_eq!(p[1].volume, None);
        assert!(p.iter().all(|x| x.source == Wfm));
    }

    #[test]
    fn persists_and_merges_across_calls() {
        let db = mem_db();
        assert!(load(&db, "x").unwrap().is_empty());
        record(&db, "x", vec![pt(2.0, 10.0, Wfm, None)], MergeMode::Replace).unwrap();
        let m = record(
            &db,
            "x",
            vec![pt(2.0, 12.0, Wfm, None), pt(1.0, 13.0, Wfm, Some(2))],
            MergeMode::Replace,
        )
        .unwrap();
        assert_eq!(load(&db, "x").unwrap(), m);
        assert_eq!(m.iter().map(|p| p.median).collect::<Vec<_>>(), [12.0, 13.0]);
        assert_eq!(m[1].rank, Some(2));
        assert!(load(&db, "other").unwrap().is_empty());
    }

    #[test]
    fn an_empty_refresh_changes_nothing() {
        let db = mem_db();
        record(&db, "x", vec![pt(1.0, 10.0, Wfm, None)], MergeMode::Replace).unwrap();
        let m = record(&db, "x", vec![], MergeMode::Replace).unwrap();
        assert_eq!(m.len(), 1);
    }
}
