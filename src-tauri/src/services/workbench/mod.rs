//! Trade workbench backend (port of WFHelper `tradeWorkbench.ts`, `tradeWfmMatcher.ts` and
//! `tradeWorkbenchIpc.ts`). Two halves:
//! - `engine` (here): executes a user-confirmed selling plan one row at a time against an order
//!   surface, journaling every intent before the request leaves so a crash forces a review.
//! - `matcher`: matches a confirmed in-game trade to the user's own WFM listings and closes them.
//!
//! Nothing here runs on its own: creating, repricing or closing an order happens only after an
//! explicit user action (WFM rules, `docs/wfm-auth.md` §4).

pub mod commands;
pub mod journal;
pub mod matcher;
pub mod types;

use crate::services::clock::now_ms_f64;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;

use super::wfm::errors::WfmError;
use super::wfm::orders::MyOrder;
use journal::{Entry, Outcome, OutcomeStatus, Request, Resolved};
use types::*;

/// On top of the scheduler budget: user-visible bulk runs stay deliberately slow.
pub const INTER_ROW_DELAY: Duration = Duration::from_millis(400);

pub struct NewSellOrder {
    pub item_id: String,
    pub platinum: u32,
    pub quantity: u32,
    pub rank: Option<u32>,
    pub subtype: Option<String>,
}

/// What the engine needs from the outside world. Production wires the WFM client and SQLite;
/// tests inject fakes so nothing touches the network.
pub trait WorkbenchHost: Send + Sync + 'static {
    fn create_order(
        &self,
        order: NewSellOrder,
    ) -> impl Future<Output = Result<String, WfmError>> + Send;
    fn update_order(
        &self,
        order_id: &str,
        platinum: u32,
        quantity: u32,
    ) -> impl Future<Output = Result<String, WfmError>> + Send;
    fn my_sell_orders(&self) -> impl Future<Output = Result<Vec<MyOrder>, WfmError>> + Send;
    fn item_id(&self, slug: &str) -> impl Future<Output = Result<Option<String>, WfmError>> + Send;
    fn with_db<R>(&self, f: impl FnOnce(&Connection) -> R) -> R;
    fn emit(&self, state: &WorkbenchState);
}

#[derive(Default)]
struct Inner {
    journal_error: Option<String>,
    run: Option<WorkbenchRunProgress>,
    running: bool,
    cancel: bool,
}

pub struct Workbench<H> {
    host: H,
    inter_row_delay: Duration,
    inner: Mutex<Inner>,
}

fn normalize_subtype(s: Option<&str>) -> Option<String> {
    let t = s?.trim().to_lowercase();
    (!t.is_empty() && t != "regular").then_some(t)
}

impl<H: WorkbenchHost> Workbench<H> {
    pub fn new(host: H, inter_row_delay: Duration) -> Self {
        let journal_error = host.with_db(|c| journal::load(c).err());
        let wb = Self {
            host,
            inter_row_delay,
            inner: Mutex::new(Inner {
                journal_error,
                ..Default::default()
            }),
        };
        let s = wb.state();
        if s.review_required {
            tracing::warn!(
                "[Workbench] starting in review mode ({} unsettled intent(s), {:?})",
                s.unsettled_count,
                s.journal_error
            );
        }
        wb
    }

    pub fn state(&self) -> WorkbenchState {
        let unsettled = self.host.with_db(journal::unsettled_count);
        let i = self.inner.lock().unwrap();
        let review_required = i.journal_error.is_some() || unsettled > 0;
        let phase = if i.running {
            if i.cancel {
                WorkbenchPhase::Cancelling
            } else {
                WorkbenchPhase::Running
            }
        } else if review_required {
            WorkbenchPhase::Review
        } else {
            WorkbenchPhase::Idle
        };
        WorkbenchState {
            phase,
            review_required,
            journal_error: i.journal_error.clone(),
            unsettled_count: unsettled,
            run: i.run.clone(),
        }
    }

    fn emit(&self) {
        self.host.emit(&self.state());
    }

    /// Edits the row of the active run (if any).
    fn progress(&self, row_id: &str, f: impl FnOnce(&mut WorkbenchRowProgress)) {
        let mut i = self.inner.lock().unwrap();
        if let Some(row) = i
            .run
            .as_mut()
            .and_then(|r| r.rows.iter_mut().find(|p| p.row_id == row_id))
        {
            f(row);
        }
    }

