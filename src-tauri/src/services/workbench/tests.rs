//! Ports of WFHelper `tradeWorkbench.test.ts` and `tradeWfmMatcher.test.ts`. The Electron-only parts
//! (prototype-pollution of plain objects, the on-disk JSON file) become their SQLite / HashMap
//! equivalents; every behavioural case keeps its name.

use crate::db::mem_conn;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;

use super::matcher::*;
use super::*;
use crate::services::gdpr_import::{Direction, TradeType};
use crate::services::wfm::catalog::{SetLookup, SetPart};
use crate::services::wfm::orders::MyOrders;

type BoxFut<T> = Pin<Box<dyn Future<Output = T> + Send>>;

fn fut<T: Send + 'static>(v: T) -> BoxFut<T> {
    Box::pin(async move { v })
}

fn order(id: &str, platinum: u32, quantity: u32, name: &str, slug: &str) -> MyOrder {
    MyOrder {
        id: id.into(),
        order_type: "sell".into(),
        platinum,
        quantity,
        per_trade: 1,
        visible: true,
        mod_rank: None,
        subtype: None,
        item_id: None,
        item_name: name.into(),
        item_slug: Some(slug.into()),
        item_thumb: None,
        created_at: None,
        updated_at: None,
    }
}

// ---------------------------------------------------------------- plan validation and parsing

fn row(id: &str) -> WorkbenchPlanRow {
    WorkbenchPlanRow {
        row_id: id.into(),
        mode: WorkbenchRowMode::Create,
        slug: format!("slug_{id}"),
        item_name: format!("Item {id}"),
        quantity: 1,
        platinum: 10,
        rank: None,
        subtype: None,
        order_id: None,
        override_ack: None,
    }
}

fn plan(rows: Vec<WorkbenchPlanRow>) -> WorkbenchPlan {
    WorkbenchPlan {
        plan_id: "plan-1".into(),
        created_at: 1000.0,
        rows,
        known_order_ids: None,
    }
}

fn snapshot(plan: &WorkbenchPlan, safe: u32, total: u32) -> WorkbenchSafetySnapshot {
    WorkbenchSafetySnapshot {
        captured_at: 2000.0,
        rows: plan
            .rows
            .iter()
            .map(|r| (r.row_id.clone(), WorkbenchSafetyRow { safe, total }))
            .collect(),
    }
}

fn ack_override() -> Option<WorkbenchOverride> {
    Some(WorkbenchOverride {
        acknowledged_at: 1.0,
        reason_keys: vec!["k".into()],
    })
}

#[test]
fn rejects_quantities_above_the_fresh_safety_snapshot_without_an_override() {
    let p = plan(vec![WorkbenchPlanRow {
        quantity: 5,
        ..row("a")
    }]);
    let v = validate_plan(&p, &snapshot(&p, 3, 10));
    assert!(!v.ok);
    assert_eq!(
        v.rows[0],
        WorkbenchRowValidation {
            row_id: "a".into(),
            ok: false,
            reason: Some(WorkbenchRowRejection::OverSafe)
        }
    );
}

#[test]
fn accepts_an_over_safe_quantity_only_with_a_recorded_override_capped_at_total() {
    let p = plan(vec![WorkbenchPlanRow {
        quantity: 5,
        override_ack: ack_override(),
        ..row("a")
    }]);
    assert!(validate_plan(&p, &snapshot(&p, 3, 10)).ok);
    // Beyond the account total nothing is sellable, override or not.
    assert_eq!(
        validate_plan(&p, &snapshot(&p, 3, 4)).rows[0].reason,
        Some(WorkbenchRowRejection::OverTotal)
    );
}

#[test]
fn rejects_a_plan_over_the_per_run_cap_and_rows_missing_from_the_snapshot() {
    let many = |n| plan((0..n).map(|i| row(&format!("r{i}"))).collect());
    let at_cap = many(MAX_ROWS_PER_RUN);
    assert_eq!(MAX_ROWS_PER_RUN, 100);
    assert!(validate_plan(&at_cap, &snapshot(&at_cap, 99, 99)).ok);

    let big = many(MAX_ROWS_PER_RUN + 1);
    assert_eq!(
        validate_plan(&big, &snapshot(&big, 99, 99)).plan_error,
        Some(WorkbenchPlanError::TooManyRows)
    );
    // The parse bound sits above the cap so the validator reports this, not the parser.
    assert!(big.clone().sanitized().is_some());
    assert!(many(MAX_ROWS_PER_RUN + 11).sanitized().is_none());

    let p = plan(vec![row("a")]);
    let empty = WorkbenchSafetySnapshot {
        captured_at: 1.0,
        rows: HashMap::new(),
    };
    assert_eq!(
        validate_plan(&p, &empty).rows[0].reason,
        Some(WorkbenchRowRejection::MissingSafety)
    );
    assert_eq!(
        validate_plan(&plan(vec![]), &empty).plan_error,
        Some(WorkbenchPlanError::Empty)
    );
    assert_eq!(
        validate_plan(&plan(vec![row("a"), row("a")]), &empty).plan_error,
        Some(WorkbenchPlanError::DuplicateRowIds)
    );
}

#[test]
fn parses_only_well_formed_plans() {
    let update_without_order = WorkbenchPlanRow {
        mode: WorkbenchRowMode::Update,
        ..row("a")
    };
    assert!(plan(vec![update_without_order]).sanitized().is_none());
    assert!(
        plan(vec![WorkbenchPlanRow {
            slug: "  ".into(),
            ..row("a")
        }])
        .sanitized()
        .is_none()
    );
    assert!(
        plan(vec![WorkbenchPlanRow {
            quantity: 0,
            ..row("a")
        }])
        .sanitized()
        .is_none()
    );
    assert!(
        plan(vec![WorkbenchPlanRow {
            platinum: 100_001,
            ..row("a")
        }])
        .sanitized()
        .is_none()
    );

    let good = plan(vec![WorkbenchPlanRow {
        slug: " s ".into(),
        rank: Some(11),
        ..row("a")
    }])
    .sanitized()
    .unwrap();
    assert_eq!(good.rows[0].slug, "s");
    assert_eq!(good.rows[0].rank, None);

    let json = r#"{"planId":"p","createdAt":1,"rows":[{"rowId":"a","mode":"create","slug":"s","itemName":"n","quantity":1,"platinum":2,"override":null}]}"#;
    assert!(
        serde_json::from_str::<WorkbenchPlan>(json)
            .unwrap()
            .sanitized()
            .is_some()
    );
    let negative = json.replace("\"quantity\":1", "\"quantity\":-1");
    assert!(serde_json::from_str::<WorkbenchPlan>(&negative).is_err());
}

#[test]
fn a_row_id_named_like_a_map_member_is_just_a_missing_safety_row() {
    let p = plan(vec![WorkbenchPlanRow {
        quantity: 5,
        ..row("toString")
    }]);
    let empty = WorkbenchSafetySnapshot {
        captured_at: 1.0,
        rows: HashMap::new(),
    };
    assert_eq!(
        validate_plan(&p, &empty).rows[0].reason,
        Some(WorkbenchRowRejection::MissingSafety)
    );
    assert_eq!(
        validate_plan(&p, &snapshot(&p, 1, 1)).rows[0].reason,
        Some(WorkbenchRowRejection::OverTotal)
    );
}

#[test]
fn recognizes_order_limit_phrasings_and_nothing_else() {
    assert!(is_order_limit_message("app.post_order.limit_exceeded"));
    assert!(is_order_limit_message(
        "You have reached the maximum number of orders"
    ));
    assert!(is_order_limit_message("too many orders"));
    assert!(!is_order_limit_message("invalid item"));
    // A rate limit is transient and scheduler-handled; it must never read as the account cap.
    assert!(!is_order_limit_message(
        "Warframe.market rate limit hit. Please wait 30s."
    ));
}

// ---------------------------------------------------------------- execution engine

type CreateFn =
    Box<dyn Fn(&FakeWb, NewSellOrder) -> BoxFut<Result<String, WfmError>> + Send + Sync>;

struct FakeWb {
    db: Mutex<Connection>,
    calls: Mutex<Vec<String>>,
    counter: AtomicU32,
    create: CreateFn,
    sell: Mutex<Vec<MyOrder>>,
}

impl FakeWb {
    fn with(create: CreateFn) -> Arc<Self> {
        Arc::new(Self {
            db: Mutex::new(mem_conn()),
            calls: Mutex::default(),
            counter: AtomicU32::new(0),
            create,
            sell: Mutex::default(),
        })
    }

