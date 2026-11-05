//! Production wiring of the workbench: the WFM client and SQLite behind the host traits, and the
//! `workbench_*` commands. Every command here is a user action (a click), never a timer.

use std::sync::Arc;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State};

use super::matcher::{
    ItemRankInfo, MatchContract, MatchHost, Matcher, WorkbenchMatchKind, WorkbenchTrade,
    WorkbenchTradeMatch,
};
use super::types::*;
use super::{INTER_ROW_DELAY, NewSellOrder, Workbench, WorkbenchHost};
use crate::db::Db;
use crate::services::wfm::catalog::{self, SetLookup};
use crate::services::wfm::contracts::{self, Contracts, WfmContract};
use crate::services::wfm::errors::WfmError;
use crate::services::wfm::legacy_v1;
use crate::services::wfm::orders::{self, MyOrder, MyOrders, SEND_PER_TRADE};
use crate::services::wfm::scheduler::{self, Priority};
use crate::services::wfm::session::Session;
use crate::services::wfm::validators::{CreateOrder, OrderUpdates};

pub const STATE_EVENT: &str = "workbench-updated";
pub const ORDER_CLOSED_EVENT: &str = "wfm-order-closed";

/// Reads the session and database from managed state on every call, so it needs only the handle.
#[derive(Clone)]
pub struct WfmHost(AppHandle);

impl WfmHost {
    fn session(&self) -> State<'_, Session> {
        self.0.state()
    }
    fn db(&self) -> State<'_, Db> {
        self.0.state()
    }
}

impl WorkbenchHost for WfmHost {
    async fn create_order(&self, o: NewSellOrder) -> Result<String, WfmError> {
        let order = CreateOrder {
            item_id: o.item_id,
            order_type: "sell".into(),
            platinum: o.platinum,
            quantity: o.quantity,
            visible: Some(true),
            mod_rank: o.rank,
            per_trade: None,
            subtype: o.subtype,
        };
        let created = scheduler::scoped(
            Priority::Job,
            orders::create(self.session().client(), &self.db(), order, &SEND_PER_TRADE),
        )
        .await?;
        Ok(created.id)
    }

    async fn update_order(
        &self,
        id: &str,
        platinum: u32,
        quantity: u32,
    ) -> Result<String, WfmError> {
        let updates = OrderUpdates {
            platinum: Some(platinum),
            quantity: Some(quantity),
            ..Default::default()
        };
        Ok(scheduler::scoped(
            Priority::Job,
            orders::update(self.session().client(), id, updates),
        )
        .await?
        .id)
    }

    async fn my_sell_orders(&self) -> Result<Vec<MyOrder>, WfmError> {
        Ok(orders::my_orders(self.session().client(), &self.db())
            .await?
            .sell)
    }

    async fn item_id(&self, slug: &str) -> Result<Option<String>, WfmError> {
        let items = catalog::items_containing(self.session().client(), &self.db(), |l| {
            l.iter().any(|i| i.slug == slug)
        })
        .await?;
        Ok(items.iter().find(|i| i.slug == slug).map(|i| i.id.clone()))
    }

    fn with_db<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        f(&self.db().lock())
    }

    fn emit(&self, state: &WorkbenchState) {
        let _ = self.0.emit(STATE_EVENT, state);
    }
}

impl MatchHost for WfmHost {
    fn logged_in(&self) -> bool {
        self.session().status().logged_in
    }

    async fn my_orders(&self) -> Result<MyOrders, WfmError> {
        orders::my_orders(self.session().client(), &self.db()).await
    }

    async fn slug_by_name(&self, name: &str) -> Option<String> {
        let wanted = name.trim().to_lowercase();
        let items = catalog::items(self.session().client(), &self.db())
            .await
            .ok()?;
        items
            .iter()
            .find(|i| i.name.to_lowercase() == wanted)
            .map(|i| i.slug.clone())
    }

    async fn item_rank_info(&self, slug: &str) -> Result<Option<ItemRankInfo>, WfmError> {
        // Only the live item establishes rankless status; the cached list may omit the rank.
        Ok(catalog::item(self.session().client(), slug)
            .await?
            .map(|i| ItemRankInfo {
                has_ranks: Some(i.max_rank.unwrap_or(0) > 0),
                id: i.id,
                slug: i.slug,
            }))
    }

    async fn set_lookup(&self, slug: &str) -> Result<SetLookup, WfmError> {
        catalog::item_set(self.session().client(), slug).await
    }

    // shortcut: one page of 100 auctions, WFM caps a seller's riven slots below that.
    async fn my_contracts(&self) -> Result<Vec<MatchContract>, WfmError> {
        let session = self.session();
        let user = session.status().user_name;
        let page = contracts::my_contracts(
            session.client(),
            &self.0.state::<Contracts>(),
            user.as_deref(),
            None,
            Some(100),
        )
        .await?;
        Ok(page.contracts.into_iter().map(match_contract).collect())
    }

    async fn close_order(
        &self,
        id: &str,
        quantity: u32,
        per_trade: Option<u32>,
    ) -> Result<(), WfmError> {
        orders::close(self.session().client(), id, quantity, per_trade).await?;
        Ok(())
    }

    async fn close_contract(&self, id: &str) -> Result<(), WfmError> {
        legacy_v1::auction_close(self.session().client(), id).await
    }
}

fn match_contract(c: WfmContract) -> MatchContract {
    MatchContract {
        id: c.id,
        weapon_url_name: c.weapon_url_name,
        riven_suffix: c.riven_suffix,
        item_thumb: c.item_thumb,
        platinum: c.platinum,
    }
}

