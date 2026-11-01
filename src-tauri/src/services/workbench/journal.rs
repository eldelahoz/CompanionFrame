//! Write-ahead journal of workbench mutations (migration `workbench_journal`). Port of the JSON file
//! in WFHelper's `tradeWorkbench.ts`: an intent is stored before its request leaves, the outcome only
//! after WFM answered, so an intent without outcome is what forces review after a crash.

use std::collections::HashSet;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::types::{WorkbenchOverrideAck, WorkbenchReviewClassification, WorkbenchRowMode};

const MAX_SETTLED_ENTRIES: u32 = 300;
const MAX_OVERRIDE_RECORDS: u32 = 200;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub slug: String,
    pub item_name: String,
    pub platinum: u32,
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Catalog id resolved just before the send; recorded for the audit trail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutcomeStatus {
    Confirmed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub status: OutcomeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub at: f64,
}

/// The user's explicit review verdict for an intent with no outcome.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resolved {
    pub classification: WorkbenchReviewClassification,
    pub at: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub intent_id: String,
    pub plan_id: String,
    pub row_id: String,
    pub mode: WorkbenchRowMode,
    pub request: Request,
    pub created_at: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Resolved>,
}

impl Entry {
    pub fn is_settled(&self) -> bool {
        self.outcome.is_some() || self.resolved.is_some()
    }
}

pub fn append(conn: &Connection, e: &Entry) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO workbench_intents (intent_id, plan_id, entry, settled) VALUES (?1, ?2, ?3, ?4)",
        params![e.intent_id, e.plan_id, json(e)?, e.is_settled()],
    )?;
    Ok(())
}

/// Rewrites the stored entry (outcome / review verdict) and prunes old settled history.
pub fn update(conn: &Connection, e: &Entry) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE workbench_intents SET entry = ?2, settled = ?3 WHERE intent_id = ?1",
        params![e.intent_id, json(e)?, e.is_settled()],
    )?;
    prune(conn)
}

fn json<T: Serialize>(v: &T) -> rusqlite::Result<String> {
    serde_json::to_string(v).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))
}

/// Every entry, oldest first. A row that no longer parses makes the whole journal untrustworthy.
pub fn load(conn: &Connection) -> Result<Vec<Entry>, String> {
    let mut stmt = conn
        .prepare("SELECT entry FROM workbench_intents ORDER BY seq")
        .map_err(|e| format!("journal unreadable: {e}"))?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .and_then(Iterator::collect)
        .map_err(|e| format!("journal unreadable: {e}"))?;
    rows.iter()
        .map(|j| serde_json::from_str(j).map_err(|e| format!("journal corrupt: {e}")))
        .collect()
}

pub fn unsettled(conn: &Connection) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt =
        conn.prepare("SELECT entry FROM workbench_intents WHERE settled = 0 ORDER BY seq")?;
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(rows
        .iter()
        .filter_map(|j| serde_json::from_str(j).ok())
        .collect())
}

pub fn unsettled_count(conn: &Connection) -> u32 {
    conn.query_row(
        "SELECT count(*) FROM workbench_intents WHERE settled = 0",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

pub fn get(conn: &Connection, intent_id: &str) -> Option<Entry> {
    let j: String = conn
        .query_row(
            "SELECT entry FROM workbench_intents WHERE intent_id = ?1",
            [intent_id],
            |r| r.get(0),
        )
        .ok()?;
    serde_json::from_str(&j).ok()
}

pub fn push_override(conn: &Connection, ack: &WorkbenchOverrideAck) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO workbench_overrides (ack) VALUES (?1)",
        [json(ack)?],
    )?;
    prune(conn)
}

pub fn overrides(conn: &Connection) -> Vec<WorkbenchOverrideAck> {
    let Ok(mut stmt) = conn.prepare("SELECT ack FROM workbench_overrides ORDER BY seq") else {
        return vec![];
    };
    stmt.query_map([], |r| r.get::<_, String>(0))
        .map(|rows| {
            rows.filter_map(|j| serde_json::from_str(&j.ok()?).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Own sell-order ids that existed when the plan started.
pub fn set_preexisting(conn: &Connection, plan_id: &str, ids: &[String]) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO workbench_preexisting (plan_id, order_ids) VALUES (?1, ?2)",
        params![plan_id, json(&ids)?],
    )?;
    Ok(())
}

pub fn preexisting(conn: &Connection, plan_id: &str) -> HashSet<String> {
    conn.query_row(
        "SELECT order_ids FROM workbench_preexisting WHERE plan_id = ?1",
        [plan_id],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
    .unwrap_or_default()
    .into_iter()
    .collect()
}

/// Settled history and audit acks are capped; unsettled intents are never pruned. A pre-run
/// snapshot only matters while its plan still has entries to classify.
fn prune(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM workbench_intents WHERE settled = 1 AND seq <= (
             SELECT seq FROM workbench_intents WHERE settled = 1 ORDER BY seq DESC LIMIT 1 OFFSET ?1)",
        [MAX_SETTLED_ENTRIES],
    )?;
    conn.execute(
        "DELETE FROM workbench_overrides WHERE seq <= (
             SELECT seq FROM workbench_overrides ORDER BY seq DESC LIMIT 1 OFFSET ?1)",
        [MAX_OVERRIDE_RECORDS],
    )?;
    conn.execute(
        "DELETE FROM workbench_preexisting WHERE plan_id NOT IN (SELECT plan_id FROM workbench_intents)",
        [],
    )?;
    Ok(())
}

/// Explicit user consent: replace an untrustworthy journal with an empty one.
pub fn reset(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DELETE FROM workbench_intents; DELETE FROM workbench_overrides; DELETE FROM workbench_preexisting",
    )
}