    fn new() -> Arc<Self> {
        Self::with(Box::new(|f, p| {
            f.calls.lock().unwrap().push(format!(
                "create:{}:{}x{}",
                p.item_id, p.platinum, p.quantity
            ));
            let n = f.counter.fetch_add(1, Ordering::SeqCst) + 1;
            fut(Ok(format!("order-{n}")))
        }))
    }

    fn entries(&self) -> Vec<journal::Entry> {
        journal::load(&self.db.lock().unwrap()).unwrap()
    }

    fn insert_raw(&self, intent_id: &str, entry: &str) {
        self.db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO workbench_intents (intent_id, plan_id, entry) VALUES (?1, 'p', ?2)",
                [intent_id, entry],
            )
            .unwrap();
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl WorkbenchHost for Arc<FakeWb> {
    fn create_order(
        &self,
        o: NewSellOrder,
    ) -> impl Future<Output = Result<String, WfmError>> + Send {
        (self.create)(self, o)
    }
    fn update_order(
        &self,
        id: &str,
        _platinum: u32,
        _quantity: u32,
    ) -> impl Future<Output = Result<String, WfmError>> + Send {
        self.calls.lock().unwrap().push(format!("update:{id}"));
        fut(Ok(id.to_owned()))
    }
    fn my_sell_orders(&self) -> impl Future<Output = Result<Vec<MyOrder>, WfmError>> + Send {
        fut(Ok(self.sell.lock().unwrap().clone()))
    }
    fn item_id(&self, slug: &str) -> impl Future<Output = Result<Option<String>, WfmError>> + Send {
        fut(Ok(Some(format!("id-{slug}"))))
    }
    fn with_db<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        f(&self.db.lock().unwrap())
    }
    fn emit(&self, _state: &WorkbenchState) {}
}

fn engine(fake: &Arc<FakeWb>) -> Arc<Workbench<Arc<FakeWb>>> {
    Arc::new(Workbench::new(fake.clone(), Duration::ZERO))
}

async fn wait_finish(wb: &Workbench<Arc<FakeWb>>) {
    for _ in 0..1000 {
        if wb.state().run.is_some_and(|r| r.finished_at.is_some()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("workbench run did not finish");
}

fn statuses(wb: &Workbench<Arc<FakeWb>>) -> Vec<WorkbenchRowStatus> {
    wb.state()
        .run
        .unwrap()
        .rows
        .iter()
        .map(|r| r.status)
        .collect()
}

fn stop_reason(wb: &Workbench<Arc<FakeWb>>) -> Option<WorkbenchStopReason> {
    wb.state().run.unwrap().stop_reason
}

fn run_three(fake: &Arc<FakeWb>) -> (Arc<Workbench<Arc<FakeWb>>>, WorkbenchPlan) {
    let wb = engine(fake);
    let p = plan(vec![row("a"), row("b"), row("c")]);
    assert!(wb.execute(p.clone(), &snapshot(&p, 99, 99)).started);
    (wb, p)
}

#[tokio::test]
async fn journals_the_intent_before_the_request_leaves() {
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    let fake = FakeWb::with(Box::new(move |f, p| {
        let entries = journal::load(&f.db.lock().unwrap()).unwrap();
        s.lock().unwrap().push((
            entries.len(),
            entries.iter().filter(|e| e.outcome.is_none()).count(),
        ));
        fut(Ok(format!("order-{}", p.item_id)))
    }));
    let wb = engine(&fake);
    let p = plan(vec![row("a"), row("b")]);
    assert!(wb.execute(p.clone(), &snapshot(&p, 99, 99)).started);
    wait_finish(&wb).await;

    // At each send exactly one entry (the current one) was unsettled in the journal.
    assert_eq!(*seen.lock().unwrap(), [(1, 1), (2, 1)]);
    let entries = fake.entries();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .all(|e| e.outcome.as_ref().unwrap().status == OutcomeStatus::Confirmed)
    );
    assert!(!wb.state().review_required);
}

#[tokio::test]
async fn executes_strictly_sequentially_in_plan_order() {
    let (active, max_active) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)));
    let order = Arc::new(Mutex::new(vec![]));
    let (a, m, o) = (active.clone(), max_active.clone(), order.clone());
    let fake = FakeWb::with(Box::new(move |_, p| {
        let (a, m, o) = (a.clone(), m.clone(), o.clone());
        Box::pin(async move {
            let now = a.fetch_add(1, Ordering::SeqCst) + 1;
            m.fetch_max(now, Ordering::SeqCst);
            o.lock().unwrap().push(p.item_id.clone());
            tokio::time::sleep(Duration::from_millis(10)).await;
            a.fetch_sub(1, Ordering::SeqCst);
            Ok(format!("order-{}", p.item_id))
        })
    }));
    let (wb, _) = run_three(&fake);
    wait_finish(&wb).await;

    assert_eq!(max_active.load(Ordering::SeqCst), 1);
    assert_eq!(
        *order.lock().unwrap(),
        ["id-slug_a", "id-slug_b", "id-slug_c"]
    );
    assert_eq!(stop_reason(&wb), Some(WorkbenchStopReason::Completed));
}

#[tokio::test]
async fn cancel_stops_after_the_in_flight_operation_and_marks_the_rest_cancelled() {
    let started = Arc::new(tokio::sync::Notify::new());
    let sends = Arc::new(AtomicU32::new(0));
    let (st, se) = (started.clone(), sends.clone());
    let fake = FakeWb::with(Box::new(move |_, p| {
        let (st, se) = (st.clone(), se.clone());
        Box::pin(async move {
            se.fetch_add(1, Ordering::SeqCst);
            st.notify_one();
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok(format!("order-{}", p.item_id))
        })
    }));
    let (wb, _) = run_three(&fake);
    started.notified().await;
    assert_eq!(wb.cancel().phase, WorkbenchPhase::Cancelling);
    wait_finish(&wb).await;

    assert_eq!(stop_reason(&wb), Some(WorkbenchStopReason::Cancelled));
    assert_eq!(
        statuses(&wb),
        [
            WorkbenchRowStatus::Done,
            WorkbenchRowStatus::Cancelled,
            WorkbenchRowStatus::Cancelled
        ]
    );
    assert_eq!(sends.load(Ordering::SeqCst), 1);
    // The finished first row settled its journal entry despite the cancel.
    let entries = fake.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].outcome.as_ref().unwrap().status,
        OutcomeStatus::Confirmed
    );
}

#[tokio::test]
async fn stops_on_the_order_limit_error_and_blocks_remaining_rows_without_retrying() {
    let creates = Arc::new(AtomicU32::new(0));
    let c = creates.clone();
    let fake = FakeWb::with(Box::new(move |_, p| {
        if c.fetch_add(1, Ordering::SeqCst) + 1 >= 2 {
            return fut(Err(WfmError::invalid(
                "WFMClient v2 API error: app.post_order.limit_exceeded",
            )));
        }
        fut(Ok(format!("order-{}", p.item_id)))
    }));
    let (wb, _) = run_three(&fake);
    wait_finish(&wb).await;

    assert_eq!(stop_reason(&wb), Some(WorkbenchStopReason::OrderLimit));
    assert_eq!(
        statuses(&wb),
        [
            WorkbenchRowStatus::Done,
            WorkbenchRowStatus::Failed,
            WorkbenchRowStatus::Blocked
        ]
    );
    assert_eq!(creates.load(Ordering::SeqCst), 2);
    // The blocked row never produced an intent; the failed one is settled.
    let entries = fake.entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[1].outcome.as_ref().unwrap().status,
        OutcomeStatus::Failed
    );
    assert!(!wb.state().review_required);
}

#[tokio::test]
async fn stops_when_the_session_dies_mid_run() {
    let fake = FakeWb::with(Box::new(|_, _| {
        fut(Err(WfmError::Unauthorized {
            detail: "Warframe.market session expired or invalid.".into(),
        }))
    }));
    let wb = engine(&fake);
    let p = plan(vec![row("a"), row("b")]);
    wb.execute(p.clone(), &snapshot(&p, 99, 99));
    wait_finish(&wb).await;
    assert_eq!(stop_reason(&wb), Some(WorkbenchStopReason::Auth));
    assert_eq!(
        statuses(&wb),
        [WorkbenchRowStatus::Failed, WorkbenchRowStatus::Blocked]
    );
}