type Engine<'a> = State<'a, Arc<Workbench<WfmHost>>>;

pub fn setup(app: &tauri::App) {
    let host = WfmHost(app.handle().clone());
    app.manage(Arc::new(Workbench::new(host.clone(), INTER_ROW_DELAY)));
    app.manage(Matcher::new(host));
}

fn invalid(what: &str) -> String {
    tracing::warn!("[Security] workbench: blocked an invalid {what} payload");
    format!("Invalid workbench {what} payload.")
}

#[tauri::command]
#[specta::specta]
pub fn workbench_get_state(wb: Engine) -> WorkbenchState {
    wb.state()
}

#[tauri::command]
#[specta::specta]
pub fn workbench_preview_plan(
    plan: WorkbenchPlan,
    safety: WorkbenchSafetySnapshot,
) -> Result<WorkbenchPlanValidation, String> {
    match (plan.sanitized(), safety.sanitized()) {
        (Some(p), Some(s)) => Ok(validate_plan(&p, &s)),
        _ => Err(invalid("plan")),
    }
}

#[tauri::command]
#[specta::specta]
pub fn workbench_execute_plan(
    wb: Engine,
    plan: WorkbenchPlan,
    safety: WorkbenchSafetySnapshot,
) -> WorkbenchExecuteResult {
    match (plan.sanitized(), safety.sanitized()) {
        (Some(p), Some(s)) => wb.execute(p, &s),
        _ => WorkbenchExecuteResult {
            started: false,
            error: Some(invalid("plan")),
            validation: None,
            state: wb.state(),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub fn workbench_cancel_run(wb: Engine) -> WorkbenchState {
    wb.cancel()
}

#[tauri::command]
#[specta::specta]
pub fn workbench_ack_override(
    wb: Engine,
    ack: WorkbenchOverrideAck,
) -> Result<WorkbenchState, String> {
    let ack = ack
        .sanitized()
        .ok_or_else(|| invalid("override acknowledgement"))?;
    Ok(wb.acknowledge_override(&ack))
}

#[tauri::command]
#[specta::specta]
pub async fn workbench_reconcile(wb: Engine<'_>) -> Result<WorkbenchReviewReport, String> {
    Ok(wb.reconcile().await)
}

#[tauri::command]
#[specta::specta]
pub fn workbench_resolve_review(
    wb: Engine,
    payload: WorkbenchResolveReviewPayload,
) -> Result<WorkbenchState, String> {
    let payload = payload
        .sanitized()
        .ok_or_else(|| invalid("review resolution"))?;
    Ok(wb.resolve_review(&payload))
}

/// Read-only: which of my listings a confirmed trade would close.
#[tauri::command]
#[specta::specta]
pub async fn workbench_match_trade(
    matcher: State<'_, Matcher<WfmHost>>,
    trade: WorkbenchTrade,
) -> Result<Vec<WorkbenchTradeMatch>, String> {
    Ok(matcher.match_trade(&trade).await)
}

/// Tells the Market view a listing lost `quantity` units (the amount sent to WFM's close call).
/// Contracts are not in the Market table, so only orders are announced.
pub fn announce_order_closed(app: &AppHandle, m: &WorkbenchTradeMatch) {
    if !matches!(m.kind, WorkbenchMatchKind::Order) {
        return;
    }
    let payload = serde_json::json!({ "order_id": m.order_id, "quantity": m.quantity });
    if let Err(e) = app.emit(ORDER_CLOSED_EVENT, payload) {
        tracing::warn!(
            "[Workbench] could not announce closed order {}: {e}",
            m.order_id
        );
    }
}

/// Closes one matched listing. Only called after the user confirms the match. With the ledger
/// row's id, a successful close is remembered on that row (`wfmClosed`).
#[tauri::command]
#[specta::specta]
pub async fn workbench_close_match(
    app: AppHandle,
    matcher: State<'_, Matcher<WfmHost>>,
    db: State<'_, Db>,
    trade_match: WorkbenchTradeMatch,
    event_id: Option<String>,
) -> Result<bool, String> {
    let closed = matcher.close_match(&trade_match).await;
    if closed {
        announce_order_closed(&app, &trade_match);
    }
    if let (true, Some(id)) = (closed, event_id)
        && let Err(e) = crate::services::ledger::mark_wfm_closed(&db.lock(), &id)
    {
        tracing::warn!("[Workbench] could not flag ledger row {id} as closed: {e:?}");
    }
    Ok(closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::wfm::client::WfmClient;
    use crate::services::wfm::mock::{reply, spawn};

    #[tokio::test]
    async fn own_riven_auctions_become_match_contracts_and_close_through_v1() {
        let row = r#"{"id":"a1","platinum":120,"item":{"weapon_url_name":"rubico","name":"visio-critatis"}}"#;
        let m = spawn(vec![
            reply(200, &format!(r#"{{"payload":{{"auctions":[{row}]}}}}"#)),
            reply(200, "{}"),
        ]);
        let c = WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        let page = contracts::my_contracts(&c, &Contracts::default(), Some("Me"), None, Some(100))
            .await
            .unwrap();
        let got: Vec<_> = page.contracts.into_iter().map(match_contract).collect();
        assert_eq!(
            got,
            [MatchContract {
                id: "a1".into(),
                weapon_url_name: Some("rubico".into()),
                riven_suffix: Some("visio-critatis".into()),
                item_thumb: None,
                platinum: 120,
            }]
        );
        legacy_v1::auction_close(&c, "a1").await.unwrap();
        assert!(m.requests()[1].starts_with("put /v1/auctions/entry/a1/close "));
    }
}
