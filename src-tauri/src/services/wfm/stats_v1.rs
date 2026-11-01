//! The one place that talks to the deprecated v1 `/items/{slug}/statistics` (no v2 equivalent:
//! checked 2026-10-25, `/v2/item/{slug}/statistics` and friends all 404). Swap this module when
//! WFM ships one.
//!
//! `extract_average` is the port of WFHelper's `extractAverageFromStatsPayload`: the volume-weighted
//! average of the last 48 h of closed sells, with one fake-close outlier filter.

use std::collections::BTreeMap;

use chrono::DateTime;
use reqwest::Method;
use serde_json::Value;

use super::client::{Api, WfmClient};
use super::errors::{Class, WfmError};

const WINDOW_MS: i64 = 48 * 60 * 60 * 1000;
/// WFM records a close at whatever price the closed order carried, so one fake close can be the
/// whole window. Measured by WFHelper 2026-09-24: 3 of 3213 prices exceeded 10x their lowest ask.
const OUTLIER_FACTOR: f64 = 10.0;

/// `Ok(None)`: 400/404, the item has no statistics. Any other failure is an error, never "none".
pub async fn fetch(client: &WfmClient, slug: &str) -> Result<Option<Value>, WfmError> {
    let path = format!("/items/{slug}/statistics");
    match client.request(Api::V1, Method::GET, &path, None).await {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.class() == Class::NoData => Ok(None),
        Err(e) => Err(e),
    }
}

/// WFHelper's `toFiniteNumber`: a finite number, or a non-empty numeric string.
pub(super) fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        Value::String(s) if !s.trim().is_empty() => {
            s.trim().parse().ok().filter(|f: &f64| f.is_finite())
        }
        _ => None,
    }
}

/// WFHelper's `normalizeRank`: null/blank/negative/NaN -> None, otherwise floored.
fn rank_of(v: &Value) -> Option<u32> {
    num(v).filter(|f| *f >= 0.0).map(|f| f.floor() as u32)
}

/// `None`: the row does not belong to the asked pool. `Some(r)`: it does, with rank `r`.
fn pooled_rank(row: &Value, rank: Option<u32>) -> Option<Option<u32>> {
    let raw = ["mod_rank", "rank"]
        .iter()
        .find_map(|k| row.get(*k).filter(|v| !v.is_null()));
    let row_rank = raw.and_then(rank_of);
    if raw.is_some() && row_rank.is_none() {
        return None;
    }
    let ok = match rank {
        Some(r) => row_rank == Some(r),
        None => matches!(row_rank, None | Some(0)),
    };
    ok.then_some(row_rank)
}

pub(super) fn time_ms(row: &Value) -> Option<i64> {
    let s = row.get("datetime")?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.timestamp_millis())
}

pub(super) fn rows<'a>(group: Option<&'a Value>, windows: &[&str]) -> &'a [Value] {
    windows
        .iter()
        .find_map(|w| group?.get(*w)?.as_array())
        .map_or(&[], Vec::as_slice)
}

fn median_of(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 {
        v[m]
    } else {
        (v[m - 1] + v[m]) / 2.0
    })
}

fn pooled_medians(rows: &[Value], rank: Option<u32>, keep: impl Fn(&Value) -> bool) -> Vec<f64> {
    rows.iter()
        .filter(|r| r.is_object() && keep(r) && pooled_rank(r, rank).is_some())
        .filter_map(|r| num(r.get("median")?))
        .filter(|m| *m > 0.0)
        .collect()
}

/// What an honest trade of this rank fetches: the live sell book, or without one the rank's closed
/// history before the window.
fn reference_price(payload: &Value, rank: Option<u32>, window_start: i64) -> Option<f64> {
    let live = payload.get("statistics_live");
    let sells = |r: &Value| r.get("order_type").and_then(Value::as_str) == Some("sell");
    let book = pooled_medians(rows(live, &["48hours", "48_hours"]), rank, sells);
    if !book.is_empty() {
        return median_of(book);
    }
    let closed = payload.get("statistics_closed");
    median_of(pooled_medians(rows(closed, &["90days"]), rank, |r| {
        let sell = r
            .get("order_type")
            .is_none_or(|t| t.is_null() || t == "sell");
        sell && time_ms(r).is_some_and(|t| t < window_start)
    }))
}