#[tokio::test]
async fn fails_a_row_that_needs_a_subtype_names_the_choices_and_moves_on() {
    let creates = Arc::new(AtomicU32::new(0));
    let c = creates.clone();
    let fake = FakeWb::with(Box::new(move |_, p| {
        c.fetch_add(1, Ordering::SeqCst);
        if p.item_id != "id-slug_a" {
            return fut(Ok("order-b".into()));
        }
        fut(Err(WfmError::SubtypeRequired {
            subtypes: vec!["intact".into(), "radiant".into()],
        }))
    }));
    let wb = engine(&fake);
    let p = plan(vec![row("a"), row("b")]);
    wb.execute(p.clone(), &snapshot(&p, 99, 99));
    wait_finish(&wb).await;

    assert_eq!(stop_reason(&wb), Some(WorkbenchStopReason::Completed));
    assert_eq!(
        statuses(&wb),
        [WorkbenchRowStatus::Failed, WorkbenchRowStatus::Done]
    );
    let run = wb.state().run.unwrap();
    assert!(
        run.rows[0]
            .error
            .as_ref()
            .unwrap()
            .contains("intact, radiant")
    );
    // One attempt for the failed row, one for the next: no retry.
    assert_eq!(creates.load(Ordering::SeqCst), 2);
    assert!(
        fake.entries()[0]
            .outcome
            .as_ref()
            .unwrap()
            .error
            .as_ref()
            .unwrap()
            .contains("intact, radiant")
    );
}

#[tokio::test]
async fn refuses_the_whole_plan_when_any_row_exceeds_its_fresh_safety_cap() {
    let fake = FakeWb::new();
    let wb = engine(&fake);
    let p = plan(vec![
        row("a"),
        WorkbenchPlanRow {
            quantity: 4,
            ..row("b")
        },
    ]);
    let r = wb.execute(p.clone(), &snapshot(&p, 2, 10));
    assert!(!r.started);
    let reason = r
        .validation
        .unwrap()
        .rows
        .iter()
        .find(|x| x.row_id == "b")
        .unwrap()
        .reason;
    assert_eq!(reason, Some(WorkbenchRowRejection::OverSafe));
    assert!(fake.calls().is_empty());
    assert!(fake.entries().is_empty());
}

#[tokio::test]
async fn uses_the_existing_order_surface_for_update_rows() {
    let fake = FakeWb::new();
    let wb = engine(&fake);
    let p = plan(vec![WorkbenchPlanRow {
        mode: WorkbenchRowMode::Update,
        order_id: Some("ord-9".into()),
        platinum: 33,
        ..row("a")
    }]);
    wb.execute(p.clone(), &snapshot(&p, 99, 99));
    wait_finish(&wb).await;
    assert_eq!(fake.calls(), ["update:ord-9"]);
    assert_eq!(
        fake.entries()[0]
            .outcome
            .as_ref()
            .unwrap()
            .order_id
            .as_deref(),
        Some("ord-9")
    );
}

// ---------------------------------------------------------------- journal recovery and review

