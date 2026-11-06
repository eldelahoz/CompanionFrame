//! My orders, kept in memory: the views paint the last copy at once and WFM is asked in the
//! background. Confirmed writes edit the copy in place; every change goes out as `wfm-orders-updated`.
//!
//! A fetch replaces the copy. One that was already in flight when a write confirmed can bring the old
//! row back for a moment; the next poll (30 s) corrects it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::time::MissedTickBehavior;

use super::client::WfmClient;
use super::errors::WfmError;
use super::orders::{self, MyOrder, MyOrders, OrderAck, SEND_PER_TRADE, VisibilityResult};
use super::session::Session;
use super::validators::{self, CreateOrder, OrderUpdates};
use crate::db::Db;

pub const UPDATED_EVENT: &str = "wfm-orders-updated";
const FRESH: Duration = Duration::from_secs(30);
/// A forced fetch that finds a copy this young (someone fetched while it waited) is satisfied by it.
const JUST_NOW: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Held {
    orders: Option<MyOrders>,
    at: Option<Instant>,
}

#[derive(Default)]
pub struct MyOrdersCache {
    held: Mutex<Held>,
    /// One fetch at a time; the next caller finds the copy it produced.
    fetching: tokio::sync::Mutex<()>,
    /// Bulk runs in progress: the poll waits for them.
    holds: AtomicUsize,
}

/// What `wfm_orders_my_cached` answers: the last copy (`None`: nothing fetched yet) and whether a
/// refresh was started because the copy is older than 30 s or missing.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct CachedOrders {
    pub orders: Option<MyOrders>,
    pub refreshing: bool,
}

/// Keeps the poll quiet while it lives (a bulk run is writing).
pub struct Hold<'a>(&'a MyOrdersCache);

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        self.0.release();
    }
}

impl MyOrdersCache {
    pub fn hold(&self) -> Hold<'_> {
        self.holds.fetch_add(1, Ordering::SeqCst);
        Hold(self)
    }

    fn release(&self) {
        let _ = self
            .holds
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
    }

    /// The copy, and whether it is younger than `age`.
    fn snapshot(&self, age: Duration) -> (Option<MyOrders>, bool) {
        let h = self.held.lock().unwrap();
        (h.orders.clone(), h.at.is_some_and(|t| t.elapsed() < age))
    }

    fn store(&self, o: &MyOrders) {
        *self.held.lock().unwrap() = Held {
            orders: Some(o.clone()),
            at: Some(Instant::now()),
        };
    }

    /// A fresh copy as is, else fetched; `force` fetches regardless (the Refresh button, the poll).
    pub async fn fetch(
        &self,
        client: &WfmClient,
        db: &Db,
        force: bool,
        emit: impl Fn(&MyOrders),
    ) -> Result<MyOrders, WfmError> {
        let age = if force { JUST_NOW } else { FRESH };
        if let (Some(o), true) = self.snapshot(age)
            && !force
        {
            return Ok(o);
        }
        let _one = self.fetching.lock().await;
        if let (Some(o), true) = self.snapshot(age) {
            return Ok(o);
        }
        let t = Instant::now();
        let o = orders::my_orders(client, db).await?;
        self.store(&o);
        emit(&o);
        tracing::debug!("timing: my_orders_fetch ms={}", t.elapsed().as_millis());
        Ok(o)
    }

    /// Edits the copy (when there is one) and announces the result.
    fn change(&self, emit: impl Fn(&MyOrders), f: impl FnOnce(&mut MyOrders)) {
        let out = {
            let mut h = self.held.lock().unwrap();
            let Some(o) = h.orders.as_mut() else { return };
            f(o);
            o.clone()
        };
        emit(&out);
    }

    /// A created or updated order. WFM answers an update without the item name: the row keeps its own.
    pub fn upsert(&self, order: MyOrder, emit: impl Fn(&MyOrders)) {
        self.change(emit, |o| {
            let side = if order.order_type == "buy" {
                &mut o.buy
            } else {
                &mut o.sell
            };
            match side.iter_mut().find(|x| x.id == order.id) {
                Some(row) => {
                    let old = (
                        row.item_name.clone(),
                        row.item_slug.clone(),
                        row.item_thumb.clone(),
                    );
                    *row = order;
                    if row.item_name == "(unknown)" {
                        (row.item_name, row.item_slug, row.item_thumb) = old;
                    }
                }
                None => side.insert(0, order),
            }
        });
    }

    pub fn remove(&self, id: &str, emit: impl Fn(&MyOrders)) {
        self.change(emit, |o| {
            o.sell.retain(|x| x.id != id);
            o.buy.retain(|x| x.id != id);
        });
    }

    /// `quantity` units were closed: the row shrinks, or goes when nothing is left.
    pub fn lower(&self, id: &str, quantity: u32, emit: impl Fn(&MyOrders)) {
        self.change(emit, |o| {
            for side in [&mut o.sell, &mut o.buy] {
                side.retain_mut(|x| {
                    if x.id != id {
                        return true;
                    }
                    x.quantity = x.quantity.saturating_sub(quantity);
                    x.per_trade = x.per_trade.min(x.quantity.max(1));
                    x.quantity > 0
                });
            }
        });
    }

    /// Signed out: forget everything. `true` when there was something to forget.
    pub fn clear(&self, emit: impl Fn(&MyOrders)) -> bool {
        let had = std::mem::take(&mut *self.held.lock().unwrap())
            .orders
            .is_some();
        if had {
            emit(&MyOrders::default());
        }
        had
    }
}

