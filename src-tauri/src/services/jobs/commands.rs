//! Production wiring of the jobs engine (the WFM client, the my-orders cache and SQLite behind the host
//! trait) and the `jobs_*` commands. Every start is a click; nothing here resumes by itself.

use std::sync::Arc;

use rusqlite::Connection;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};

use super::books::{MarketBook, MarketBooks};
use super::{
    ItemError, ItemStatus, Job, JobHost, JobItem, JobKind, JobState, JobUpdate, Jobs, UPDATED_EVENT,
};
use crate::db::Db;
use crate::services::wfm::client::WfmClient;
use crate::services::wfm::errors::WfmError;
use crate::services::wfm::my_orders::{MyOrdersCache, announce};
use crate::services::wfm::orders::{self, MyOrders};
use crate::services::wfm::scheduler::{self, Priority};
use crate::services::wfm::session::Session;
use crate::services::wfm::validators::{self, OrderUpdates};
use crate::services::workbench::types::{WorkbenchPhase, WorkbenchRowStatus, WorkbenchState};
use crate::services::workbench::{INTER_ROW_DELAY, Workbench, commands::WfmHost};

/// More than a seller's slots can hold; a bigger request is a mistake, not something to cut short.
pub const MAX_JOB_ITEMS: usize = 500;

/// One book read for a `MarketLoad` item (its id is the slug), kept for the modal to pick up.
pub async fn load_book(
    client: &WfmClient,
    books: &MarketBooks,
    item: &JobItem,
) -> Result<(), WfmError> {
    let orders = orders::orders_item(client, &item.id, false, None, None).await?;
    books.put(&item.id, orders);
    Ok(())
}

pub struct AppHost(AppHandle);

pub type AppJobs = Jobs<AppHost>;

/// One confirmed write, and the my-orders copy edited the way the single-order commands do, so
/// `wfm-orders-updated` keeps the rows in step.
pub async fn write_order(
    client: &WfmClient,
    cache: &MyOrdersCache,
    emit: impl Fn(&MyOrders),
    kind: &JobKind,
    item: &JobItem,
) -> Result<(), WfmError> {
    let patch = |updates| orders::update(client, &item.id, updates);
    let order = match kind {
        JobKind::OrdersVisibility { visible } => {
            patch(OrderUpdates {
                visible: Some(*visible),
                ..Default::default()
            })
            .await?
        }
        JobKind::Reprice => {
            let platinum = item
                .platinum
                .ok_or_else(|| WfmError::invalid("reprice: no price"))?;
            patch(OrderUpdates {
                platinum: Some(platinum),
                ..Default::default()
            })
            .await?
        }
        JobKind::OrdersDelete => {
            // 404: the order is already gone (an earlier run's delete landed), which is the goal.
            match orders::delete(client, &item.id).await {
                Ok(_) | Err(WfmError::Api { status: 404, .. }) => cache.remove(&item.id, emit),
                Err(e) => return Err(e),
            }
            return Ok(());
        }
        JobKind::Workbench | JobKind::PricesLoad | JobKind::MarketLoad => {
            return Err(WfmError::invalid("not an orders job"));
        }
    };
    cache.upsert(order, emit);
    Ok(())
}

impl JobHost for AppHost {
    async fn run_item(&self, kind: &JobKind, item: &JobItem) -> Result<(), ItemError> {
        let (session, cache) = (self.0.state::<Session>(), self.0.state::<MyOrdersCache>());
        // Priority::Job: the user clicked and waits for it, so it goes ahead of the Background price sweeps.
        let sent = if *kind == JobKind::MarketLoad {
            let books = self.0.state::<MarketBooks>();
            scheduler::scoped(Priority::Job, load_book(session.client(), &books, item)).await
        } else {
            scheduler::scoped(
                Priority::Job,
                write_order(session.client(), &cache, announce(&self.0), kind, item),
            )
            .await
        };
        sent.map_err(|e| ItemError {
            halt: matches!(e, WfmError::Unauthorized { .. }),
            // A lost connection, not an answer from the server.
            offline: matches!(e, WfmError::Network { .. }),
            reason: e.to_string(),
        })
    }