#[tokio::test]
async fn a_crash_between_request_and_response_forces_review_and_classifies_unknown() {
    let fake = FakeWb::with(Box::new(|_, _| Box::pin(std::future::pending())));
    let wb = engine(&fake);
    let p = plan(vec![row("a")]);
    wb.execute(p.clone(), &snapshot(&p, 99, 99));
    for _ in 0..1000 {
        if !fake.entries().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(fake.entries().len(), 1);

    // Restart: a fresh engine over the same database.
    let restarted = engine(&fake);
    let state = restarted.state();
    assert!(state.review_required);
    assert_eq!(state.phase, WorkbenchPhase::Review);
    assert_eq!(state.unsettled_count, 1);

    let blocked = restarted.execute(p.clone(), &snapshot(&p, 99, 99));
    assert!(!blocked.started);
    assert!(blocked.error.unwrap().to_lowercase().contains("review"));

    let report = restarted.reconcile().await;
    assert_eq!(report.rows.len(), 1);
    assert_eq!(
        report.rows[0].classification,
        WorkbenchReviewClassification::Unknown
    );

    let resolved = restarted.resolve_review(&WorkbenchResolveReviewPayload {
        resolutions: vec![WorkbenchReviewResolution {
            intent_id: report.rows[0].intent_id.clone(),
            classification: WorkbenchReviewClassification::Failed,
        }],
        reset_corrupt_journal: None,
    });
    assert!(!resolved.review_required);
    assert_eq!(
        fake.entries()[0].resolved.as_ref().unwrap().classification,
        WorkbenchReviewClassification::Failed
    );
}

fn api(status: u16) -> WfmError {
    WfmError::Api {
        status,
        detail: "x".into(),
    }
}

fn net() -> WfmError {
    WfmError::Network { detail: "x".into() }
}

async fn run_one_failing_create(err: WfmError) -> (Arc<FakeWb>, Arc<Workbench<Arc<FakeWb>>>) {
    let fake = FakeWb::with(Box::new(move |_, _| fut(Err(err.clone()))));
    let wb = engine(&fake);
    let p = plan(vec![row("a")]);
    wb.execute(p.clone(), &snapshot(&p, 99, 99));
    wait_finish(&wb).await;
    (fake, wb)
}

#[tokio::test]
async fn a_create_with_an_unknown_result_stays_unsettled_and_blocks_a_new_run() {
    for err in [net(), api(500), api(503)] {
        let label = err.to_string();
        let (fake, wb) = run_one_failing_create(err).await;
        assert_eq!(statuses(&wb), [WorkbenchRowStatus::Failed], "{label}");
        let row_error = wb.state().run.unwrap().rows[0].error.clone().unwrap();
        assert!(
            row_error.contains("check the review"),
            "{label}: {row_error}"
        );
        assert!(fake.entries()[0].outcome.is_none(), "{label}");
        let state = wb.state();
        assert!(state.review_required, "{label}");
        assert_eq!(state.unsettled_count, 1, "{label}");

        let p = plan(vec![row("a")]);
        let again = wb.execute(p.clone(), &snapshot(&p, 99, 99));
        assert!(!again.started, "{label}");
        assert!(again.error.unwrap().to_lowercase().contains("review"));
        assert_eq!(fake.calls().len(), 0, "{label}: no second create left");

        // Resolving the review is what unlocks the next run.
        let report = wb.reconcile().await;
        wb.resolve_review(&WorkbenchResolveReviewPayload {
            resolutions: vec![WorkbenchReviewResolution {
                intent_id: report.rows[0].intent_id.clone(),
                classification: WorkbenchReviewClassification::Failed,
            }],
            reset_corrupt_journal: None,
        });
        assert!(!wb.state().review_required, "{label}");
        assert!(
            wb.execute(p.clone(), &snapshot(&p, 99, 99)).started,
            "{label}"
        );
    }
}

#[tokio::test]
async fn reconcile_of_an_unknown_create_matches_the_order_when_wfm_has_it_and_not_when_it_does_not()
{
    for err in [net(), api(500)] {
        for (server_has_it, expected) in [
            (true, WorkbenchReviewClassification::Confirmed),
            (false, WorkbenchReviewClassification::Unknown),
        ] {
            let (fake, wb) = run_one_failing_create(err.clone()).await;
            // row("a") lists slug_a at the plan's price and quantity.
            let sent = fake.entries()[0].request.clone();
            if server_has_it {
                *fake.sell.lock().unwrap() =
                    vec![live("ord-1", sent.platinum, sent.quantity, &sent.slug)];
            }
            let report = wb.reconcile().await;
            let r = &report.rows[0];
            assert_eq!(r.classification, expected, "{err} has_it={server_has_it}");
            assert_eq!(
                r.matched_order_id.as_deref(),
                server_has_it.then_some("ord-1")
            );
            assert!(wb.state().review_required, "reconcile settles nothing");
        }
    }
}

#[tokio::test]
async fn a_create_that_wfm_rejected_is_failed_and_settled_not_left_for_review() {
    let rows = [
        (api(400), "400"),
        (api(409), "409"),
        (WfmError::Unauthorized { detail: "x".into() }, "401"),
        (
            WfmError::RateLimited {
                retry_after_secs: Some(3),
            },
            "429",
        ),
        (WfmError::invalid("bad input"), "invalid"),
        (
            WfmError::SubtypeRequired {
                subtypes: vec!["intact".into()],
            },
            "subtype",
        ),
    ];
    for (err, label) in rows {
        let (fake, wb) = run_one_failing_create(err).await;
        assert_eq!(statuses(&wb), [WorkbenchRowStatus::Failed], "{label}");
        assert_eq!(
            fake.entries()[0].outcome.as_ref().unwrap().status,
            OutcomeStatus::Failed,
            "{label}"
        );
        let state = wb.state();
        assert!(!state.review_required, "{label}");
        assert_eq!(state.unsettled_count, 0, "{label}");
    }
}

// The client turns a 509 (too many connections) into RateLimited, so no Api{509} ever reaches the workbench.
#[tokio::test]
async fn a_509_reaches_the_workbench_as_rate_limited_and_stays_failed_without_review() {
    let (_, wb) = run_one_failing_create(WfmError::RateLimited {
        retry_after_secs: None,
    })
    .await;
    assert_eq!(statuses(&wb), [WorkbenchRowStatus::Failed]);
    let state = wb.state();
    assert!(!state.review_required);
    assert_eq!(state.unsettled_count, 0);
}

fn raw_entry(id: &str, mode: &str, request: &str) -> String {
    format!(
        r#"{{"intentId":"{id}","planId":"p","rowId":"a","mode":"{mode}","request":{request},"createdAt":1}}"#
    )
}

fn live(id: &str, platinum: u32, quantity: u32, slug: &str) -> MyOrder {
    order(id, platinum, quantity, slug, slug)
}

#[tokio::test]
async fn reconciliation_classifies_against_refetched_live_orders() {
    let fake = FakeWb::new();
    fake.insert_raw(
        "i-create-found",
        &raw_entry(
            "i-create-found",
            "create",
            r#"{"slug":"boltor","itemName":"Boltor","platinum":10,"quantity":1}"#,
        ),
    );
    fake.insert_raw(
        "i-update-stale",
        &raw_entry(
            "i-update-stale",
            "update",
            r#"{"slug":"lex","itemName":"Lex","platinum":25,"quantity":1,"orderId":"ord-2"}"#,
        ),
    );
    fake.insert_raw(
        "i-update-gone",
        &raw_entry(
            "i-update-gone",
            "update",
            r#"{"slug":"vasto","itemName":"Vasto","platinum":9,"quantity":1,"orderId":"ord-3"}"#,
        ),
    );
    *fake.sell.lock().unwrap() = vec![live("ord-1", 10, 1, "boltor"), live("ord-2", 20, 1, "lex")];
    let wb = engine(&fake);

    let report = wb.reconcile().await;
    let by_id: HashMap<_, _> = report
        .rows
        .iter()
        .map(|r| (r.intent_id.as_str(), r))
        .collect();
    assert_eq!(
        by_id["i-create-found"].classification,
        WorkbenchReviewClassification::Confirmed
    );
    assert_eq!(
        by_id["i-create-found"].matched_order_id.as_deref(),
        Some("ord-1")
    );
    assert_eq!(
        by_id["i-update-stale"].classification,
        WorkbenchReviewClassification::Failed
    );
    assert_eq!(
        by_id["i-update-gone"].classification,
        WorkbenchReviewClassification::Unknown
    );
    // Reconcile alone settles nothing: review still required.
    assert!(wb.state().review_required);
}

#[tokio::test]
async fn an_unreadable_journal_forces_review_with_an_error_and_never_executes() {
    let fake = FakeWb::new();
    fake.insert_raw("x", "{ this is not json");
    let wb = engine(&fake);
    let state = wb.state();
    assert!(
        state
            .journal_error
            .as_ref()
            .unwrap()
            .to_lowercase()
            .contains("journal")
    );
    assert!(state.review_required);

    let p = plan(vec![row("a")]);
    assert!(!wb.execute(p.clone(), &snapshot(&p, 99, 99)).started);
    assert!(fake.calls().is_empty());
    // The unreadable row is preserved for inspection, not overwritten.
    let raw: String = fake
        .db
        .lock()
        .unwrap()
        .query_row("SELECT entry FROM workbench_intents", [], |r| r.get(0))
        .unwrap();
    assert_eq!(raw, "{ this is not json");

    let resolved = wb.resolve_review(&WorkbenchResolveReviewPayload {
        resolutions: vec![],
        reset_corrupt_journal: Some(true),
    });
    assert_eq!(resolved.journal_error, None);
    assert!(!resolved.review_required);
    assert!(fake.entries().is_empty());
}

#[tokio::test]
async fn a_well_formed_row_with_a_malformed_entry_is_treated_as_corrupt() {
    let fake = FakeWb::new();
    fake.insert_raw("x", r#"{"nonsense":true}"#);
    let state = engine(&fake).state();
    assert!(state.journal_error.unwrap().contains("corrupt"));
    assert!(state.review_required);
}

#[tokio::test]
async fn override_acknowledgements_are_journaled_for_audit() {
    let fake = FakeWb::new();
    let wb = engine(&fake);
    let ack = WorkbenchOverrideAck {
        plan_id: "p".into(),
        row_id: "a".into(),
        item_name: "Boltor".into(),
        safe_quantity: 1,
        requested_quantity: 2,
        reason_keys: vec!["inventory.safety.reason.lastCopy".into()],
        acknowledged_at: 5.0,
    };
    wb.acknowledge_override(&ack);
    assert_eq!(
        journal::overrides(&fake.db.lock().unwrap()),
        std::slice::from_ref(&ack)
    );
    assert!(
        WorkbenchOverrideAck {
            requested_quantity: 0,
            ..ack
        }
        .sanitized()
        .is_none()
    );
}

// ---------------------------------------------------------------- review classification evidence

async fn classify_with(
    entries: &[(&str, &str, &str)],
    sell: Vec<MyOrder>,
    preexisting: Option<&[String]>,
) -> WorkbenchReviewReport {
    let fake = FakeWb::new();
    for (id, mode, request) in entries {
        fake.insert_raw(id, &raw_entry(id, mode, request));
    }
    if let Some(ids) = preexisting {
        journal::set_preexisting(&fake.db.lock().unwrap(), "p", ids).unwrap();
    }
    *fake.sell.lock().unwrap() = sell;
    engine(&fake).reconcile().await
}

const BOLTOR: &str = r#"{"slug":"boltor","itemName":"Boltor","platinum":10,"quantity":1}"#;

#[tokio::test]
async fn an_order_that_existed_before_the_run_never_confirms_a_create() {
    let report = classify_with(
        &[("i-1", "create", BOLTOR)],
        vec![live("ord-1", 10, 1, "boltor")],
        Some(&["ord-1".to_owned()]),
    )
    .await;
    assert_eq!(
        report.rows[0].classification,
        WorkbenchReviewClassification::Unknown
    );
}

#[tokio::test]
async fn a_new_order_with_the_same_shape_still_confirms_the_create() {
    let report = classify_with(
        &[("i-1", "create", BOLTOR)],
        vec![
            live("ord-1", 10, 1, "boltor"),
            live("ord-9", 10, 1, "boltor"),
        ],
        Some(&["ord-1".to_owned()]),
    )
    .await;
    assert_eq!(
        report.rows[0].classification,
        WorkbenchReviewClassification::Confirmed
    );
    assert_eq!(report.rows[0].matched_order_id.as_deref(), Some("ord-9"));
}

#[tokio::test]
async fn a_create_is_not_confirmed_by_an_order_of_another_quantity_or_subtype() {
    let wrong_quantity = classify_with(
        &[(
            "i-1",
            "create",
            r#"{"slug":"boltor","itemName":"Boltor","platinum":10,"quantity":3}"#,
        )],
        vec![live("ord-1", 10, 1, "boltor")],
        None,
    )
    .await;
    assert_eq!(
        wrong_quantity.rows[0].classification,
        WorkbenchReviewClassification::Unknown
    );

    let mut intact = live("ord-1", 10, 1, "axi_a1_relic");
    intact.subtype = Some("intact".into());
    let wrong_subtype = classify_with(
        &[(
            "i-1",
            "create",
            r#"{"slug":"axi_a1_relic","itemName":"Axi A1 Relic","platinum":10,"quantity":1,"subtype":"radiant"}"#,
        )],
        vec![intact],
        None,
    )
    .await;
    assert_eq!(
        wrong_subtype.rows[0].classification,
        WorkbenchReviewClassification::Unknown
    );
}

#[tokio::test]
async fn an_update_whose_quantity_change_was_lost_is_not_reported_as_confirmed() {
    let report = classify_with(
        &[(
            "i-1",
            "update",
            r#"{"slug":"lex","itemName":"Lex","platinum":25,"quantity":4,"orderId":"ord-2"}"#,
        )],
        vec![live("ord-2", 25, 1, "lex")],
        None,
    )
    .await;
    assert_eq!(
        report.rows[0].classification,
        WorkbenchReviewClassification::Unknown
    );
}

#[tokio::test]
async fn records_the_pre_run_order_snapshot_when_a_plan_executes() {
    let fake = FakeWb::new();
    let wb = engine(&fake);
    let p = WorkbenchPlan {
        known_order_ids: Some(vec!["ord-1".into()]),
        ..plan(vec![row("a")])
    };
    assert!(wb.execute(p.clone(), &snapshot(&p, 99, 99)).started);
    wait_finish(&wb).await;
    let known = journal::preexisting(&fake.db.lock().unwrap(), "plan-1");
    assert_eq!(known, ["ord-1".to_owned()].into());
}

#[tokio::test]
async fn leaves_an_intent_open_when_the_user_resolves_it_as_unknown() {
    let fake = FakeWb::new();
    fake.insert_raw("i-1", &raw_entry("i-1", "create", BOLTOR));
    let wb = engine(&fake);
    let resolve = |c| {
        wb.resolve_review(&WorkbenchResolveReviewPayload {
            resolutions: vec![WorkbenchReviewResolution {
                intent_id: "i-1".into(),
                classification: c,
            }],
            reset_corrupt_journal: None,
        })
    };
    let left = resolve(WorkbenchReviewClassification::Unknown);
    assert!(left.review_required);
    assert_eq!(left.unsettled_count, 1);
    assert!(fake.entries()[0].resolved.is_none());

    let settled = resolve(WorkbenchReviewClassification::Failed);
    assert!(!settled.review_required);
    assert_eq!(
        fake.entries()[0].resolved.as_ref().unwrap().classification,
        WorkbenchReviewClassification::Failed
    );
}

// ---------------------------------------------------------------- matcher

type NameFn = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;
type SetFn = Box<dyn Fn(&str) -> Result<SetLookup, WfmError> + Send + Sync>;

struct FakeMatch {
    logged_in: bool,
    orders: Mutex<MyOrders>,
    orders_calls: AtomicUsize,
    name: NameFn,
    rank: Option<ItemRankInfo>,
    rank_err: bool,
    rank_calls: Mutex<Vec<String>>,
    set: SetFn,
    set_calls: AtomicUsize,
    contracts: Vec<MatchContract>,
    contracts_calls: AtomicUsize,
    closed: Mutex<Vec<String>>,
    close_err: bool,
}

impl FakeMatch {
    fn new() -> Self {
        Self {
            logged_in: true,
            orders: Mutex::default(),
            orders_calls: AtomicUsize::new(0),
            name: Box::new(|_| None),
            rank: None,
            rank_err: false,
            rank_calls: Mutex::default(),
            set: Box::new(|_| Ok(SetLookup::NotSet)),
            set_calls: AtomicUsize::new(0),
            contracts: vec![],
            contracts_calls: AtomicUsize::new(0),
            closed: Mutex::default(),
            close_err: false,
        }
    }
    fn sell(self, orders: Vec<MyOrder>) -> Self {
        self.orders.lock().unwrap().sell = orders;
        self
    }
    fn buy(self, orders: Vec<MyOrder>) -> Self {
        self.orders.lock().unwrap().buy = orders;
        self
    }
    fn names(mut self, f: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Self {
        self.name = Box::new(f);
        self
    }
    fn sets(
        mut self,
        f: impl Fn(&str) -> Result<SetLookup, WfmError> + Send + Sync + 'static,
    ) -> Self {
        self.set = Box::new(f);
        self
    }
    fn matcher(self) -> (Matcher<Arc<FakeMatch>>, Arc<FakeMatch>) {
        let host = Arc::new(self);
        (Matcher::new(host.clone()), host)
    }
}

impl MatchHost for Arc<FakeMatch> {
    fn logged_in(&self) -> bool {
        self.logged_in
    }
    fn my_orders(&self) -> impl Future<Output = Result<MyOrders, WfmError>> + Send {
        self.orders_calls.fetch_add(1, Ordering::SeqCst);
        fut(Ok(self.orders.lock().unwrap().clone()))
    }
    fn slug_by_name(&self, name: &str) -> impl Future<Output = Option<String>> + Send {
        fut((self.name)(name))
    }
    fn item_rank_info(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<Option<ItemRankInfo>, WfmError>> + Send {
        self.rank_calls.lock().unwrap().push(slug.into());
        fut(if self.rank_err {
            Err(WfmError::Network {
                detail: "offline".into(),
            })
        } else {
            Ok(self.rank.clone())
        })
    }
    fn set_lookup(&self, slug: &str) -> impl Future<Output = Result<SetLookup, WfmError>> + Send {
        self.set_calls.fetch_add(1, Ordering::SeqCst);
        fut((self.set)(slug))
    }
    fn my_contracts(&self) -> impl Future<Output = Result<Vec<MatchContract>, WfmError>> + Send {
        self.contracts_calls.fetch_add(1, Ordering::SeqCst);
        fut(Ok(self.contracts.clone()))
    }
    fn close_order(
        &self,
        id: &str,
        quantity: u32,
        per_trade: Option<u32>,
    ) -> impl Future<Output = Result<(), WfmError>> + Send {
        self.closed
            .lock()
            .unwrap()
            .push(format!("order:{id}:{quantity}:{per_trade:?}"));
        fut(if self.close_err {
            Err(WfmError::invalid("Network error"))
        } else {
            Ok(())
        })
    }
    fn close_contract(&self, id: &str) -> impl Future<Output = Result<(), WfmError>> + Send {
        self.closed.lock().unwrap().push(format!("contract:{id}"));
        fut(Ok(()))
    }
}

fn trade(kind: TradeType, plat: i64, items: &[(&str, u32, Direction)]) -> WorkbenchTrade {
    WorkbenchTrade {
        partner: "Buyer".into(),
        plat_change: plat,
        kind,
        items: items
            .iter()
            .map(|(n, c, d)| WorkbenchTradeItem {
                display_name: (*n).into(),
                count: *c,
                direction: *d,
            })
            .collect(),
    }
}

fn sale(plat: i64, items: &[(&str, u32)]) -> WorkbenchTrade {
    let items: Vec<_> = items
        .iter()
        .map(|(n, c)| (*n, *c, Direction::Given))
        .collect();
    trade(TradeType::Sale, plat, &items)
}

fn snake(name: &str) -> String {
    name.to_lowercase().replace(' ', "_")
}

fn ids(m: &[WorkbenchTradeMatch]) -> Vec<&str> {
    m.iter().map(|m| m.order_id.as_str()).collect()
}

fn set_of(set: &str, parts: &[(&str, u32)]) -> SetLookup {
    SetLookup::Set {
        set_slug: set.into(),
        parts: parts
            .iter()
            .map(|(s, q)| SetPart {
                slug: (*s).into(),
                quantity_in_set: *q,
            })
            .collect(),
    }
}

fn corinth_set<S: AsRef<str>>(parts: &[S]) -> SetLookup {
    set_of(
        "corinth_prime_set",
        &parts.iter().map(|p| (p.as_ref(), 1)).collect::<Vec<_>>(),
    )
}

const CORINTH: [&str; 4] = [
    "Corinth Prime Blueprint",
    "Corinth Prime Barrel",
    "Corinth Prime Receiver",
    "Corinth Prime Stock",
];

fn corinth_names(subset: usize) -> impl Fn(&str) -> Option<String> + Send + Sync + 'static {
    move |n| CORINTH[..subset].contains(&n).then(|| snake(n))
}

fn corinth_order(id: &str) -> MyOrder {
    order(id, 60, 1, "Corinth Prime Set", "corinth_prime_set")
}

#[tokio::test]
async fn returns_nothing_when_not_logged_in() {
    let mut f = FakeMatch::new();
    f.logged_in = false;
    let (m, host) = f.matcher();
    assert!(
        m.match_trade(&sale(50, &[("Ash Prime Chassis", 1)]))
            .await
            .is_empty()
    );
    assert_eq!(host.orders_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn returns_nothing_when_no_relevant_items_exist() {
    let (m, _) = FakeMatch::new().matcher();
    assert!(m.match_trade(&sale(50, &[])).await.is_empty());
}

#[tokio::test]
async fn matches_a_sell_order_by_item_name() {
    let mut o = order("order123", 50, 1, "Ash Prime Chassis", "ash_prime_chassis");
    o.item_thumb = Some("/items/ash_prime_chassis.png".into());
    let (m, _) = FakeMatch::new().sell(vec![o]).matcher();
    let r = m.match_trade(&sale(50, &[("Ash Prime Chassis", 1)])).await;
    assert_eq!(r.len(), 1);
    assert_eq!(
        (
            r[0].order_id.as_str(),
            r[0].item_name.as_str(),
            r[0].platinum,
            r[0].partner.as_str()
        ),
        ("order123", "Ash Prime Chassis", 50, "Buyer")
    );
    assert_eq!(r[0].trade_type, TradeType::Sale);
    assert_eq!(r[0].kind, WorkbenchMatchKind::Order);
}

#[tokio::test]
async fn matches_with_blueprint_stripping() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order(
            "order456",
            30,
            2,
            "Ash Prime Chassis",
            "ash_prime_chassis",
        )])
        .matcher();
    let r = m
        .match_trade(&sale(30, &[("Ash Prime Chassis Blueprint", 1)]))
        .await;
    assert_eq!(ids(&r), ["order456"]);
}

#[tokio::test]
async fn selects_closest_plat_match_when_multiple_orders_exist() {
    let (m, _) = FakeMatch::new()
        .sell(vec![
            order("order_far", 200, 1, "Soma Prime Set", "soma_prime_set"),
            order("order_close", 55, 1, "Soma Prime Set", "soma_prime_set"),
        ])
        .matcher();
    let r = m.match_trade(&sale(50, &[("Soma Prime Set", 1)])).await;
    assert_eq!(ids(&r), ["order_close"]);
}

#[tokio::test]
async fn matches_buy_orders_for_purchases() {
    let (m, _) = FakeMatch::new()
        .buy(vec![order(
            "buy_order1",
            25,
            5,
            "Nikana Prime Blade",
            "nikana_prime_blade",
        )])
        .matcher();
    let t = trade(
        TradeType::Purchase,
        25,
        &[("Nikana Prime Blade", 1, Direction::Received)],
    );
    let r = m.match_trade(&t).await;
    assert_eq!(ids(&r), ["buy_order1"]);
    assert_eq!(r[0].trade_type, TradeType::Purchase);
}

#[tokio::test]
async fn returns_nothing_when_no_orders_match() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order(
            "order_other",
            100,
            1,
            "Ember Prime Set",
            "ember_prime_set",
        )])
        .matcher();
    assert!(
        m.match_trade(&sale(50, &[("Frost Prime Chassis", 1)]))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn closes_the_full_stack_traded_bounded_by_order_qty() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order(
            "order_large",
            5,
            20,
            "Forma Blueprint",
            "forma_blueprint",
        )])
        .matcher();
    let r = m.match_trade(&sale(50, &[("Forma Blueprint", 10)])).await;
    assert_eq!(r[0].quantity, 10); // no artificial 6-cap
}