    fn mark_remaining(&self, plan: &WorkbenchPlan, from: usize, status: WorkbenchRowStatus) {
        for row in &plan.rows[from..] {
            self.progress(&row.row_id, |p| {
                if p.status == WorkbenchRowStatus::Pending {
                    p.status = status;
                }
            });
        }
    }

    fn journal_intent(
        &self,
        plan: &WorkbenchPlan,
        row: &WorkbenchPlanRow,
        item_id: Option<String>,
    ) -> Result<String, String> {
        let entry = Entry {
            intent_id: uuid::Uuid::new_v4().to_string(),
            plan_id: plan.plan_id.clone(),
            row_id: row.row_id.clone(),
            mode: row.mode,
            request: Request {
                slug: row.slug.clone(),
                item_name: row.item_name.clone(),
                platinum: row.platinum,
                quantity: row.quantity,
                rank: row.rank,
                subtype: row.subtype.clone(),
                order_id: row.order_id.clone(),
                item_id,
            },
            created_at: now_ms_f64(),
            outcome: None,
            resolved: None,
        };
        self.host
            .with_db(|c| journal::append(c, &entry))
            .map_err(|e| e.to_string())?;
        Ok(entry.intent_id)
    }

    fn journal_outcome(
        &self,
        intent_id: &str,
        status: OutcomeStatus,
        order_id: Option<String>,
        error: Option<String>,
    ) {
        // The mutation already happened; a failed outcome write leaves the entry unsettled, so
        // the next start forces review instead of losing it.
        let res = self.host.with_db(|c| {
            let Some(mut e) = journal::get(c, intent_id) else {
                return Ok(());
            };
            e.outcome = Some(Outcome {
                status,
                order_id,
                error,
                at: now_ms_f64(),
            });
            journal::update(c, &e)
        });
        if let Err(e) = res {
            tracing::error!("[Workbench] failed to persist outcome: {e}");
        }
    }

    async fn run_plan(self: Arc<Self>, plan: WorkbenchPlan) {
        let mut stop = WorkbenchStopReason::Completed;
        for (index, row) in plan.rows.iter().enumerate() {
            if self.inner.lock().unwrap().cancel {
                self.mark_remaining(&plan, index, WorkbenchRowStatus::Cancelled);
                stop = WorkbenchStopReason::Cancelled;
                break;
            }
            self.progress(&row.row_id, |p| p.status = WorkbenchRowStatus::InFlight);
            self.emit();

            let fail = |msg: String| {
                self.progress(&row.row_id, |p| {
                    p.status = WorkbenchRowStatus::Failed;
                    p.error = Some(msg);
                });
            };
            let mut item_id = None;
            if row.mode == WorkbenchRowMode::Create {
                item_id = match self.host.item_id(&row.slug).await {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::warn!("[Workbench] catalog lookup failed for {}: {e}", row.slug);
                        None
                    }
                };
                if item_id.is_none() {
                    fail(format!("No warframe.market item for \"{}\"", row.slug));
                    self.emit();
                    continue;
                }
            }

            // Write-ahead: the intent must be durable before the request leaves, or a crash in
            // flight would leave an untracked live mutation.
            let intent_id = match self.journal_intent(&plan, row, item_id.clone()) {
                Ok(id) => id,
                Err(e) => {
                    fail(format!("journal write failed: {e}"));
                    self.mark_remaining(&plan, index + 1, WorkbenchRowStatus::Blocked);
                    stop = WorkbenchStopReason::Error;
                    self.emit();
                    break;
                }
            };

            // An update resent is idempotent (same price and quantity), so a lost answer stays Failed.
            let creating = !matches!(
                (&row.order_id, row.mode),
                (Some(_), WorkbenchRowMode::Update)
            );
            let sent = match (&row.order_id, row.mode) {
                (Some(id), WorkbenchRowMode::Update) => {
                    self.host.update_order(id, row.platinum, row.quantity).await
                }
                _ => {
                    self.host
                        .create_order(NewSellOrder {
                            item_id: item_id.unwrap_or_default(),
                            platinum: row.platinum,
                            quantity: row.quantity,
                            rank: row.rank,
                            subtype: row.subtype.clone(),
                        })
                        .await
                }
            };
            match sent {
                Ok(id) => {
                    self.journal_outcome(
                        &intent_id,
                        OutcomeStatus::Confirmed,
                        Some(id.clone()),
                        None,
                    );
                    self.progress(&row.row_id, |p| {
                        p.status = WorkbenchRowStatus::Done;
                        p.order_id = Some(id);
                    });
                }
                Err(err) if creating && err.outcome_unknown() => {
                    // WFM may have created it anyway: no outcome keeps the intent unsettled, so
                    // reconcile checks our orders and a re-click cannot list it twice.
                    fail(format!(
                        "No clear answer from warframe.market ({err}). The order may have been created: check the review before selling it again."
                    ));
                }
                Err(err) => {
                    // A subtype the user never chose is not something a bulk run may guess, so
                    // the row is reported with the choices and left for the order dialog.
                    let message = match &err {
                        WfmError::SubtypeRequired { subtypes } if !subtypes.is_empty() => format!(
                            "Pick a subtype for \"{}\" on warframe.market: {}",
                            row.item_name,
                            subtypes.join(", ")
                        ),
                        e => e.to_string(),
                    };
                    self.journal_outcome(
                        &intent_id,
                        OutcomeStatus::Failed,
                        None,
                        Some(message.clone()),
                    );
                    fail(message.clone());
                    // Account-tier order cap / dead session: every further row hits the same wall.
                    let halt = if is_order_limit_message(&message) {
                        Some(WorkbenchStopReason::OrderLimit)
                    } else if matches!(err, WfmError::Unauthorized { .. }) {
                        Some(WorkbenchStopReason::Auth)
                    } else {
                        None
                    };
                    if let Some(reason) = halt {
                        self.mark_remaining(&plan, index + 1, WorkbenchRowStatus::Blocked);
                        stop = reason;
                        self.emit();
                        break;
                    }
                }
            }
            self.emit();
            if index + 1 < plan.rows.len() {
                tokio::time::sleep(self.inter_row_delay).await;
            }
        }

