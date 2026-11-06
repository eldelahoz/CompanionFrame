//! The alert engine: a periodic tick evaluates due rules against warframe.market (riven auctions on
//! v1, item order books on v2), dedups by listing+price, records hits and notifies once per new
//! listing. Evaluation is the only part that touches the network; it runs under the shared WFM
//! budget (the shared `Scheduler`, background priority) and never holds the DB lock across an await.

use crate::services::clock::now_ms;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::Method;
use serde::Serialize;

use super::rules::{
    Auction, MarketAlertBinding, MarketAlertHit, MarketAlertKind, MarketAlertRule,
    item_display_name, item_hit, match_item_order, match_riven, parse_auctions, riven_hit,
    riven_search_path,
};
use super::store;
use crate::db::Db;
use crate::services::notify::channels::NotificationSource;
use crate::services::notify::{NotificationDispatch, Notifier};
use crate::services::wfm::client::{Api, WfmClient};
use crate::services::wfm::errors::WfmError;
use crate::services::wfm::orders::orders_item;
use crate::services::wfm::scheduler::{self, Priority};

pub const TICK_SECS: u64 = 60;
pub const INITIAL_DELAY_SECS: u64 = 30;
const RULE_EVAL_INTERVAL_MS: i64 = 3 * 60_000;
const MAX_REQUESTS_PER_TICK: usize = 4;
const FAILURE_BASE_MS: i64 = 5 * 60_000;
const FAILURE_CEILING_MS: i64 = 60 * 60_000;
const HOUR_MS: i64 = 60 * 60_000;

/// Everything a tick needs from the app, so tests can hand in a mock client and a fake notifier.
pub struct Ctx<'a> {
    pub client: &'a WfmClient,
    pub db: &'a Db,
    pub notifier: &'a dyn Notifier,
    /// The signed-in user: their own listings never alert them.
    pub own_name: Option<String>,
    pub changed: &'a (dyn Fn() + Sync),
}

impl Ctx<'_> {
    fn is_own(&self, name: &str) -> bool {
        !name.is_empty()
            && self
                .own_name
                .as_deref()
                .is_some_and(|o| o.eq_ignore_ascii_case(name))
    }

    fn db<T>(&self, f: impl FnOnce(&rusqlite::Connection) -> T) -> T {
        f(&self.db.lock())
    }
}

#[derive(Default)]
struct Runtime {
    cooldown_until: HashMap<String, i64>,
    next_eval: HashMap<String, i64>,
    failures: HashMap<String, u32>,
    requests: VecDeque<i64>,
    last_tick_at: Option<String>,
    /// (rule id, message): cleared when that rule next succeeds, is deleted or is switched off.
    last_error: Option<(String, String)>,
}

#[derive(Default)]
pub struct Engine {
    rt: Mutex<Runtime>,
    busy: AtomicBool,
    armed: AtomicBool,
}