#[tokio::test]
async fn never_closes_more_than_the_orders_listed_quantity() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order(
            "order_small",
            5,
            3,
            "Forma Blueprint",
            "forma_blueprint",
        )])
        .matcher();
    let r = m.match_trade(&sale(50, &[("Forma Blueprint", 5)])).await;
    assert_eq!(r[0].quantity, 3); // traded 5 but only 3 listed
}

fn corinth_trade(n: usize, count: u32) -> WorkbenchTrade {
    sale(
        60,
        &CORINTH[..n].iter().map(|p| (*p, count)).collect::<Vec<_>>(),
    )
}

#[tokio::test]
async fn closes_a_set_order_when_the_trade_delivers_every_part() {
    let (m, _) = FakeMatch::new()
        .sell(vec![corinth_order("set_order")])
        .names(corinth_names(4))
        .sets(|_| Ok(corinth_set(&CORINTH.map(snake))))
        .matcher();
    let r = m.match_trade(&corinth_trade(4, 1)).await;
    assert_eq!((ids(&r), r[0].quantity), (vec!["set_order"], 1));
}

#[tokio::test]
async fn does_not_close_a_set_order_on_partial_part_coverage() {
    let (m, _) = FakeMatch::new()
        .sell(vec![corinth_order("set_order_partial")])
        .names(corinth_names(2))
        .sets(|_| Ok(corinth_set(&CORINTH.map(snake))))
        .matcher();
    assert!(m.match_trade(&corinth_trade(2, 1)).await.is_empty());
}