    fn with_db<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        f(&self.0.state::<Db>().lock())
    }

    fn emit(&self, update: &JobUpdate) {
        let _ = self.0.emit(UPDATED_EVENT, update);
    }

    fn hold(&self, on: bool) {
        self.0.state::<MyOrdersCache>().set_held(on);
    }
}

pub fn setup(app: &tauri::App) {
    app.manage(MarketBooks::default());
    app.manage(Arc::new(AppJobs::new(
        AppHost(app.handle().clone()),
        INTER_ROW_DELAY,
    )));
}

/// The workbench run as a job: its engine stays in charge, this only shows its progress.
pub fn mirror_workbench(jobs: &AppJobs, s: &WorkbenchState) {
    let Some(run) = &s.run else { return };
    let id = format!("workbench-{}", run.plan_id);
    let finished = run.finished_at.is_some();
    // A run already dismissed must not come back with the next state event.
    if finished && !jobs.has(&id) {
        return;
    }
    let items: Vec<JobItem> = run
        .rows
        .iter()
        .map(|r| JobItem {
            id: r.row_id.clone(),
            name: r.item_name.clone(),
            platinum: None,
            status: match r.status {
                WorkbenchRowStatus::Pending => ItemStatus::Pending,
                WorkbenchRowStatus::InFlight => ItemStatus::Running,
                WorkbenchRowStatus::Done => ItemStatus::Done,
                WorkbenchRowStatus::Failed => ItemStatus::Failed,
                WorkbenchRowStatus::Cancelled | WorkbenchRowStatus::Blocked => ItemStatus::Skipped,
            },
            reason: r.error.clone(),
        })
        .collect();
    let state = match (finished, s.phase) {
        (true, _) => JobState::Done,
        (false, WorkbenchPhase::Cancelling) => JobState::Stopping,
        _ => JobState::Running,
    };
    jobs.mirror(
        &id,
        JobKind::Workbench,
        state,
        (0, items.len() as u32),
        items,
    );
}

type Engine<'a> = State<'a, Arc<AppJobs>>;

/// Valid ids only (a bad one is a bug in the caller, not something to skip), each once, none beyond the cap.
fn checked_ids(ids: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id.trim().to_owned();
        if !validators::valid_id(&id) {
            return Err("Invalid order id.".into());
        }
        if seen.insert(id.clone()) {
            out.push(id);
        }
    }
    if out.is_empty() {
        return Err("No orders selected.".into());
    }
    if out.len() > MAX_JOB_ITEMS {
        return Err(format!("At most {MAX_JOB_ITEMS} orders at a time."));
    }
    Ok(out)
}

fn items(ids: Vec<(String, Option<u32>)>, cache: &MyOrdersCache) -> Vec<JobItem> {
    ids.into_iter()
        .map(|(id, platinum)| JobItem {
            name: cache.item_name(&id).unwrap_or_else(|| id.clone()),
            id,
            platinum,
            status: ItemStatus::Pending,
            reason: None,
        })
        .collect()
}

fn start_orders(
    jobs: &Arc<AppJobs>,
    session: &Session,
    cache: &MyOrdersCache,
    kind: JobKind,
    rows: Vec<(String, Option<u32>)>,
) -> Result<String, String> {
    if session.client().token().is_none() {
        return Err("Sign in to warframe.market first.".into());
    }
    jobs.start(kind, items(rows, cache))
}

#[tauri::command]
#[specta::specta]
pub fn jobs_start_orders_visibility(
    jobs: Engine,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    ids: Vec<String>,
    visible: bool,
) -> Result<String, String> {
    let rows = checked_ids(ids)?.into_iter().map(|i| (i, None)).collect();
    start_orders(
        &jobs,
        &session,
        &cache,
        JobKind::OrdersVisibility { visible },
        rows,
    )
}