#[derive(Debug, PartialEq)]
pub struct Average {
    pub average: u32,
    pub volume: f64,
}

/// `body` is the raw response (`{"payload": {...}}`). `None`: no usable closed sell in the window.
pub fn extract_average(body: &Value, rank: Option<u32>, now_ms: i64) -> Option<Average> {
    let payload = body.get("payload")?;
    let window = rows(payload.get("statistics_closed"), &["48hours", "48_hours"]);
    let mut samples: BTreeMap<(i64, Option<u32>), (f64, f64)> = BTreeMap::new();
    for row in window.iter().filter(|r| r.is_object()) {
        if row
            .get("order_type")
            .is_some_and(|t| !t.is_null() && t != "sell")
        {
            continue;
        }
        let Some(row_rank) = pooled_rank(row, rank) else {
            continue;
        };
        let (Some(t), Some(price), Some(volume)) = (
            time_ms(row),
            row.get("wa_price").and_then(num),
            row.get("volume").and_then(num),
        ) else {
            continue;
        };
        if t < now_ms - WINDOW_MS || t > now_ms || price <= 0.0 || volume <= 0.0 {
            continue;
        }
        samples.insert((t, row_rank), (price, volume));
    }
    let reference = if samples.is_empty() {
        None
    } else {
        reference_price(payload, rank, now_ms - WINDOW_MS)
    };
    let (mut total, mut volume) = (0.0, 0.0);
    for (price, v) in samples.values() {
        if reference.is_some_and(|r| r > 0.0 && *price > r * OUTLIER_FACTOR) {
            continue;
        }
        // wa_price already weights the trades inside each bucket by quantity.
        total += price * v;
        volume += v;
    }
    let average = (total / volume).round();
    (volume > 0.0 && average.is_finite() && average > 0.0).then_some(Average {
        average: average as u32,
        volume,
    })
}