#[tokio::test]
async fn prefers_the_set_order_over_a_lone_part_order_when_the_full_set_is_traded() {
    let (m, _) = FakeMatch::new()
        .sell(vec![
            order(
                "part_order",
                60,
                1,
                "Corinth Prime Barrel",
                "corinth_prime_barrel",
            ),
            corinth_order("set_order"),
        ])
        .names(corinth_names(4))
        .sets(|_| Ok(corinth_set(&CORINTH.map(snake))))
        .matcher();
    let r = m.match_trade(&corinth_trade(4, 1)).await;
    assert_eq!(r[0].order_id, "set_order");
}

#[tokio::test]
async fn requires_per_part_quantities_before_closing_a_set() {
    let names = [
        "Kronen Prime Blueprint",
        "Kronen Prime Blade",
        "Kronen Prime Handle",
    ];
    let build = || {
        FakeMatch::new()
            .sell(vec![order(
                "kronen_set_order",
                80,
                1,
                "Kronen Prime Set",
                "kronen_prime_set",
            )])
            .names(move |n| names.contains(&n).then(|| snake(n)))
            .sets(|_| {
                Ok(set_of(
                    "kronen_prime_set",
                    &[
                        ("kronen_prime_blueprint", 1),
                        ("kronen_prime_blade", 2),
                        ("kronen_prime_handle", 2),
                    ],
                ))
            })
            .matcher()
            .0
    };
    let partial = build()
        .match_trade(&sale(80, &[(names[0], 1), (names[1], 1), (names[2], 1)]))
        .await;
    assert!(partial.is_empty());
    let full = build()
        .match_trade(&sale(80, &[(names[0], 1), (names[1], 2), (names[2], 2)]))
        .await;
    assert_eq!(
        (ids(&full), full[0].quantity),
        (vec!["kronen_set_order"], 1)
    );
}

#[tokio::test]
async fn does_not_close_a_set_whose_contents_cannot_be_resolved() {
    let (m, _) = FakeMatch::new()
        .sell(vec![corinth_order("unresolved_set")])
        .names(|n| Some(n.to_lowercase()))
        .sets(|_| Err(WfmError::invalid("unavailable")))
        .matcher();
    let t = sale(
        60,
        &[("corinth_prime_blueprint", 1), ("corinth_prime_barrel", 1)],
    );
    assert!(m.match_trade(&t).await.is_empty());
}

#[tokio::test]
async fn does_not_fall_through_to_a_part_order_when_set_metadata_is_unavailable() {
    let (m, _) = FakeMatch::new()
        .sell(vec![
            order(
                "barrel_order",
                20,
                1,
                "Corinth Prime Barrel",
                "corinth_prime_barrel",
            ),
            corinth_order("set_order"),
        ])
        .names(|n| Some(snake(n)))
        .sets(|_| Err(WfmError::invalid("unavailable")))
        .matcher();
    let t = sale(
        60,
        &[("Corinth Prime Barrel", 1), ("Corinth Prime Stock", 1)],
    );
    assert!(m.match_trade(&t).await.is_empty());
}

#[tokio::test]
async fn bounds_set_lookups_by_traded_items_not_active_orders() {
    let many = (0..200)
        .map(|i| {
            order(
                &format!("set_{i}"),
                50,
                1,
                &format!("Prime Set {i}"),
                &format!("prime_{i}_set"),
            )
        })
        .collect();
    let (m, host) = FakeMatch::new()
        .sell(many)
        .names(|n| Some(snake(n)))
        .matcher();
    m.match_trade(&sale(
        50,
        &[("Forma Blueprint", 1), ("Orokin Catalyst Blueprint", 1)],
    ))
    .await;
    assert_eq!(host.set_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn closes_as_many_sets_as_the_trade_covers_bounded_by_order_qty() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order(
            "multi_set_order",
            60,
            3,
            "Corinth Prime Set",
            "corinth_prime_set",
        )])
        .names(corinth_names(2))
        .sets(|_| {
            Ok(corinth_set(&[
                "corinth_prime_blueprint",
                "corinth_prime_barrel",
            ]))
        })
        .matcher();
    let r = m.match_trade(&corinth_trade(2, 2)).await;
    assert_eq!((ids(&r), r[0].quantity), (vec!["multi_set_order"], 2));
}