fn announce(app: &AppHandle) -> impl Fn(&MyOrders) + '_ {
    |o| {
        let _ = app.emit(UPDATED_EVENT, o);
    }
}

/// The poll: every 30 s while signed in, someone has asked for the orders, the window is showing and no
/// bulk run is writing. Signing out empties the copy.
pub fn start_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(FRESH);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let (session, db, cache) = (
                app.state::<Session>(),
                app.state::<Db>(),
                app.state::<MyOrdersCache>(),
            );
            if session.client().token().is_none() {
                cache.clear(announce(&app));
                continue;
            }
            let showing = app.get_webview_window("main").is_some_and(|w| {
                w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false)
            });
            if showing
                && cache.holds.load(Ordering::SeqCst) == 0
                && cache.snapshot(FRESH).0.is_some()
            {
                let _ = cache
                    .fetch(session.client(), &db, true, announce(&app))
                    .await;
            }
        }
    });
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_my(
    app: AppHandle,
    session: State<'_, Session>,
    db: State<'_, Db>,
    cache: State<'_, MyOrdersCache>,
) -> Result<MyOrders, WfmError> {
    cache
        .fetch(session.client(), &db, false, announce(&app))
        .await
}

/// The last copy at once, however old; a refresh starts in the background when it is stale or missing
/// and its result arrives as `wfm-orders-updated`.
#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_my_cached(
    app: AppHandle,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
) -> Result<CachedOrders, WfmError> {
    if session.client().token().is_none() {
        return Ok(CachedOrders {
            orders: None,
            refreshing: false,
        });
    }
    let (orders, fresh) = cache.snapshot(FRESH);
    if !fresh {
        tauri::async_runtime::spawn(async move {
            let (session, db, cache) = (
                app.state::<Session>(),
                app.state::<Db>(),
                app.state::<MyOrdersCache>(),
            );
            let _ = cache
                .fetch(session.client(), &db, false, announce(&app))
                .await;
        });
    }
    Ok(CachedOrders {
        orders,
        refreshing: !fresh,
    })
}

/// The Refresh button: always asks WFM.
#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_refresh(
    app: AppHandle,
    session: State<'_, Session>,
    db: State<'_, Db>,
    cache: State<'_, MyOrdersCache>,
) -> Result<MyOrders, WfmError> {
    cache
        .fetch(session.client(), &db, true, announce(&app))
        .await
}