        let done = {
            let mut i = self.inner.lock().unwrap();
            if let Some(run) = i.run.as_mut() {
                run.finished_at = Some(now_ms_f64());
                run.stop_reason = Some(stop);
            }
            i.running = false;
            i.cancel = false;
            i.run.as_ref().map_or(0, |r| {
                r.rows
                    .iter()
                    .filter(|p| p.status == WorkbenchRowStatus::Done)
                    .count()
            })
        };
        self.emit();
        tracing::info!(
            "[Workbench] run {} finished: {stop:?} ({done}/{} done)",
            plan.plan_id,
            plan.rows.len()
        );
    }

    /// Validates and starts a run in the background. Never blocks on WFM.
    pub fn execute(
        self: &Arc<Self>,
        plan: WorkbenchPlan,
        safety: &WorkbenchSafetySnapshot,
    ) -> WorkbenchExecuteResult {
        let state = self.state();
        let refuse = |error: &str, validation| WorkbenchExecuteResult {
            started: false,
            error: Some(error.into()),
            validation,
            state: state.clone(),
        };
        if self.inner.lock().unwrap().running {
            return refuse("A workbench run is already active.", None);
        }
        if state.review_required {
            return refuse(
                "Unreconciled journal entries require review before executing.",
                None,
            );
        }
        let validation = validate_plan(&plan, safety);
        if !validation.ok {
            return refuse("Plan failed safety validation.", Some(validation));
        }
        if let Some(ids) = plan.known_order_ids.as_deref().filter(|i| !i.is_empty())
            && let Err(e) = self
                .host
                .with_db(|c| journal::set_preexisting(c, &plan.plan_id, ids))
        {
            return refuse(&format!("journal write failed: {e}"), None);
        }
        {
            let mut i = self.inner.lock().unwrap();
            i.run = Some(WorkbenchRunProgress {
                plan_id: plan.plan_id.clone(),
                started_at: now_ms_f64(),
                finished_at: None,
                stop_reason: None,
                rows: plan
                    .rows
                    .iter()
                    .map(|r| WorkbenchRowProgress {
                        row_id: r.row_id.clone(),
                        item_name: r.item_name.clone(),
                        status: WorkbenchRowStatus::Pending,
                        order_id: None,
                        error: None,
                    })
                    .collect(),
            });
            i.running = true;
            i.cancel = false;
        }
        self.emit();
        tauri::async_runtime::spawn(Arc::clone(self).run_plan(plan));
        WorkbenchExecuteResult {
            started: true,
            error: None,
            validation: Some(validation),
            state: self.state(),
        }
    }

    /// Cancels after the in-flight operation settles; nothing is aborted mid-request.
    pub fn cancel(&self) -> WorkbenchState {
        {
            let mut i = self.inner.lock().unwrap();
            if !i.running {
                drop(i);
                return self.state();
            }
            i.cancel = true;
        }
        self.emit();
        self.state()
    }

    pub fn acknowledge_override(&self, ack: &WorkbenchOverrideAck) -> WorkbenchState {
        if let Err(e) = self.host.with_db(|c| journal::push_override(c, ack)) {
            tracing::warn!("[Workbench] failed to persist override ack: {e}");
        }
        self.state()
    }

    /// Re-fetches our own orders and classifies unsettled intents. Read-only: the journal is only
    /// settled by `resolve_review`, after the user confirms.
    pub async fn reconcile(&self) -> WorkbenchReviewReport {
        let unsettled = self.host.with_db(journal::unsettled).unwrap_or_default();
        let (mut sell, mut fetch_error) = (vec![], None);
        if !unsettled.is_empty() {
            match self.host.my_sell_orders().await {
                Ok(o) => sell = o,
                Err(e) => fetch_error = Some(e.to_string()),
            }
        }
        let rows = unsettled
            .into_iter()
            .map(|e| {
                let (classification, matched_order_id) = if fetch_error.is_some() {
                    (WorkbenchReviewClassification::Unknown, None)
                } else {
                    let known = self.host.with_db(|c| journal::preexisting(c, &e.plan_id));
                    classify(&e, &sell, &known)
                };
                WorkbenchReviewRow {
                    intent_id: e.intent_id,
                    plan_id: e.plan_id,
                    row_id: e.row_id,
                    mode: e.mode,
                    item_name: e.request.item_name,
                    slug: e.request.slug,
                    platinum: e.request.platinum,
                    quantity: e.request.quantity,
                    created_at: e.created_at,
                    classification,
                    matched_order_id,
                }
            })
            .collect();
        WorkbenchReviewReport {
            generated_at: now_ms_f64(),
            rows,
            fetch_error,
        }
    }

    pub fn resolve_review(&self, payload: &WorkbenchResolveReviewPayload) -> WorkbenchState {
        let journal_error = self.inner.lock().unwrap().journal_error.clone();
        if journal_error.is_some() && payload.reset_corrupt_journal == Some(true) {
            // Explicit user consent: replace the unreadable journal with a fresh one.
            match self.host.with_db(journal::reset) {
                Ok(()) => {
                    self.inner.lock().unwrap().journal_error = None;
                    tracing::warn!(
                        "[Workbench] corrupt journal replaced after explicit review confirmation"
                    );
                }
                Err(e) => {
                    self.inner.lock().unwrap().journal_error =
                        Some(format!("journal reset failed: {e}"));
                }
            }
            self.emit();
            return self.state();
        }
        for r in &payload.resolutions {
            // "unknown" is the absence of a verdict, so it leaves the intent open; only an
            // explicit confirmed/failed from the user settles a row.
            if r.classification == WorkbenchReviewClassification::Unknown {
                continue;
            }
            let res = self.host.with_db(|c| {
                match journal::get(c, &r.intent_id).filter(|e| !e.is_settled()) {
                    Some(mut e) => {
                        e.resolved = Some(Resolved {
                            classification: r.classification,
                            at: now_ms_f64(),
                        });
                        journal::update(c, &e)
                    }
                    None => Ok(()),
                }
            });
            if let Err(e) = res {
                tracing::error!("[Workbench] failed to persist review resolution: {e}");
            }
        }
        self.emit();
        self.state()
    }
}

