//! Workbench contract (port of WFHelper `tradeWorkbenchTypes.ts`): the UI builds a selling plan,
//! the backend journals every mutation and executes it. Payloads from the webview are untrusted:
//! `sanitized()` is the parse boundary, `validate_plan` the safety gate shared by preview and execute.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

pub const MAX_ROWS_PER_RUN: usize = 100;
/// Above the run cap on purpose, so the validator (not the parser) reports an oversized plan.
const MAX_PARSED_ROWS: usize = MAX_ROWS_PER_RUN + 10;
const MAX_ID_LENGTH: usize = 64;
const MAX_NAME_LENGTH: usize = 120;
const MAX_PLATINUM: u32 = 100_000;
const MAX_QUANTITY: u32 = 999;
const MAX_REASON_KEYS: usize = 16;
const MAX_KNOWN_ORDER_IDS: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchRowMode {
    Create,
    Update,
}

/// User consent to sell copies the safety engine reserved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchOverride {
    pub acknowledged_at: f64,
    pub reason_keys: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchPlanRow {
    pub row_id: String,
    pub mode: WorkbenchRowMode,
    /// WFM catalog slug; the item id is resolved at execution time.
    pub slug: String,
    pub item_name: String,
    pub quantity: u32,
    pub platinum: u32,
    pub rank: Option<u32>,
    pub subtype: Option<String>,
    /// Existing sell order to reprice/restock; required in "update" mode.
    pub order_id: Option<String>,
    #[serde(rename = "override")]
    pub override_ack: Option<WorkbenchOverride>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchPlan {
    pub plan_id: String,
    pub created_at: f64,
    pub rows: Vec<WorkbenchPlanRow>,
    /// Own sell-order ids that existed when the plan was built; review needs them so a pre-run
    /// order is never read as a create that landed.
    pub known_order_ids: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct WorkbenchSafetyRow {
    pub safe: u32,
    pub total: u32,
}

/// Captured by the UI from the live safety engine at confirm time; execute re-validates against it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchSafetySnapshot {
    pub captured_at: f64,
    pub rows: HashMap<String, WorkbenchSafetyRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkbenchRowRejection {
    MissingSafety,
    OverSafe,
    OverTotal,
    BadQuantity,
    BadPrice,
    MissingOrderId,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchRowValidation {
    pub row_id: String,
    pub ok: bool,
    pub reason: Option<WorkbenchRowRejection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkbenchPlanError {
    TooManyRows,
    DuplicateRowIds,
    Empty,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchPlanValidation {
    pub ok: bool,
    /// Set when the plan is rejected as a whole (cap, duplicates), not per row.
    pub plan_error: Option<WorkbenchPlanError>,
    pub rows: Vec<WorkbenchRowValidation>,
    pub total_units: u32,
    pub total_platinum: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkbenchRowStatus {
    Pending,
    InFlight,
    Done,
    Failed,
    Cancelled,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchRowProgress {
    pub row_id: String,
    pub item_name: String,
    pub status: WorkbenchRowStatus,
    pub order_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum WorkbenchStopReason {
    Completed,
    Cancelled,
    OrderLimit,
    Auth,
    Error,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchRunProgress {
    pub plan_id: String,
    pub started_at: f64,
    pub finished_at: Option<f64>,
    pub stop_reason: Option<WorkbenchStopReason>,
    pub rows: Vec<WorkbenchRowProgress>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchPhase {
    Idle,
    Running,
    Cancelling,
    Review,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchState {
    pub phase: WorkbenchPhase,
    /// True until every journaled intent is settled or explicitly resolved.
    pub review_required: bool,
    /// Set when the journal was unreadable; review is forced until reset.
    pub journal_error: Option<String>,
    pub unsettled_count: u32,
    pub run: Option<WorkbenchRunProgress>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchReviewClassification {
    Confirmed,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchReviewRow {
    pub intent_id: String,
    pub plan_id: String,
    pub row_id: String,
    pub mode: WorkbenchRowMode,
    pub item_name: String,
    pub slug: String,
    pub platinum: u32,
    pub quantity: u32,
    pub created_at: f64,
    pub classification: WorkbenchReviewClassification,
    /// Live order that proves the mutation landed, when one was found.
    pub matched_order_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchReviewReport {
    pub generated_at: f64,
    pub rows: Vec<WorkbenchReviewRow>,
    pub fetch_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchReviewResolution {
    pub intent_id: String,
    pub classification: WorkbenchReviewClassification,
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchResolveReviewPayload {
    pub resolutions: Vec<WorkbenchReviewResolution>,
    /// Explicit consent to replace an unreadable journal with a fresh one.
    pub reset_corrupt_journal: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchOverrideAck {
    pub plan_id: String,
    pub row_id: String,
    pub item_name: String,
    pub safe_quantity: u32,
    pub requested_quantity: u32,
    pub reason_keys: Vec<String>,
    pub acknowledged_at: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchExecuteResult {
    pub started: bool,
    pub error: Option<String>,
    pub validation: Option<WorkbenchPlanValidation>,
    pub state: WorkbenchState,
}

/// WFM caps concurrent orders by account tier and the exact error string is undocumented, so
/// require an order context AND a cap word. A bare rate-limit message stays transient.
pub fn is_order_limit_message(message: &str) -> bool {
    let m = message
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    m.contains("order")
        && ["limit", "too many", "maximum"]
            .iter()
            .any(|w| m.contains(w))
}

fn bounded(s: &str, max: usize) -> Option<String> {
    let t = s.trim();
    (!t.is_empty() && t.chars().count() <= max).then(|| t.to_owned())
}

fn finite_ms(v: f64) -> Option<f64> {
    (v.is_finite() && v >= 0.0).then(|| v.round())
}

fn reason_keys(keys: &[String]) -> Vec<String> {
    keys.iter()
        .take(MAX_REASON_KEYS)
        .filter_map(|k| bounded(k, MAX_NAME_LENGTH))
        .collect()
}

impl WorkbenchPlanRow {
    fn sanitized(self) -> Option<Self> {
        let mode = self.mode;
        let order_id = self.order_id.and_then(|s| bounded(&s, MAX_ID_LENGTH));
        if mode == WorkbenchRowMode::Update && order_id.is_none() {
            return None;
        }
        let override_ack = match self.override_ack {
            Some(o) => Some(WorkbenchOverride {
                acknowledged_at: finite_ms(o.acknowledged_at)?,
                reason_keys: reason_keys(&o.reason_keys),
            }),
            None => None,
        };
        Some(Self {
            row_id: bounded(&self.row_id, MAX_ID_LENGTH)?,
            mode,
            slug: bounded(&self.slug, MAX_NAME_LENGTH)?,
            item_name: bounded(&self.item_name, MAX_NAME_LENGTH)?,
            quantity: (1..=MAX_QUANTITY)
                .contains(&self.quantity)
                .then_some(self.quantity)?,
            platinum: (1..=MAX_PLATINUM)
                .contains(&self.platinum)
                .then_some(self.platinum)?,
            rank: self.rank.filter(|r| *r <= 10),
            subtype: self.subtype.and_then(|s| bounded(&s, 32)),
            order_id,
            override_ack,
        })
    }
}

impl WorkbenchPlan {
    /// Hostile-input boundary: `None` for anything that is not a well-formed plan.
    pub fn sanitized(self) -> Option<Self> {
        if self.rows.len() > MAX_PARSED_ROWS {
            return None;
        }
        let rows = self
            .rows
            .into_iter()
            .map(WorkbenchPlanRow::sanitized)
            .collect::<Option<Vec<_>>>()?;
        let ids: Vec<String> = self
            .known_order_ids
            .iter()
            .flatten()
            .take(MAX_KNOWN_ORDER_IDS)
            .filter_map(|s| bounded(s, MAX_ID_LENGTH))
            .collect();
        Some(Self {
            plan_id: bounded(&self.plan_id, MAX_ID_LENGTH)?,
            created_at: finite_ms(self.created_at)?,
            rows,
            known_order_ids: (!ids.is_empty()).then_some(ids),
        })
    }
}

impl WorkbenchSafetySnapshot {
    pub fn sanitized(self) -> Option<Self> {
        if self.rows.len() > MAX_PARSED_ROWS * 4 {
            return None;
        }
        Some(Self {
            captured_at: finite_ms(self.captured_at)?,
            rows: self.rows,
        })
    }
}

impl WorkbenchOverrideAck {
    pub fn sanitized(self) -> Option<Self> {
        let quantity = |v: u32, min| (min..=MAX_QUANTITY).contains(&v).then_some(v);
        Some(Self {
            plan_id: bounded(&self.plan_id, MAX_ID_LENGTH)?,
            row_id: bounded(&self.row_id, MAX_ID_LENGTH)?,
            item_name: bounded(&self.item_name, MAX_NAME_LENGTH)?,
            safe_quantity: quantity(self.safe_quantity, 0)?,
            requested_quantity: quantity(self.requested_quantity, 1)?,
            reason_keys: reason_keys(&self.reason_keys),
            acknowledged_at: finite_ms(self.acknowledged_at)?,
        })
    }
}

impl WorkbenchResolveReviewPayload {
    pub fn sanitized(self) -> Option<Self> {
        if self.resolutions.len() > MAX_PARSED_ROWS * 4 {
            return None;
        }
        let resolutions = self
            .resolutions
            .into_iter()
            .map(|r| {
                Some(WorkbenchReviewResolution {
                    intent_id: bounded(&r.intent_id, MAX_ID_LENGTH)?,
                    classification: r.classification,
                })
            })
            .collect::<Option<_>>()?;
        Some(Self {
            resolutions,
            reset_corrupt_journal: self.reset_corrupt_journal.filter(|b| *b),
        })
    }
}

/// Shared by preview and execute so both report identical verdicts. Every row must pass; a
/// partially valid plan never executes partially.
pub fn validate_plan(
    plan: &WorkbenchPlan,
    safety: &WorkbenchSafetySnapshot,
) -> WorkbenchPlanValidation {
    let fail = |plan_error| WorkbenchPlanValidation {
        ok: false,
        plan_error: Some(plan_error),
        rows: vec![],
        total_units: 0,
        total_platinum: 0.0,
    };
    if plan.rows.is_empty() {
        return fail(WorkbenchPlanError::Empty);
    }
    if plan.rows.len() > MAX_ROWS_PER_RUN {
        return fail(WorkbenchPlanError::TooManyRows);
    }
    let ids: HashSet<&str> = plan.rows.iter().map(|r| r.row_id.as_str()).collect();
    if ids.len() != plan.rows.len() {
        return fail(WorkbenchPlanError::DuplicateRowIds);
    }

    let (mut ok, mut total_units, mut total_platinum) = (true, 0u32, 0f64);
    let rows = plan
        .rows
        .iter()
        .map(|row| {
            let snapshot = safety.rows.get(&row.row_id);
            let reason = if row.quantity < 1 {
                Some(WorkbenchRowRejection::BadQuantity)
            } else if row.platinum < 1 {
                Some(WorkbenchRowRejection::BadPrice)
            } else if row.mode == WorkbenchRowMode::Update && row.order_id.is_none() {
                Some(WorkbenchRowRejection::MissingOrderId)
            } else if let Some(s) = snapshot {
                // An override never authorizes selling more than the account holds.
                if row.quantity > s.total {
                    Some(WorkbenchRowRejection::OverTotal)
                } else if row.quantity > s.safe && row.override_ack.is_none() {
                    Some(WorkbenchRowRejection::OverSafe)
                } else {
                    None
                }
            } else {
                Some(WorkbenchRowRejection::MissingSafety)
            };
            if reason.is_some() {
                ok = false;
            } else {
                total_units += row.quantity;
                total_platinum += f64::from(row.quantity) * f64::from(row.platinum);
            }
            WorkbenchRowValidation {
                row_id: row.row_id.clone(),
                ok: reason.is_none(),
                reason,
            }
        })
        .collect();
    WorkbenchPlanValidation {
        ok,
        plan_error: None,
        rows,
        total_units,
        total_platinum,
    }
}