#[tokio::test]
async fn does_not_close_quantities_that_violate_the_listing_per_trade_bundle() {
    let mut o = order("bundle_order", 10, 12, "Vitus Essence", "vitus_essence");
    o.per_trade = 6;
    let (m, _) = FakeMatch::new().sell(vec![o]).matcher();
    assert!(
        m.match_trade(&sale(30, &[("Vitus Essence", 3)]))
            .await
            .is_empty()
    );
    let r = m.match_trade(&sale(60, &[("Vitus Essence", 6)])).await;
    assert_eq!(
        (ids(&r), r[0].quantity, r[0].per_trade),
        (vec!["bundle_order"], 6, Some(6))
    );
}

#[tokio::test]
async fn ignores_items_with_wrong_direction_for_sale_trades() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order(
            "order_match",
            50,
            1,
            "Ash Prime Chassis",
            "ash_prime_chassis",
        )])
        .matcher();
    // In a sale, the 'received' items are what we got (plat), not what we sold.
    let t = trade(
        TradeType::Sale,
        50,
        &[("Ash Prime Chassis", 1, Direction::Received)],
    );
    assert!(m.match_trade(&t).await.is_empty());
}

// mixed baskets

fn stock(id: &str, name: &str, plat: u32, rank: Option<u32>) -> MyOrder {
    MyOrder {
        mod_rank: rank,
        ..order(id, plat, 3, name, &snake(name))
    }
}

#[tokio::test]
async fn closes_one_listing_per_distinct_item_in_the_trade() {
    let (m, _) = FakeMatch::new()
        .sell(vec![
            stock("stock_order", "Boar Prime Stock", 30, None),
            stock("barrel_order", "Boar Prime Barrel", 25, None),
            stock("energize_order", "Arcane Energize", 40, Some(5)),
        ])
        .matcher();
    let r = m
        .match_trade(&sale(
            95,
            &[
                ("Boar Prime Stock", 2),
                ("Boar Prime Barrel", 2),
                ("Arcane Energize (RANK 5)", 2),
            ],
        ))
        .await;
    assert_eq!(ids(&r), ["stock_order", "barrel_order", "energize_order"]);
    assert!(r.iter().all(|m| m.quantity == 2));
}

#[tokio::test]
async fn never_closes_the_same_listing_twice_for_one_trade() {
    let (m, _) = FakeMatch::new()
        .sell(vec![stock("stock_order", "Boar Prime Stock", 30, None)])
        .matcher();
    let r = m
        .match_trade(&sale(
            60,
            &[("Boar Prime Stock", 1), ("Boar Prime Stock Blueprint", 1)],
        ))
        .await;
    assert_eq!(r.len(), 1);
}

fn braton() -> SetLookup {
    set_of(
        "braton_prime_set",
        &[("braton_prime_blueprint", 1), ("braton_prime_barrel", 1)],
    )
}

#[tokio::test]
async fn closes_the_set_listing_plus_the_unrelated_item_beside_it() {
    let (m, _) = FakeMatch::new()
        .names(|n| {
            [
                "Braton Prime Blueprint",
                "Braton Prime Barrel",
                "Vitus Essence",
            ]
            .contains(&n)
            .then(|| snake(n))
        })
        .sets(|s| {
            Ok(if s.starts_with("braton") {
                braton()
            } else {
                SetLookup::NotSet
            })
        })
        .sell(vec![
            stock("braton_set_order", "Braton Prime Set", 90, None),
            stock("barrel_order", "Braton Prime Barrel", 20, None),
            stock("vitus_order", "Vitus Essence", 15, None),
        ])
        .matcher();
    let r = m
        .match_trade(&sale(
            105,
            &[
                ("Braton Prime Blueprint", 1),
                ("Braton Prime Barrel", 1),
                ("Vitus Essence", 1),
            ],
        ))
        .await;
    assert_eq!(ids(&r), ["braton_set_order", "vitus_order"]);
}

#[tokio::test]
async fn closes_two_copies_of_the_same_full_set() {
    let (m, _) = FakeMatch::new()
        .names(|n| Some(snake(n)))
        .sets(|_| Ok(braton()))
        .sell(vec![stock(
            "braton_set_order",
            "Braton Prime Set",
            90,
            None,
        )])
        .matcher();
    let r = m
        .match_trade(&sale(
            180,
            &[("Braton Prime Blueprint", 2), ("Braton Prime Barrel", 2)],
        ))
        .await;
    assert_eq!((ids(&r), r[0].quantity), (vec!["braton_set_order"], 2));
}

#[tokio::test]
async fn closes_a_full_set_and_the_extra_component_beside_it() {
    let (m, _) = FakeMatch::new()
        .names(|n| Some(snake(n)))
        .sets(|_| Ok(braton()))
        .sell(vec![
            stock("braton_set_order", "Braton Prime Set", 90, None),
            stock("barrel_order", "Braton Prime Barrel", 20, None),
        ])
        .matcher();
    let r = m
        .match_trade(&sale(
            110,
            &[("Braton Prime Blueprint", 1), ("Braton Prime Barrel", 2)],
        ))
        .await;
    assert_eq!(
        r.iter()
            .map(|m| (m.order_id.as_str(), m.quantity))
            .collect::<Vec<_>>(),
        [("braton_set_order", 1), ("barrel_order", 1)]
    );
}

#[tokio::test]
async fn closes_every_distinct_full_set_covered_by_one_trade() {
    let (m, _) = FakeMatch::new()
        .names(|n| Some(snake(n)))
        .sets(|s| {
            Ok(if s.starts_with("braton") {
                braton()
            } else {
                set_of(
                    "burston_prime_set",
                    &[("burston_prime_blueprint", 1), ("burston_prime_barrel", 1)],
                )
            })
        })
        .sell(vec![
            stock("braton_set_order", "Braton Prime Set", 90, None),
            stock("burston_set_order", "Burston Prime Set", 80, None),
        ])
        .matcher();
    let r = m
        .match_trade(&sale(
            170,
            &[
                ("Braton Prime Blueprint", 1),
                ("Braton Prime Barrel", 1),
                ("Burston Prime Blueprint", 1),
                ("Burston Prime Barrel", 1),
            ],
        ))
        .await;
    assert_eq!(ids(&r), ["braton_set_order", "burston_set_order"]);
}

#[tokio::test]
async fn keeps_unavailable_parts_blocked_when_another_set_matches() {
    let (m, _) = FakeMatch::new()
        .names(|n| Some(snake(n)))
        .sets(|s| {
            if s.starts_with("braton") {
                Ok(braton())
            } else {
                Err(WfmError::invalid("unavailable"))
            }
        })
        .sell(vec![
            stock("braton_set_order", "Braton Prime Set", 90, None),
            stock("mystery_order", "Mystery Prime Barrel", 20, None),
        ])
        .matcher();
    let r = m
        .match_trade(&sale(
            110,
            &[
                ("Braton Prime Blueprint", 1),
                ("Braton Prime Barrel", 1),
                ("Mystery Prime Barrel", 1),
            ],
        ))
        .await;
    assert_eq!(ids(&r), ["braton_set_order"]);
}

// ranked items

fn live_wire() -> MyOrder {
    MyOrder {
        item_id: Some("live-wire-item".into()),
        ..order("live-wire-order", 7, 1, "Live Wire", "live_wire")
    }
}

fn live_wire_trade(rank: u32) -> WorkbenchTrade {
    sale(7, &[(&format!("Live Wire (COMMON RANK {rank})"), 1)])
}

fn rank_info(id: &str, slug: &str, has_ranks: Option<bool>) -> Option<ItemRankInfo> {
    Some(ItemRankInfo {
        id: id.into(),
        slug: slug.into(),
        has_ranks,
    })
}