struct Outcome {
    hits: Vec<MarketAlertHit>,
    keys: Vec<String>,
    candidates: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketAlertEngineStatus {
    pub running: bool,
    pub rule_count: u32,
    pub enabled_count: u32,
    pub last_tick_at: Option<String>,
    pub requests_last_hour: u32,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketAlertTestFire {
    pub matches: u32,
    pub detail: String,
}

fn seen_key(id: &str, platinum: f64) -> String {
    // The price is in the key so a real price drop on the same listing may fire again.
    format!("{id}:{platinum}")
}

impl Engine {
    pub fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    fn rt(&self) -> std::sync::MutexGuard<'_, Runtime> {
        self.rt.lock().unwrap()
    }

    fn note_request(&self, now: i64) {
        let mut rt = self.rt();
        rt.requests.push_back(now);
        while rt.requests.front().is_some_and(|t| *t <= now - HOUR_MS) {
            rt.requests.pop_front();
        }
    }

    async fn get<T, Fut>(&self, now: i64, f: impl FnOnce() -> Fut) -> Result<T, WfmError>
    where
        Fut: std::future::Future<Output = Result<T, WfmError>>,
    {
        self.note_request(now);
        scheduler::scoped(Priority::Background, f()).await
    }

    async fn evaluate(
        &self,
        cx: &Ctx<'_>,
        rule: &MarketAlertRule,
        skip_dedup: bool,
        now: i64,
    ) -> Result<Outcome, WfmError> {
        let fresh = |keys: &[String]| -> HashSet<String> {
            if skip_dedup {
                return keys.iter().cloned().collect();
            }
            let seen = cx.db(|c| store::seen(c, &rule.id)).unwrap_or_default();
            keys.iter()
                .filter(|k| !seen.contains(*k))
                .cloned()
                .collect()
        };
        if let (MarketAlertKind::Riven, Some(m)) = (rule.kind, &rule.riven) {
            let path = riven_search_path(m);
            let raw = self
                .get(now, || cx.client.request(Api::V1, Method::GET, &path, None))
                .await?;
            let auctions: Vec<Auction> = parse_auctions(&raw)
                .into_iter()
                .filter(|a| !cx.is_own(&a.seller) && match_riven(m, a))
                .collect();
            let keys: Vec<String> = auctions
                .iter()
                .map(|a| seen_key(&a.id, a.platinum))
                .collect();
            let new = fresh(&keys);
            let mut out = Outcome {
                hits: vec![],
                keys: vec![],
                candidates: auctions.len(),
            };
            for (a, k) in auctions.iter().zip(keys) {
                if new.contains(&k) {
                    out.hits.push(riven_hit(rule, a));
                    out.keys.push(k);
                }
            }
            return Ok(out);
        }
        if let (MarketAlertKind::Item, Some(m)) = (rule.kind, &rule.item) {
            let orders = self
                .get(now, || {
                    orders_item(cx.client, &m.item_url_name, false, None, None)
                })
                .await?;
            let owned = cx.db(|c| store::owned(c, &m.item_url_name));
            let orders: Vec<_> = orders
                .into_iter()
                .filter(|o| {
                    let user = o.user.as_ref();
                    // Only crossplay-off sellers on another platform cannot trade with PC.
                    // shortcut: crossplay flag is not modelled by BookUser, so any non-pc seller is skipped.
                    let foreign = user
                        .and_then(|u| u.platform.as_deref())
                        .is_some_and(|p| !p.eq_ignore_ascii_case("pc"));
                    !o.id.is_empty()
                        && o.visible != Some(false)
                        && !foreign
                        && !cx.is_own(user.and_then(|u| u.ingame_name.as_deref()).unwrap_or(""))
                        && match_item_order(m, o, owned)
                })
                .collect();
            let keys: Vec<String> = orders
                .iter()
                .map(|o| seen_key(&o.id, f64::from(o.platinum)))
                .collect();
            let new = fresh(&keys);
            let name = item_display_name(&m.item_url_name);
            let mut out = Outcome {
                hits: vec![],
                keys: vec![],
                candidates: orders.len(),
            };
            for (o, k) in orders.iter().zip(keys) {
                if new.contains(&k) {
                    out.hits.push(item_hit(rule, m, o, &name));
                    out.keys.push(k);
                }
            }
            return Ok(out);
        }
        Ok(Outcome {
            hits: vec![],
            keys: vec![],
            candidates: 0,
        })
    }

    fn set_error(&self, cx: &Ctx<'_>, error: Option<(String, String)>) {
        let changed = {
            let mut rt = self.rt();
            let changed = rt.last_error != error;
            rt.last_error = error;
            changed
        };
        if changed {
            (cx.changed)();
        }
    }

    async fn run_rule(
        &self,
        cx: &Ctx<'_>,
        rule: MarketAlertRule,
        binding: MarketAlertBinding,
        now: i64,
    ) {
        let result = self.evaluate(cx, &rule, false, now).await;
        // An edit or delete during the request makes its result stale.
        let current = cx.db(|c| store::get(c, &rule.id)).ok().flatten().as_ref() == Some(&rule);
        match result {
            _ if !current => {
                if let Err(e) = result {
                    self.set_error(cx, Some((rule.id.clone(), e.to_string())));
                }
            }
            Ok(out) => {
                {
                    let mut rt = self.rt();
                    rt.failures.remove(&rule.id);
                    rt.next_eval
                        .insert(rule.id.clone(), now + RULE_EVAL_INTERVAL_MS);
                }
                self.set_error(cx, None);
                if out.hits.is_empty() {
                    return;
                }
                cx.db(|c| {
                    let _ = store::mark_seen(c, &rule.id, &out.keys, now);
                    let _ = store::record_hits(c, &out.hits);
                });
                let first = &out.hits[0];
                let extra = if out.hits.len() > 1 {
                    format!(" (+{} more)", out.hits.len() - 1)
                } else {
                    String::new()
                };
                let mut d = NotificationDispatch::new(
                    NotificationSource::MarketAlerts,
                    first.title.clone(),
                    format!("{}{extra}", first.detail),
                );
                d.native_enabled = binding.native;
                cx.notifier.notify(d);
                if !rule.no_cooldown {
                    self.rt().cooldown_until.insert(
                        rule.id.clone(),
                        now + i64::from(rule.cooldown_minutes) * 60_000,
                    );
                }
                (cx.changed)();
            }
            Err(e) => {
                {
                    let mut rt = self.rt();
                    let n = rt.failures.entry(rule.id.clone()).or_default();
                    *n += 1;
                    let backoff = (FAILURE_BASE_MS << (*n - 1).min(10)).min(FAILURE_CEILING_MS);
                    rt.next_eval.insert(rule.id.clone(), now + backoff);
                }
                self.set_error(cx, Some((rule.id.clone(), e.to_string())));
            }
        }
    }

    /// One engine pass: the longest-waiting due rules, at most [`MAX_REQUESTS_PER_TICK`]. Returns how
    /// many rules ran. A tick that starts while another is running does nothing.
    pub async fn tick(&self, cx: &Ctx<'_>, now: i64) -> usize {
        if self.busy.swap(true, Ordering::SeqCst) {
            return 0;
        }
        self.rt().last_tick_at = Some(chrono::Utc::now().to_rfc3339());
        let rules = cx.db(store::load).unwrap_or_default();
        let mut due: Vec<_> = {
            let rt = self.rt();
            rules
                .into_iter()
                .filter(|(r, _)| {
                    r.enabled
                        && r.kind != MarketAlertKind::Baro
                        && rt.cooldown_until.get(&r.id).copied().unwrap_or(0) <= now
                        && rt.next_eval.get(&r.id).copied().unwrap_or(0) <= now
                })
                .collect()
        };
        {
            let rt = self.rt();
            due.sort_by_key(|(r, _)| rt.next_eval.get(&r.id).copied().unwrap_or(0));
        }
        due.truncate(MAX_REQUESTS_PER_TICK);
        let n = due.len();
        for (rule, binding) in due {
            self.run_rule(cx, rule, binding, now).await;
        }
        self.busy.store(false, Ordering::SeqCst);
        n
    }

    /// Evaluates one rule ignoring dedup and cooldown, and notifies with a `Test:` title.
    pub async fn test_fire(&self, cx: &Ctx<'_>, id: &str) -> Result<MarketAlertTestFire, String> {
        let rule = cx
            .db(|c| store::get(c, id))
            .map_err(|e| e.to_string())?
            .ok_or("unknown rule")?;
        if rule.kind == MarketAlertKind::Baro {
            return Err("baro rules are not evaluated yet".into());
        }
        let out = self
            .evaluate(cx, &rule, true, now_ms())
            .await
            .map_err(|e| e.to_string())?;
        let detail = out.hits.first().map_or_else(
            || format!("no current matches ({} listings checked)", out.candidates),
            |h| h.detail.clone(),
        );
        cx.notifier.notify(NotificationDispatch::new(
            NotificationSource::MarketAlerts,
            format!("Test: {}", rule.name),
            detail.clone(),
        ));
        Ok(MarketAlertTestFire {
            matches: out.hits.len() as u32,
            detail,
        })
    }

    pub fn cooldown_until(&self, id: &str, now: i64) -> Option<i64> {
        self.rt()
            .cooldown_until
            .get(id)
            .copied()
            .filter(|t| *t > now)
    }

    /// Forgets timing state so the rule is due on the next tick (edit, re-enable, manual reset).
    pub fn reset_rule(&self, id: &str) {
        let mut rt = self.rt();
        rt.cooldown_until.remove(id);
        rt.next_eval.remove(id);
        rt.failures.remove(id);
    }

    /// Like [`reset_rule`](Self::reset_rule), and drops the rule's error: a rule that will not run
    /// again cannot clear it itself.
    pub fn forget_rule(&self, id: &str) {
        self.reset_rule(id);
        let mut rt = self.rt();
        if rt.last_error.as_ref().is_some_and(|(r, _)| r == id) {
            rt.last_error = None;
        }
    }

    pub fn status(&self, db: &Db) -> MarketAlertEngineStatus {
        let rules = store::load(&db.lock()).unwrap_or_default();
        let mut rt = self.rt();
        let now = now_ms();
        rt.requests.retain(|t| *t > now - HOUR_MS);
        MarketAlertEngineStatus {
            running: self.armed.load(Ordering::SeqCst),
            rule_count: rules.len() as u32,
            enabled_count: rules.iter().filter(|(r, _)| r.enabled).count() as u32,
            last_tick_at: rt.last_tick_at.clone(),
            requests_last_hour: rt.requests.len() as u32,
            last_error: rt.last_error.as_ref().map(|(_, m)| m.clone()),
        }
    }
}