/// The max-rank pool of the same response: WFM reports a ranked mod or arcane at rank 0 and at its
/// highest rank, so the highest `mod_rank` in the payload is the max. `None`: unranked item, or no
/// usable sale at that rank. Returns the rank with its average.
pub fn extract_max_rank_average(body: &Value, now_ms: i64) -> Option<(u32, Average)> {
    let payload = body.get("payload")?;
    let closed = payload.get("statistics_closed");
    let top = ["48hours", "90days"]
        .iter()
        .flat_map(|w| rows(closed, &[w]))
        .filter_map(|r| rank_of(r.get("mod_rank")?))
        .max()
        .filter(|r| *r > 0)?;
    extract_average(body, Some(top), now_ms).map(|a| (top, a))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_792_900_000_000;
    fn at(hours_ago: i64) -> String {
        DateTime::from_timestamp_millis(NOW - hours_ago * 3_600_000)
            .unwrap()
            .to_rfc3339()
    }
    fn row(h: i64, price: f64, volume: f64) -> Value {
        json!({"datetime": at(h), "wa_price": price, "volume": volume, "order_type": "sell"})
    }
    fn body(closed: Vec<Value>) -> Value {
        json!({"payload": {"statistics_closed": {"48hours": closed}}})
    }

    #[test]
    fn volume_weighted_average_of_the_window() {
        let b = body(vec![
            row(1, 60.0, 1.0),
            row(2, 70.0, 3.0),
            row(60, 500.0, 9.0),
        ]);
        // 60*1 + 70*3 = 270 over 4 = 67.5, rounded; the 60 h-old row is outside the window.
        assert_eq!(
            extract_average(&b, None, NOW),
            Some(Average {
                average: 68,
                volume: 4.0
            })
        );
    }

    #[test]
    fn nothing_usable_is_none() {
        assert_eq!(extract_average(&json!({}), None, NOW), None);
        assert_eq!(extract_average(&body(vec![]), None, NOW), None);
        let bad = body(vec![
            row(1, 0.0, 2.0),
            row(1, 50.0, 0.0),
            row(-1, 50.0, 1.0),
        ]);
        assert_eq!(extract_average(&bad, None, NOW), None);
    }

    #[test]
    fn buy_rows_are_ignored() {
        let mut buy = row(1, 10.0, 5.0);
        buy["order_type"] = json!("buy");
        let b = body(vec![buy, row(2, 80.0, 1.0)]);
        assert_eq!(extract_average(&b, None, NOW).unwrap().average, 80);
    }

    #[test]
    fn rank_pools_never_mix() {
        let ranked = |h, price, rank| {
            let mut r = row(h, price, 1.0);
            r["mod_rank"] = json!(rank);
            r
        };
        let b = body(vec![
            ranked(1, 10.0, 0),
            ranked(2, 40.0, 5),
            row(3, 12.0, 1.0),
        ]);
        // No rank asked: rank 0 and rankless rows only.
        assert_eq!(extract_average(&b, None, NOW).unwrap().average, 11);
        assert_eq!(extract_average(&b, Some(5), NOW).unwrap().average, 40);
        assert_eq!(extract_average(&b, Some(3), NOW), None);
    }

    #[test]
    fn a_fake_close_far_above_the_live_book_is_dropped() {
        let mut b = body(vec![row(1, 50.0, 1.0), row(2, 9000.0, 1.0)]);
        b["payload"]["statistics_live"] =
            json!({"48hours": [{"order_type": "sell", "median": 45.0}]});
        assert_eq!(extract_average(&b, None, NOW).unwrap().average, 50);
        // Without a reference nothing is judged an outlier.
        let lone = body(vec![row(1, 50.0, 1.0), row(2, 9000.0, 1.0)]);
        assert_eq!(extract_average(&lone, None, NOW).unwrap().average, 4525);
    }

    #[test]
    fn reference_falls_back_to_closed_history_before_the_window() {
        let mut b = body(vec![row(1, 50.0, 1.0), row(2, 9000.0, 1.0)]);
        b["payload"]["statistics_closed"]["90days"] =
            json!([{"datetime": at(100), "median": 48.0, "order_type": "sell"}]);
        assert_eq!(extract_average(&b, None, NOW).unwrap().average, 50);
    }

    #[test]
    fn accepts_the_underscored_window_name_and_string_numbers() {
        let b = json!({"payload": {"statistics_closed": {"48_hours": [
            {"datetime": at(1), "wa_price": "70", "volume": "2"}
        ]}}});
        assert_eq!(extract_average(&b, None, NOW).unwrap().average, 70);
    }

    #[test]
    fn max_rank_and_unranked_get_their_own_average_from_one_payload() {
        let ranked = |h: i64, price: f64, rank: u32| json!({"datetime": at(h), "wa_price": price, "volume": 1.0, "order_type": "sell", "mod_rank": rank});
        let b = body(vec![
            ranked(1, 5.0, 0),
            ranked(2, 40.0, 10),
            ranked(3, 6.0, 0),
        ]);
        assert_eq!(extract_average(&b, None, NOW).map(|a| a.average), Some(6));
        assert_eq!(
            extract_max_rank_average(&b, NOW).map(|(r, a)| (r, a.average)),
            Some((10, 40))
        );
        // Unranked items (no mod_rank, or only rank 0) have no max pool.
        assert_eq!(
            extract_max_rank_average(&body(vec![row(1, 60.0, 1.0)]), NOW),
            None
        );
        assert_eq!(
            extract_max_rank_average(&body(vec![ranked(1, 5.0, 0)]), NOW),
            None
        );
    }
}