fn classify(
    e: &Entry,
    sell: &[MyOrder],
    known: &std::collections::HashSet<String>,
) -> (WorkbenchReviewClassification, Option<String>) {
    use WorkbenchReviewClassification::*;
    let r = &e.request;
    if e.mode == WorkbenchRowMode::Update {
        let Some(o) = sell.iter().find(|o| Some(&o.id) == r.order_id.as_ref()) else {
            return (Unknown, None);
        };
        if o.platinum != r.platinum {
            return (Failed, None);
        }
        // A short quantity can mean the update never applied or that units sold after it did, so
        // the price alone never proves the whole update landed.
        if o.quantity != r.quantity {
            return (Unknown, None);
        }
        return (Confirmed, Some(o.id.clone()));
    }
    let found = sell.iter().find(|o| {
        // An order that was already there before the run proves nothing about a create.
        !known.contains(&o.id)
            && o.item_slug.as_deref() == Some(r.slug.as_str())
            && o.platinum == r.platinum
            && o.quantity == r.quantity
            && (r.rank.is_none() || o.mod_rank == r.rank)
            && normalize_subtype(o.subtype.as_deref()) == normalize_subtype(r.subtype.as_deref())
    });
    // Absence proves nothing: the create may have failed, or landed and sold out.
    found.map_or((Unknown, None), |o| (Confirmed, Some(o.id.clone())))
}

#[cfg(test)]
mod tests;
