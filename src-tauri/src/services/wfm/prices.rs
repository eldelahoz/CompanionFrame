//! Item prices: the 48 h closed-sales average from `stats_v1`, cached in `kv_cache` with a TTL.
//!
//! PR #59 table, applied to every outcome:
//! - 200 -> cache the price (and the 90-day history riding on the same response).
//! - 400/404 -> cache "no data" (the only negative cache).
//! - 401/403/408/429/5xx/network -> cache nothing; serve the stale entry if there is one, else error.

use crate::services::clock::now_ms;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Emitter, Manager, State};

use super::catalog::path_slug;
use super::client::WfmClient;
use super::errors::WfmError;
use super::scheduler::{Priority, Scheduler};
use super::session::Session;
use super::stats_history::{self, MarketStatPoint, MergeMode};
use super::stats_v1;
use crate::db::Db;
use crate::services::cache::{kv_get, kv_get_many, kv_put, kv_read_totals};

/// Fresh for interactive lookups (Market, history): a trader wants the live number.
const TTL_MS: i64 = 5 * 60 * 1000;
/// Fresh for inventory valuation: a 48 h average barely moves in 6 h, so a restart within it costs no request.
const BULK_FRESH_MS: i64 = 6 * 60 * 60 * 1000;
/// Past this many failures in a row a background refresh stops; the next bulk call schedules it again.
const REFRESH_MAX_FAILS: u32 = 3;
pub const REFRESHED_EVENT: &str = "wfm-prices-refreshed";
const BULK_MAX: usize = 500;
const BULK_PARALLEL: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PriceStatus {
    Ok,
    /// The item has no statistics (400/404) or no closed sale in the last 48 h.
    NoData,
    /// Transient failure with nothing cached to fall back to.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PriceEntry {
    pub slug: String,
    pub status: PriceStatus,
    pub price: Option<u32>,
    /// The same statistics at the item's highest mod rank; `None` for unranked items or without a sale there.
    pub max_price: Option<u32>,
    /// The rank `max_price` is for.
    pub max_rank: Option<u32>,
    /// Served from a cache entry past its freshness window (a refresh is under way or WFM was unreachable).
    pub stale: bool,
    /// When the price was fetched (ms since epoch); `None` when nothing was cached.
    pub updated_at: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct Cached {
    status: PriceStatus,
    price: Option<u32>,
    /// Absent in entries cached before rank-aware prices: read as `None` until they expire.
    #[serde(default)]
    max_price: Option<u32>,
    #[serde(default)]
    max_rank: Option<u32>,
    ts: i64,
    /// Schema version; entries from before `max_price` read as 0 and are revalidated first.
    #[serde(default)]
    v: u8,
}

const CACHE_V: u8 = 2;

impl Cached {
    fn no_data(ts: i64) -> Self {
        Self {
            status: PriceStatus::NoData,
            price: None,
            max_price: None,
            max_rank: None,
            ts,
            v: CACHE_V,
        }
    }
}

fn key(slug: &str) -> String {
    format!("wfm:price:{slug}")
}

fn parse(raw: &str) -> Option<Cached> {
    let c: Cached = serde_json::from_str(raw).ok()?;
    matches!(c.status, PriceStatus::Ok | PriceStatus::NoData).then_some(c)
}

fn read(db: &Db, slug: &str) -> Option<Cached> {
    parse(&kv_get(db, &key(slug)).ok()??)
}

fn write(db: &Db, slug: &str, c: &Cached) {
    if let Err(e) = serde_json::to_string(c)
        .map_err(|e| e.to_string())
        .and_then(|j| kv_put(db, &key(slug), &j).map_err(|e| e.to_string()))
    {
        tracing::warn!(error = %e, slug, "wfm price cache write failed");
    }
}

fn entry(slug: &str, c: &Cached, stale: bool) -> PriceEntry {
    PriceEntry {
        slug: slug.into(),
        status: c.status,
        price: c.price,
        max_price: c.max_price,
        max_rank: c.max_rank,
        stale,
        updated_at: Some(c.ts as f64),
    }
}

/// `"Ash Prime Set"` -> `ash_prime_set`; anything that could change the URL route is refused.
pub fn normalize_slug(input: &str) -> Result<String, WfmError> {
    let slug = input.split_whitespace().collect::<Vec<_>>().join("_");
    path_slug(&slug.to_lowercase()).map(str::to_owned)
}

#[derive(Default)]
pub struct Prices {
    sched: Scheduler,
    /// One fetch per slug at a time: the second caller finds the first one's cache entry.
    flights: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Slugs a background refresh already covers, so overlapping bulk calls don't queue them twice.
    refreshing: Mutex<HashSet<String>>,
}

impl Prices {
    /// Cached or freshly fetched price. `Err`: transient failure with nothing cached to serve.
    pub async fn price(
        &self,
        client: &WfmClient,
        db: &Db,
        slug: &str,
        priority: Priority,
    ) -> Result<PriceEntry, WfmError> {
        self.price_within(client, db, slug, priority, TTL_MS).await
    }

    /// `price` with the freshness window as a parameter (0 forces a fetch).
    async fn price_within(
        &self,
        client: &WfmClient,
        db: &Db,
        slug: &str,
        priority: Priority,
        ttl_ms: i64,
    ) -> Result<PriceEntry, WfmError> {
        let slug = normalize_slug(slug)?;
        let fresh = |c: &Cached| now_ms() - c.ts < ttl_ms;
        if let Some(c) = read(db, &slug).filter(fresh) {
            return Ok(entry(&slug, &c, false));
        }
        let flight = self
            .flights
            .lock()
            .unwrap()
            .entry(slug.clone())
            .or_default()
            .clone();
        let result = async {
            let _one = flight.lock().await;
            if let Some(c) = read(db, &slug).filter(fresh) {
                return Ok(entry(&slug, &c, false));
            }
            let fetched = self
                .sched
                .run(priority, || stats_v1::fetch(client, &slug))
                .await;
            self.settle(db, &slug, fetched)
        }
        .await;
        let mut flights = self.flights.lock().unwrap();
        // The map and this call are the only holders left: nobody is waiting on the slug.
        if Arc::strong_count(&flight) <= 2 {
            flights.remove(&slug);
        }
        result
    }

    fn settle(
        &self,
        db: &Db,
        slug: &str,
        fetched: Result<Option<Value>, WfmError>,
    ) -> Result<PriceEntry, WfmError> {
        let now = now_ms();
        match fetched {
            Ok(Some(body)) => {
                let history = stats_history::points_from_payload(&body, now as f64);
                if let Err(e) = stats_history::record(db, slug, history, MergeMode::Replace) {
                    tracing::warn!(error = %e, slug, "wfm price history write failed");
                }
                match stats_v1::extract_average(&body, None, now) {
                    Some(avg) => {
                        let max = stats_v1::extract_max_rank_average(&body, now);
                        let c = Cached {
                            status: PriceStatus::Ok,
                            price: Some(avg.average),
                            max_price: max.as_ref().map(|(_, a)| a.average),
                            max_rank: max.map(|(r, _)| r),
                            ts: now,
                            v: CACHE_V,
                        };
                        write(db, slug, &c);
                        Ok(entry(slug, &c, false))
                    }
                    // Statistics exist but nothing sold in 48 h: answer, but do not cache a gap.
                    None => Ok(entry(slug, &Cached::no_data(now), false)),
                }
            }
            Ok(None) => {
                let c = Cached::no_data(now);
                write(db, slug, &c);
                Ok(entry(slug, &c, false))
            }
            // Transient or unexpected: never cached, the old answer (if any) stays.
            Err(e) => read(db, slug).map(|c| entry(slug, &c, true)).ok_or(e),
        }
    }

    /// Daily history for one item. Whatever is stored comes back at once; the second value lists the slug when
    /// its price entry is past `BULK_FRESH_MS` (the daily medians barely move in 6 h) and no refresh covers it yet:
    /// hand it to `refresh`. With nothing stored the call waits on WFM; only a failure there surfaces the error.
    pub async fn history(
        &self,
        client: &WfmClient,
        db: &Db,
        slug: &str,
    ) -> Result<(Vec<MarketStatPoint>, Vec<String>), WfmError> {
        let slug = normalize_slug(slug)?;
        let stored = stats_history::load(db, &slug).map_err(WfmError::invalid)?;
        if stored.is_empty() {
            let refreshed = self.price(client, db, &slug, Priority::Interactive).await;
            let stored = stats_history::load(db, &slug).map_err(WfmError::invalid)?;
            return match refreshed {
                Err(e) if stored.is_empty() => Err(e),
                _ => Ok((stored, Vec::new())),
            };
        }
        let old = read(db, &slug).is_none_or(|c| c.v < CACHE_V || now_ms() - c.ts >= BULK_FRESH_MS);
        let todo = if old && self.refreshing.lock().unwrap().insert(slug.clone()) {
            vec![slug]
        } else {
            Vec::new()
        };
        Ok((stored, todo))
    }

    /// Cached prices come back at once whatever their age; only slugs with nothing cached wait on WFM.
    /// The second value lists the cached ones past `BULK_FRESH_MS` not already being refreshed: hand them to `refresh`.
    pub async fn bulk(
        &self,
        client: &WfmClient,
        db: &Db,
        slugs: Vec<String>,
    ) -> Result<(Vec<PriceEntry>, Vec<String>), WfmError> {
        if slugs.len() > BULK_MAX {
            return Err(WfmError::invalid(format!(
                "at most {BULK_MAX} slugs per call"
            )));
        }
        let mut seen = HashSet::new();
        let slugs: Vec<_> = slugs
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .collect();
        let (t, kv0) = (Instant::now(), kv_read_totals());
        let requested = slugs.len();
        let fetched = AtomicUsize::new(0);
        let mut stale = Vec::new();
        // One batched read for every slug; a failed read just means each one is fetched as if uncached.
        let keys: Vec<_> = slugs
            .iter()
            .filter_map(|s| normalize_slug(s).ok().map(|n| key(&n)))
            .collect();
        let stored = kv_get_many(db, &keys).unwrap_or_default();
        let out = futures_util::stream::iter(slugs)
            .map(|s| async {
                let norm = normalize_slug(&s).ok();
                let hit = norm.and_then(|n| {
                    let c = parse(stored.get(&key(&n))?)?;
                    Some((n, c))
                });
                if let Some((n, c)) = hit {
                    // 2: pre-rank-pricing entry (no max price), revalidated first; 1: merely old.
                    let old = if c.v < CACHE_V {
                        2
                    } else {
                        u8::from(now_ms() - c.ts >= BULK_FRESH_MS)
                    };
                    return (entry(&n, &c, old > 0), old);
                }
                fetched.fetch_add(1, Ordering::Relaxed);
                let e = self
                    .price(client, db, &s, Priority::Background)
                    .await
                    .unwrap_or(PriceEntry {
                        slug: s,
                        status: PriceStatus::Failed,
                        price: None,
                        max_price: None,
                        max_rank: None,
                        stale: false,
                        updated_at: None,
                    });
                (e, 0)
            })
            .buffered(BULK_PARALLEL)
            .collect::<Vec<_>>()
            .await;
        let mut refreshing = self.refreshing.lock().unwrap();
        for want in [2, 1] {
            for (e, old) in &out {
                if *old == want && refreshing.insert(e.slug.clone()) {
                    stale.push(e.slug.clone());
                }
            }
        }
        let kv = kv_read_totals();
        let fetched = fetched.into_inner();
        tracing::info!(
            "timing: wfm_prices_bulk slugs={requested} cached={} fetched={fetched} stale={} queued={} kv_reads={} kv_read_ms={} ms={}",
            out.len() - fetched,
            out.iter().filter(|(_, old)| *old > 0).count(),
            stale.len(),
            kv.0 - kv0.0,
            (kv.1 - kv0.1) / 1000,
            t.elapsed().as_millis()
        );
        Ok((out.into_iter().map(|(e, _)| e).collect(), stale))
    }

    /// Re-fetches `slugs` one at a time at background priority (the scheduler paces and backs off), passing every
    /// batch of landed prices to `on_batch`. A failure keeps the old cached value; repeated failures end the run.
    pub async fn refresh(
        &self,
        client: &WfmClient,
        db: &Db,
        slugs: Vec<String>,
        on_batch: impl Fn(Vec<PriceEntry>),
    ) {
        let (mut landed, mut fails) = (Vec::new(), 0);
        let (t, mut done) = (Instant::now(), 0);
        for slug in &slugs {
            done += 1;
            match self
                .price_within(client, db, slug, Priority::Background, 0)
                .await
            {
                Ok(e) if !e.stale => {
                    fails = 0;
                    landed.push(e);
                }
                _ => fails += 1,
            }
            if landed.len() >= 20 {
                on_batch(std::mem::take(&mut landed));
            }
            if fails >= REFRESH_MAX_FAILS {
                break;
            }
        }
        if !landed.is_empty() {
            on_batch(landed);
        }
        tracing::info!(
            "timing: wfm_prices_refresh queued={} done={done} ms={}",
            slugs.len(),
            t.elapsed().as_millis()
        );
        let mut refreshing = self.refreshing.lock().unwrap();
        slugs.iter().for_each(|s| {
            refreshing.remove(s);
        });
    }
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_price(
    session: State<'_, Session>,
    db: State<'_, Db>,
    prices: State<'_, Prices>,
    slug: String,
) -> Result<PriceEntry, WfmError> {
    prices
        .price(session.client(), &db, &slug, Priority::Interactive)
        .await
}

/// Prices for many slugs (the inventory view). A slug that fails comes back `failed`, not as an error.
#[tauri::command]
#[specta::specta]
pub async fn wfm_prices_bulk(
    app: tauri::AppHandle,
    session: State<'_, Session>,
    db: State<'_, Db>,
    prices: State<'_, Prices>,
    slugs: Vec<String>,
) -> Result<Vec<PriceEntry>, WfmError> {
    let (out, stale) = prices.bulk(session.client(), &db, slugs).await?;
    spawn_refresh(app, stale);
    Ok(out)
}

/// Background refresh of `stale` slugs; landed prices reach the UI through `REFRESHED_EVENT`.
fn spawn_refresh(app: tauri::AppHandle, stale: Vec<String>) {
    if stale.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let (session, db, prices) = (
            app.state::<Session>(),
            app.state::<Db>(),
            app.state::<Prices>(),
        );
        prices
            .refresh(session.client(), &db, stale, |batch| {
                let _ = app.emit(REFRESHED_EVENT, batch);
            })
            .await;
    });
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_stats_history(
    app: tauri::AppHandle,
    session: State<'_, Session>,
    db: State<'_, Db>,
    prices: State<'_, Prices>,
    slug: String,
) -> Result<Vec<MarketStatPoint>, WfmError> {
    let (points, stale) = prices.history(session.client(), &db, &slug).await?;
    spawn_refresh(app, stale);
    Ok(points)
}

#[cfg(test)]
mod tests {
    use crate::db::mem_db;

    use super::super::mock::{Mock, reply, spawn};
    use super::*;

    fn client(m: &Mock) -> WfmClient {
        WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap()
    }

    fn stats_body(price: f64) -> String {
        let t = chrono::DateTime::from_timestamp_millis(now_ms() - 3_600_000)
            .unwrap()
            .to_rfc3339();
        let day = chrono::DateTime::from_timestamp_millis(now_ms() - 5 * 86_400_000)
            .unwrap()
            .to_rfc3339();
        format!(
            r#"{{"payload":{{"statistics_closed":{{
              "48hours":[{{"datetime":"{t}","wa_price":{price},"volume":2,"order_type":"sell"}}],
              "90days":[{{"datetime":"{day}","median":{price},"volume":9}}]}}}}}}"#
        )
    }

    fn hits(m: &Mock) -> usize {
        m.requests()
            .iter()
            .filter(|r| r.starts_with("get /v1/items/"))
            .count()
    }

    fn expire(db: &Db, slug: &str) {
        let mut c = read(db, slug).unwrap();
        c.ts -= TTL_MS + 1;
        write(db, slug, &c);
    }

    async fn price(p: &Prices, c: &WfmClient, db: &Db, slug: &str) -> Result<PriceEntry, WfmError> {
        p.price(c, db, slug, Priority::Interactive).await
    }

    #[tokio::test]
    async fn fetches_caches_and_records_history() {
        let m = spawn(vec![reply(200, &stats_body(64.0))]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        let e = price(&p, &c, &db, "Ash Prime Set").await.unwrap();
        assert_eq!(
            (e.slug.as_str(), e.status, e.price),
            ("ash_prime_set", PriceStatus::Ok, Some(64))
        );
        assert!(m.requests()[0].starts_with("get /v1/items/ash_prime_set/statistics "));
        // Within the TTL the second call is served locally.
        assert_eq!(
            price(&p, &c, &db, "ash_prime_set").await.unwrap().price,
            Some(64)
        );
        assert_eq!(hits(&m), 1);
        assert_eq!(stats_history::load(&db, "ash_prime_set").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn one_response_prices_unranked_and_max_rank() {
        let t = chrono::DateTime::from_timestamp_millis(now_ms() - 3_600_000)
            .unwrap()
            .to_rfc3339();
        let body = format!(
            r#"{{"payload":{{"statistics_closed":{{"48hours":[
              {{"datetime":"{t}","wa_price":4.0,"volume":3,"order_type":"sell","mod_rank":0}},
              {{"datetime":"{t}","wa_price":30.0,"volume":2,"order_type":"sell","mod_rank":10}}]}}}}}}"#
        );
        let m = spawn(vec![reply(200, &body)]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        let e = price(&p, &c, &db, "adaptation").await.unwrap();
        assert_eq!(
            (e.price, e.max_price, e.max_rank),
            (Some(4), Some(30), Some(10))
        );
        // The cached entry carries both buckets: no second request.
        let e = price(&p, &c, &db, "adaptation").await.unwrap();
        assert_eq!(
            (e.price, e.max_price, e.max_rank),
            (Some(4), Some(30), Some(10))
        );
        assert_eq!(hits(&m), 1);
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_request() {
        let m = spawn(vec![reply(200, &stats_body(64.0))]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        let (a, b) = tokio::join!(
            price(&p, &c, &db, "serration"),
            price(&p, &c, &db, "serration")
        );
        assert_eq!((a.unwrap().price, b.unwrap().price), (Some(64), Some(64)));
        assert_eq!(hits(&m), 1);
        assert!(p.flights.lock().unwrap().is_empty());
    }

    /// The #59 truth table.
    #[tokio::test]
    async fn only_400_and_404_are_no_data_and_cached() {
        for status in [400, 404] {
            let m = spawn(vec![reply(status, "{}")]);
            let (c, db, p) = (client(&m), mem_db(), Prices::default());
            let e = price(&p, &c, &db, "x").await.unwrap();
            assert_eq!((e.status, e.price), (PriceStatus::NoData, None), "{status}");
            price(&p, &c, &db, "x").await.unwrap();
            assert_eq!(hits(&m), 1, "{status}: negative cache serves the repeat");
        }
    }

    #[tokio::test]
    async fn transient_statuses_are_errors_and_leave_no_negative_cache() {
        for status in [401u16, 403, 408, 429, 500, 502, 503] {
            let m = spawn(vec![reply(status, "{}"), reply(200, &stats_body(70.0))]);
            let (c, db, p) = (client(&m), mem_db(), Prices::default());
            assert!(price(&p, &c, &db, "x").await.is_err(), "{status}");
            assert!(read(&db, "x").is_none(), "{status}: nothing cached");
            // The very next call goes back to WFM and recovers.
            let p2 = Prices::default();
            assert_eq!(
                price(&p2, &c, &db, "x").await.unwrap().price,
                Some(70),
                "{status}"
            );
        }
    }

    #[tokio::test]
    async fn transient_failure_keeps_serving_the_stale_price() {
        let m = spawn(vec![reply(200, &stats_body(64.0)), reply(503, "{}")]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        price(&p, &c, &db, "x").await.unwrap();
        expire(&db, "x");
        let e = price(&p, &c, &db, "x").await.unwrap();
        assert_eq!((e.price, e.stale), (Some(64), true));
        assert_eq!(hits(&m), 2);
        // The failure did not refresh the entry: it is still expired and still real.
        assert!(now_ms() - read(&db, "x").unwrap().ts >= TTL_MS);
    }

    #[tokio::test]
    async fn a_404_after_a_price_replaces_it_with_no_data() {
        let m = spawn(vec![reply(200, &stats_body(64.0)), reply(404, "{}")]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        price(&p, &c, &db, "x").await.unwrap();
        expire(&db, "x");
        let e = price(&p, &c, &db, "x").await.unwrap();
        assert_eq!((e.status, e.stale), (PriceStatus::NoData, false));
    }

    #[tokio::test]
    async fn statistics_without_a_recent_sale_are_no_data_but_not_cached() {
        let body = r#"{"payload":{"statistics_closed":{"48hours":[],"90days":[]}}}"#;
        let m = spawn(vec![reply(200, body), reply(200, &stats_body(64.0))]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        assert_eq!(
            price(&p, &c, &db, "x").await.unwrap().status,
            PriceStatus::NoData
        );
        assert!(read(&db, "x").is_none());
        assert_eq!(price(&p, &c, &db, "x").await.unwrap().price, Some(64));
    }

    #[tokio::test]
    async fn refuses_slugs_that_change_the_route() {
        let m = spawn(vec![]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        for bad in ["", "../x", "a/b", "a?b", "a#b", "a%2fb"] {
            assert!(price(&p, &c, &db, bad).await.is_err(), "{bad:?}");
        }
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn bulk_dedupes_and_reports_failures_per_slug() {
        let m = spawn(vec![reply(200, &stats_body(64.0)), reply(503, "{}")]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        let (out, _) = p
            .bulk(&c, &db, vec!["a".into(), "a".into(), "b".into()])
            .await
            .unwrap();
        assert_eq!(out.len(), 2);
        // Which request got the 200 is a race; each slug still gets its own verdict.
        let ok = out.iter().filter(|e| e.status == PriceStatus::Ok).count();
        let failed = out
            .iter()
            .filter(|e| e.status == PriceStatus::Failed)
            .count();
        assert_eq!((ok, failed), (1, 1));
        assert!(
            p.bulk(&c, &db, vec!["s".into(); BULK_MAX + 1])
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn history_without_a_store_waits_on_wfm_and_surfaces_an_outage() {
        let m = spawn(vec![reply(200, &stats_body(64.0)), reply(503, "{}")]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        let (points, todo) = p.history(&c, &db, "x").await.unwrap();
        assert_eq!((points.len(), todo.len(), hits(&m)), (1, 0, 1));
        // Nothing stored and WFM down: the error is the answer.
        assert!(p.history(&c, &db, "never").await.is_err());
    }

    fn age(db: &Db, slug: &str, ms: i64, v: u8) {
        let mut c = read(db, slug).unwrap();
        c.ts -= ms;
        c.v = v;
        write(db, slug, &c);
    }

    async fn bulk(
        p: &Prices,
        c: &WfmClient,
        db: &Db,
        s: &[&str],
    ) -> (Vec<PriceEntry>, Vec<String>) {
        p.bulk(c, db, s.iter().map(|s| s.to_string()).collect())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn bulk_serves_a_stale_entry_at_once_and_schedules_a_refresh() {
        let m = spawn(vec![
            reply(200, &stats_body(64.0)),
            reply(200, &stats_body(80.0)),
        ]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        price(&p, &c, &db, "x").await.unwrap();
        age(&db, "x", BULK_FRESH_MS + 1, CACHE_V);
        let (out, todo) = bulk(&p, &c, &db, &["x"]).await;
        assert_eq!(
            (out[0].price, out[0].stale, todo.clone()),
            (Some(64), true, vec!["x".to_string()])
        );
        assert_eq!(hits(&m), 1, "the response did not wait on the network");
        // The same slug is not scheduled twice while its refresh is pending.
        assert!(bulk(&p, &c, &db, &["x"]).await.1.is_empty());
        let landed = std::sync::Mutex::new(Vec::new());
        p.refresh(&c, &db, todo, |b| landed.lock().unwrap().extend(b))
            .await;
        assert_eq!(landed.lock().unwrap()[0].price, Some(80));
        assert_eq!(bulk(&p, &c, &db, &["x"]).await.0[0].price, Some(80));
    }

    #[tokio::test]
    async fn bulk_leaves_fresh_entries_alone_and_fetches_missing_ones() {
        let m = spawn(vec![
            reply(200, &stats_body(64.0)),
            reply(200, &stats_body(70.0)),
        ]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        price(&p, &c, &db, "x").await.unwrap();
        age(&db, "x", BULK_FRESH_MS - 60_000, CACHE_V); // past the 5 min TTL, inside the 6 h window
        let (out, todo) = bulk(&p, &c, &db, &["x", "y"]).await;
        assert!(todo.is_empty());
        assert_eq!((out[0].stale, out[1].price), (false, Some(70)));
        assert_eq!(hits(&m), 2);
    }

    #[tokio::test]
    async fn pre_v2_entries_are_revalidated_first_and_a_failed_refresh_keeps_the_old_price() {
        let m = spawn(vec![
            reply(200, &stats_body(64.0)),
            reply(200, &stats_body(64.0)),
            reply(200, &stats_body(65.0)),
            reply(503, "{}"),
        ]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        price(&p, &c, &db, "old").await.unwrap();
        price(&p, &c, &db, "new").await.unwrap();
        age(&db, "new", BULK_FRESH_MS + 1, CACHE_V);
        age(&db, "old", 0, 0); // fresh by age, but cached before max-rank prices
        let (_, todo) = bulk(&p, &c, &db, &["new", "old"]).await;
        assert_eq!(todo, ["old", "new"]);
        let landed = std::sync::Mutex::new(Vec::new());
        p.refresh(&c, &db, todo, |b| landed.lock().unwrap().extend(b))
            .await;
        // `old` landed; `new` hit the 503 and still serves its cached value.
        assert_eq!(landed.lock().unwrap().len(), 1);
        assert_eq!(read(&db, "old").unwrap().price, Some(65));
        assert_eq!(read(&db, "new").unwrap().price, Some(64));
        assert!(p.refreshing.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn history_with_a_store_answers_at_once_and_schedules_one_refresh() {
        let m = spawn(vec![
            reply(200, &stats_body(64.0)),
            reply(200, &stats_body(80.0)),
        ]);
        let (c, db, p) = (client(&m), mem_db(), Prices::default());
        price(&p, &c, &db, "x").await.unwrap();
        // Price cache past the 5 min TTL but inside 6 h: nothing to do.
        age(&db, "x", BULK_FRESH_MS - 60_000, CACHE_V);
        let (points, todo) = p.history(&c, &db, "x").await.unwrap();
        assert_eq!((points.len(), todo.len(), hits(&m)), (1, 0, 1));
        // Past 6 h: stored history comes back without touching the network, refresh queued once.
        age(&db, "x", 120_000, CACHE_V);
        let (points, todo) = p.history(&c, &db, "x").await.unwrap();
        assert_eq!(
            (points.len(), todo.clone(), hits(&m)),
            (1, vec!["x".to_string()], 1)
        );
        assert!(p.history(&c, &db, "x").await.unwrap().1.is_empty());
        let landed = std::sync::Mutex::new(Vec::new());
        p.refresh(&c, &db, todo, |b| landed.lock().unwrap().extend(b))
            .await;
        assert_eq!((landed.lock().unwrap()[0].price, hits(&m)), (Some(80), 2));
        assert!(p.history(&c, &db, "x").await.unwrap().1.is_empty());
    }
}