#[tokio::test]
async fn matches_a_rank_zero_dialog_to_a_confirmed_rankless_listing() {
    let mut f = FakeMatch::new().sell(vec![live_wire()]);
    f.rank = rank_info("live-wire-item", "live_wire", Some(false));
    let (m, host) = f.matcher();
    let r = m.match_trade(&live_wire_trade(0)).await;
    assert_eq!(ids(&r), ["live-wire-order"]);
    assert_eq!(*host.rank_calls.lock().unwrap(), ["live_wire"]);
    assert!(host.closed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rejects_a_missing_order_rank_without_confirmed_rankless_metadata() {
    for (label, info) in [
        (
            "ranked metadata",
            rank_info("live-wire-item", "live_wire", Some(true)),
        ),
        (
            "unknown metadata",
            rank_info("live-wire-item", "live_wire", None),
        ),
        (
            "another item ID",
            rank_info("other-item", "live_wire", Some(false)),
        ),
        (
            "another slug",
            rank_info("live-wire-item", "other_item", Some(false)),
        ),
    ] {
        let mut f = FakeMatch::new().sell(vec![live_wire()]);
        f.rank = info;
        let (m, _) = f.matcher();
        assert!(
            m.match_trade(&live_wire_trade(0)).await.is_empty(),
            "{label}"
        );
    }
}

#[tokio::test]
async fn does_not_accept_a_missing_rank_when_detail_lookup_fails() {
    let mut f = FakeMatch::new().sell(vec![live_wire()]);
    f.rank_err = true;
    let (m, host) = f.matcher();
    assert!(m.match_trade(&live_wire_trade(0)).await.is_empty());
    assert!(host.closed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn does_not_infer_rankless_status_from_a_catalog_with_a_missing_max_rank() {
    let (m, host) = FakeMatch::new()
        .sell(vec![live_wire()])
        .names(|_| Some("live_wire".into()))
        .matcher();
    assert!(m.match_trade(&live_wire_trade(0)).await.is_empty());
    assert_eq!(*host.rank_calls.lock().unwrap(), ["live_wire"]);
}

#[tokio::test]
async fn does_not_request_details_when_the_listing_already_has_rank_zero() {
    let (m, host) = FakeMatch::new()
        .sell(vec![MyOrder {
            mod_rank: Some(0),
            ..live_wire()
        }])
        .matcher();
    assert_eq!(
        ids(&m.match_trade(&live_wire_trade(0)).await),
        ["live-wire-order"]
    );
    assert!(host.rank_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn never_substitutes_a_missing_listing_rank_for_a_positive_traded_rank() {
    let (m, host) = FakeMatch::new().sell(vec![live_wire()]).matcher();
    assert!(m.match_trade(&live_wire_trade(1)).await.is_empty());
    assert!(host.rank_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn matches_a_mod_listed_without_the_trade_dialogs_rank_suffix() {
    let (m, _) = FakeMatch::new()
        .sell(vec![MyOrder {
            mod_rank: Some(10),
            ..order("mod-order", 15, 1, "Serration", "serration")
        }])
        .matcher();
    let r = m
        .match_trade(&sale(15, &[("Serration (RANK 10)", 1)]))
        .await;
    assert_eq!(ids(&r), ["mod-order"]);
}

#[tokio::test]
async fn requires_the_traded_rank_even_when_another_rank_matches_the_total_price() {
    let pc = |id, plat, rank| MyOrder {
        mod_rank: Some(rank),
        ..order(id, plat, 1, "Primed Continuity", "primed_continuity")
    };
    let (m, _) = FakeMatch::new()
        .sell(vec![pc("rank-0", 20, 0), pc("rank-10", 25, 10)])
        .matcher();
    let r = m
        .match_trade(&sale(20, &[("Primed Continuity (RANK 10)", 1)]))
        .await;
    assert_eq!(ids(&r), ["rank-10"]);
}

// riven auctions

fn contract(id: &str, weapon: &str, suffix: Option<&str>) -> MatchContract {
    MatchContract {
        id: id.into(),
        weapon_url_name: Some(weapon.into()),
        riven_suffix: suffix.map(Into::into),
        item_thumb: None,
        platinum: 100,
    }
}

fn riven_sale() -> WorkbenchTrade {
    sale(300, &[("Rubico Visio-Critatis (RIVEN RANK 0)", 1)])
}

#[tokio::test]
async fn matches_the_auction_by_weapon_and_roll_name() {
    let mut f = FakeMatch::new();
    f.contracts = vec![
        contract("auction-1", "rubico", Some("visio-critatis")),
        contract("auction-2", "rubico", Some("croni-tempis")),
    ];
    let (m, _) = f.matcher();
    let r = m.match_trade(&riven_sale()).await;
    assert_eq!(
        (
            r[0].kind,
            r[0].order_id.as_str(),
            r[0].quantity,
            r[0].platinum
        ),
        (WorkbenchMatchKind::Contract, "auction-1", 1, 300)
    );
}

#[tokio::test]
async fn falls_back_to_the_only_auction_for_that_weapon() {
    let mut f = FakeMatch::new();
    f.contracts = vec![contract("auction-solo", "rubico", None)];
    let (m, _) = f.matcher();
    assert_eq!(ids(&m.match_trade(&riven_sale()).await), ["auction-solo"]);
}

#[tokio::test]
async fn closes_nothing_when_several_rivens_for_the_weapon_are_listed() {
    let mut f = FakeMatch::new();
    f.contracts = vec![
        contract("auction-1", "rubico", Some("sati-manti")),
        contract("auction-2", "rubico", Some("croni-tempis")),
    ];
    let (m, _) = f.matcher();
    assert!(m.match_trade(&riven_sale()).await.is_empty());
}

#[tokio::test]
async fn ignores_auctions_when_the_riven_was_bought_not_sold() {
    let (m, host) = FakeMatch::new().matcher();
    let t = trade(
        TradeType::Purchase,
        300,
        &[(
            "Rubico Visio-Critatis (RIVEN RANK 0)",
            1,
            Direction::Received,
        )],
    );
    m.match_trade(&t).await;
    assert_eq!(host.contracts_calls.load(Ordering::SeqCst), 0);
}

// closing

fn payload(
    kind: WorkbenchMatchKind,
    id: &str,
    quantity: u32,
    per_trade: Option<u32>,
) -> WorkbenchTradeMatch {
    WorkbenchTradeMatch {
        kind,
        order_id: id.into(),
        item_name: "Test Item".into(),
        item_url_name: Some("test_item".into()),
        item_thumb: None,
        quantity,
        platinum: 50,
        partner: "Buyer".into(),
        trade_type: TradeType::Sale,
        per_trade,
    }
}

#[tokio::test]
async fn closes_a_riven_auction_through_the_contracts_route() {
    let (m, host) = FakeMatch::new().matcher();
    assert!(
        m.close_match(&payload(WorkbenchMatchKind::Contract, "auction-1", 1, None))
            .await
    );
    assert_eq!(*host.closed.lock().unwrap(), ["contract:auction-1"]);
}

#[tokio::test]
async fn closes_an_order_with_its_quantity_and_per_trade_and_returns_true() {
    let (m, host) = FakeMatch::new().matcher();
    assert!(
        m.close_match(&payload(WorkbenchMatchKind::Order, "order123", 6, Some(3)))
            .await
    );
    assert_eq!(*host.closed.lock().unwrap(), ["order:order123:6:Some(3)"]);
}

#[tokio::test]
async fn returns_false_when_the_close_fails_and_does_not_mark_it_closed() {
    let mut f = FakeMatch::new().sell(vec![order("order123", 50, 1, "Test Item", "test_item")]);
    f.close_err = true;
    let (m, _) = f.matcher();
    assert!(
        !m.close_match(&payload(WorkbenchMatchKind::Order, "order123", 1, None))
            .await
    );
    // A failed close leaves the listing matchable.
    assert_eq!(
        ids(&m.match_trade(&sale(50, &[("Test Item", 1)])).await),
        ["order123"]
    );
}

#[tokio::test]
async fn a_closed_listing_is_not_matched_again_by_a_duplicate_log_line() {
    let (m, _) = FakeMatch::new()
        .sell(vec![order("order123", 50, 5, "Test Item", "test_item")])
        .matcher();
    let t = sale(50, &[("Test Item", 1)]);
    let first = m.match_trade(&t).await;
    assert!(m.close_match(&first[0]).await);
    assert!(m.match_trade(&t).await.is_empty());
}

#[tokio::test]
async fn close_all_closes_each_match_and_reports_only_the_accepted_ones() {
    let (m, host) = FakeMatch::new().matcher();
    let two = [
        payload(WorkbenchMatchKind::Order, "a", 1, None),
        payload(WorkbenchMatchKind::Contract, "b", 1, None),
    ];
    let mut seen = vec![];
    let closed = m.close_all(&two, |x| seen.push(x.order_id.clone())).await;
    assert_eq!(
        (closed, seen),
        (
            vec!["a".into(), "b".into()],
            vec!["a".to_string(), "b".into()]
        )
    );
    assert_eq!(host.closed.lock().unwrap().len(), 2);

    let mut f = FakeMatch::new();
    f.close_err = true;
    let (m, _) = f.matcher();
    let mut called = false;
    assert!(m.close_all(&two[..1], |_| called = true).await.is_empty());
    assert!(!called, "a refused close announces nothing");
}