#[tauri::command]
#[specta::specta]
pub fn jobs_start_orders_delete(
    jobs: Engine,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    ids: Vec<String>,
) -> Result<String, String> {
    let rows = checked_ids(ids)?.into_iter().map(|i| (i, None)).collect();
    start_orders(&jobs, &session, &cache, JobKind::OrdersDelete, rows)
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepriceItem {
    pub id: String,
    pub platinum: u32,
}

#[tauri::command]
#[specta::specta]
pub fn jobs_start_reprice(
    jobs: Engine,
    session: State<'_, Session>,
    cache: State<'_, MyOrdersCache>,
    items: Vec<RepriceItem>,
) -> Result<String, String> {
    let prices: std::collections::HashMap<_, _> = items
        .iter()
        .map(|i| (i.id.trim().to_owned(), i.platinum))
        .collect();
    let ids = checked_ids(items.into_iter().map(|i| i.id).collect())?;
    let rows = ids
        .into_iter()
        .map(|id| {
            let p = validators::platinum(prices[&id]).map_err(|e| e.to_string())?;
            Ok((id, Some(p)))
        })
        .collect::<Result<_, String>>()?;
    start_orders(&jobs, &session, &cache, JobKind::Reprice, rows)
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketRow {
    pub slug: String,
    pub name: String,
}

/// Reads the order book of each item, one at a time, in the background. Read only.
#[tauri::command]
#[specta::specta]
pub fn jobs_start_market_load(jobs: Engine, rows: Vec<MarketRow>) -> Result<String, String> {
    let mut seen = std::collections::HashSet::new();
    let items: Vec<JobItem> = rows
        .into_iter()
        .filter(|r| seen.insert(r.slug.clone()))
        .map(|r| JobItem {
            id: r.slug,
            name: r.name,
            platinum: None,
            status: ItemStatus::Pending,
            reason: None,
        })
        .collect();
    if items.len() > MAX_JOB_ITEMS {
        return Err(format!("At most {MAX_JOB_ITEMS} items at a time."));
    }
    jobs.start(JobKind::MarketLoad, items)
}

/// The books already loaded among `slugs` (fresh ones only).
#[tauri::command]
#[specta::specta]
pub fn jobs_market_books(books: State<'_, MarketBooks>, slugs: Vec<String>) -> Vec<MarketBook> {
    books.get(&slugs)
}

#[tauri::command]
#[specta::specta]
pub fn jobs_stop(
    jobs: Engine,
    workbench: State<'_, Arc<Workbench<WfmHost>>>,
    id: String,
) -> Result<(), String> {
    if jobs.stop(&id)? == JobKind::Workbench {
        workbench.cancel();
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn jobs_retry_failed(jobs: Engine, id: String) -> Result<String, String> {
    jobs.retry_failed(&id)
}

#[tauri::command]
#[specta::specta]
pub fn jobs_continue(jobs: Engine, id: String) -> Result<String, String> {
    jobs.continue_job(&id)
}

#[tauri::command]
#[specta::specta]
pub fn jobs_dismiss(jobs: Engine, id: String) -> Result<(), String> {
    jobs.dismiss(&id)
}

#[tauri::command]
#[specta::specta]
pub fn jobs_list(jobs: Engine) -> Vec<Job> {
    jobs.list()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::wfm::mock::{reply, spawn};
    use crate::services::wfm::orders::MyOrder;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbb";

    fn row(id: &str) -> MyOrder {
        MyOrder {
            id: id.into(),
            order_type: "sell".into(),
            platinum: 10,
            quantity: 1,
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

    fn job_item(id: &str, platinum: Option<u32>) -> JobItem {
        JobItem {
            id: id.into(),
            name: "Serration".into(),
            platinum,
            status: ItemStatus::Pending,
            reason: None,
        }
    }

    /// A dead network is a failed item with a reason, never an empty book.
    #[tokio::test]
    async fn a_market_load_fails_the_item_when_the_network_is_down() {
        let dead = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", l.local_addr().unwrap())
        };
        let c = WfmClient::new(
            &format!("{dead}/v1"),
            &format!("{dead}/v2"),
            Box::new(|_| {}),
        )
        .unwrap();
        let books = MarketBooks::default();
        let e = load_book(&c, &books, &job_item("serration", None))
            .await
            .unwrap_err();
        assert!(matches!(e, WfmError::Network { .. }), "{e}");
        assert!(books.get(&["serration".into()]).is_empty());
    }

    /// A market load stores the raw book under the slug, so a modal opened later reads it.
    #[tokio::test]
    async fn a_market_load_keeps_the_book_for_a_later_reader() {
        let book = r#"{"data":[{"id":"o1","type":"sell","platinum":12,"quantity":1}]}"#;
        let m = spawn(vec![reply(200, book), reply(500, "{}")]);
        let c = WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        let books = MarketBooks::default();
        load_book(&c, &books, &job_item("serration", None))
            .await
            .unwrap();
        let got = books.get(&["serration".into()]);
        assert_eq!(got[0].orders.len(), 1);
        assert_eq!(got[0].orders[0].platinum, 12);
        assert!(
            load_book(&c, &books, &job_item("gone", None))
                .await
                .is_err()
        );
        assert!(books.get(&["gone".into()]).is_empty());
    }

    /// Each confirmed write edits the my-orders copy and announces it, the way the single-order
    /// commands do, so the rows follow `wfm-orders-updated` while a job runs.
    #[tokio::test]
    async fn orders_writes_edit_the_my_orders_copy_and_announce_each_change() {
        let patched = format!(
            r#"{{"data":{{"id":"{A}","type":"sell","platinum":42,"quantity":1,"visible":false,"itemId":"i"}}}}"#
        );
        let m = spawn(vec![
            reply(200, &patched),
            reply(200, &patched),
            reply(200, "{}"),
            reply(404, "{}"),
        ]);
        let c = WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        let cache = MyOrdersCache::default();
        cache.store(&MyOrders {
            sell: vec![row(A), row(B)],
            buy: vec![],
        });
        let seen = std::cell::RefCell::new(vec![]);
        let emit = |o: &MyOrders| seen.borrow_mut().push(o.clone());

        let hide = JobKind::OrdersVisibility { visible: false };
        write_order(&c, &cache, emit, &hide, &job_item(A, None))
            .await
            .unwrap();
        let last = seen.borrow().last().cloned().unwrap();
        assert!(!last.sell.iter().find(|o| o.id == A).unwrap().visible);
        assert_eq!(
            last.sell[0].item_name, "Serration",
            "the answer has no name: the row keeps its own"
        );

        write_order(&c, &cache, emit, &JobKind::Reprice, &job_item(A, Some(42)))
            .await
            .unwrap();
        assert_eq!(
            seen.borrow()
                .last()
                .unwrap()
                .sell
                .iter()
                .find(|o| o.id == A)
                .unwrap()
                .platinum,
            42
        );
        assert!(
            write_order(&c, &cache, emit, &JobKind::Reprice, &job_item(A, None))
                .await
                .is_err()
        );

        write_order(&c, &cache, emit, &JobKind::OrdersDelete, &job_item(B, None))
            .await
            .unwrap();
        let ids: Vec<_> = seen
            .borrow()
            .last()
            .unwrap()
            .sell
            .iter()
            .map(|o| o.id.clone())
            .collect();
        assert_eq!(ids, [A]);

        // Already deleted (404) counts as done and leaves the copy without it.
        write_order(&c, &cache, emit, &JobKind::OrdersDelete, &job_item(A, None))
            .await
            .unwrap();
        assert!(seen.borrow().last().unwrap().sell.is_empty());

        // Any other failure announces nothing.
        let before = seen.borrow().len();
        let e = write_order(&c, &cache, emit, &JobKind::OrdersDelete, &job_item(A, None))
            .await
            .unwrap_err();
        assert!(matches!(e, WfmError::Api { status: 599, .. }));
        assert_eq!(seen.borrow().len(), before);
        let reqs = m.requests();
        assert!(
            reqs[0].starts_with(&format!("patch /v2/order/{A} "))
                && reqs[0].contains(r#"{"visible":false}"#)
        );
        assert!(reqs[1].contains(r#"{"platinum":42}"#));
    }

    #[test]
    fn ids_are_checked_deduped_and_capped() {
        let a = "a".repeat(24);
        assert_eq!(
            checked_ids(vec![a.clone(), a.clone()]).unwrap(),
            [a.as_str()]
        );
        assert!(checked_ids(vec![]).is_err());
        assert!(checked_ids(vec![a.clone(), "nope".into()]).is_err());
        let many = (0..=MAX_JOB_ITEMS).map(|i| format!("{i:024x}")).collect();
        assert!(checked_ids(many).is_err());
    }
}