/// A bulk run started (`true`) or ended (`false`): the poll waits meanwhile.
#[tauri::command]
#[specta::specta]
pub fn wfm_orders_hold(cache: State<'_, MyOrdersCache>, hold: bool) {
    if hold {
        std::mem::forget(cache.hold());
    } else {
        cache.release();
    }
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_create(
    app: AppHandle,
    session: State<'_, Session>,
    db: State<'_, Db>,
    cache: State<'_, MyOrdersCache>,
    order: CreateOrder,
) -> Result<MyOrder, WfmError> {
    let client = session.client();
    let created = orders::create(client, &db, order, &SEND_PER_TRADE).await?;
    let named = orders::named(client, &db, created).await;
    cache.upsert(named.clone(), announce(&app));
    Ok(named)
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_update(
    app: AppHandle,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    order_id: String,
    updates: OrderUpdates,
) -> Result<MyOrder, WfmError> {
    let updated = orders::update(session.client(), &order_id, updates).await?;
    cache.upsert(updated.clone(), announce(&app));
    Ok(updated)
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_delete(
    app: AppHandle,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    order_id: String,
) -> Result<OrderAck, WfmError> {
    let ack = orders::delete(session.client(), &order_id).await?;
    cache.remove(&ack.id, announce(&app));
    Ok(ack)
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_close(
    app: AppHandle,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    order_id: String,
    quantity: u32,
    per_trade: Option<u32>,
) -> Result<OrderAck, WfmError> {
    let ack = orders::close(session.client(), &order_id, quantity, per_trade).await?;
    // `close` validated it already; this is the amount WFM took off the row.
    let closed = validators::close_quantity(quantity, per_trade)?;
    cache.lower(&ack.id, closed, announce(&app));
    Ok(ack)
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_set_visible(
    app: AppHandle,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    order_ids: Vec<String>,
    visible: bool,
) -> Result<Vec<VisibilityResult>, WfmError> {
    let _hold = cache.hold();
    let out = orders::set_visible(session.client(), &order_ids, visible).await?;
    for order in out.iter().filter_map(|r| r.order.clone()) {
        cache.upsert(order, announce(&app));
    }
    Ok(out)
}

/// Hides or shows all sell (or all buy) orders in one request, then refreshes the copy.
#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_set_all_visible(
    app: AppHandle,
    session: State<'_, Session>,
    db: State<'_, Db>,
    cache: State<'_, MyOrdersCache>,
    order_type: String,
    visible: bool,
) -> Result<u32, WfmError> {
    let client = session.client();
    let updated = orders::set_all_visible(client, &order_type, visible).await?;
    if cache
        .fetch(client, &db, true, announce(&app))
        .await
        .is_err()
    {
        // The write is confirmed even if the re-read is not: show it.
        cache.change(announce(&app), |o| {
            let side = if order_type == "buy" {
                &mut o.buy
            } else {
                &mut o.sell
            };
            side.iter_mut().for_each(|x| x.visible = visible);
        });
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::super::mock::{Mock, reply, spawn};
    use super::*;
    use crate::db::mem_db;

    fn client(m: &Mock) -> WfmClient {
        let c = WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        c
    }

    fn row(id: &str, quantity: u32) -> MyOrder {
        MyOrder {
            id: id.into(),
            order_type: "sell".into(),
            platinum: 10,
            quantity,
            per_trade: 1,
            visible: true,
            mod_rank: None,
            subtype: None,
            item_id: Some("i".into()),
            item_name: "Serration".into(),
            item_slug: Some("serration".into()),
            item_thumb: None,
            created_at: None,
            updated_at: None,
        }
    }

    const MINE: &str = r#"{"data":[{"id":"a","type":"sell","platinum":5,"quantity":3,"visible":true,"itemId":"i"}]}"#;
    const NO_ITEMS: &str = r#"{"data":{"collections":{"items":"v1"}}}"#;

    #[tokio::test]
    async fn a_fresh_copy_is_served_without_a_request_and_a_forced_fetch_announces() {
        // Each fetch also resolves names: /versions + /items answer once, then memory.
        let m = spawn(vec![
            reply(200, MINE),
            reply(200, NO_ITEMS),
            reply(
                200,
                r#"{"data":[{"id":"i","slug":"serration","i18n":{"en":{"name":"Serration"}}}]}"#,
            ),
            reply(200, MINE),
        ]);
        let (c, db, cache) = (client(&m), mem_db(), MyOrdersCache::default());
        let sent = RefCell::new(0);
        let emit = |_: &MyOrders| *sent.borrow_mut() += 1;
        cache.fetch(&c, &db, false, emit).await.unwrap();
        cache.fetch(&c, &db, false, emit).await.unwrap();
        let orders = m
            .requests()
            .iter()
            .filter(|r| r.starts_with("get /v2/orders/my "))
            .count();
        assert_eq!((orders, *sent.borrow()), (1, 1));
        // The Refresh button always asks (past the 2 s a concurrent fetch would satisfy it).
        cache.held.lock().unwrap().at = Some(Instant::now() - Duration::from_secs(5));
        cache.fetch(&c, &db, true, emit).await.unwrap();
        let orders = m
            .requests()
            .iter()
            .filter(|r| r.starts_with("get /v2/orders/my "))
            .count();
        assert_eq!((orders, *sent.borrow()), (2, 2));
    }

    #[test]
    fn confirmed_writes_edit_the_copy_in_place_and_announce_it() {
        let cache = MyOrdersCache::default();
        let seen = RefCell::new(Vec::<MyOrders>::new());
        let emit = |o: &MyOrders| seen.borrow_mut().push(o.clone());
        cache.upsert(row("a", 3), emit);
        assert!(seen.borrow().is_empty(), "no copy yet: nothing to edit");
        cache.store(&MyOrders {
            sell: vec![row("a", 3), row("b", 2)],
            buy: vec![],
        });
        // An update answer without the item name keeps the row's own.
        let mut answer = row("a", 3);
        (answer.platinum, answer.item_name, answer.item_slug) = (99, "(unknown)".into(), None);
        cache.upsert(answer, emit);
        let last = seen.borrow().last().cloned().unwrap();
        assert_eq!(
            (last.sell[0].platinum, last.sell[0].item_name.as_str()),
            (99, "Serration")
        );
        cache.upsert(row("c", 1), emit);
        assert_eq!(seen.borrow().last().unwrap().sell[0].id, "c");
        cache.lower("b", 1, emit);
        assert_eq!(seen.borrow().last().unwrap().sell[2].quantity, 1);
        cache.lower("b", 1, emit);
        cache.remove("a", emit);
        let ids: Vec<_> = seen
            .borrow()
            .last()
            .unwrap()
            .sell
            .iter()
            .map(|o| o.id.clone())
            .collect();
        assert_eq!(ids, ["c"]);
        assert!(cache.clear(emit));
        assert!(cache.snapshot(FRESH).0.is_none());
    }

    #[tokio::test]
    async fn hide_all_is_one_patch_with_the_visibility_and_the_side() {
        let m = spawn(vec![reply(200, r#"{"data":{"updated":4}}"#)]);
        let c = client(&m);
        assert_eq!(orders::set_all_visible(&c, "sell", false).await.unwrap(), 4);
        let reqs = m.requests();
        assert_eq!(reqs.len(), 1);
        assert!(
            reqs[0].starts_with("patch /v2/orders/group/all "),
            "{}",
            reqs[0]
        );
        let body: serde_json::Value =
            serde_json::from_str(reqs[0].rsplit("\r\n\r\n").next().unwrap()).unwrap();
        assert_eq!(body, serde_json::json!({"visible": false, "type": "sell"}));
        assert!(
            orders::set_all_visible(&c, "everything", true)
                .await
                .is_err()
        );
    }

    #[test]
    fn the_hold_is_counted_and_never_goes_below_zero() {
        let cache = MyOrdersCache::default();
        drop(cache.hold());
        cache.release();
        assert_eq!(cache.holds.load(Ordering::SeqCst), 0);
        let h = cache.hold();
        assert_eq!(cache.holds.load(Ordering::SeqCst), 1);
        drop(h);
    }
}
