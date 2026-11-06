//! The jobs journal (migration `jobs`): a job row and one row per item, written before a request leaves
//! and settled after, so a crash leaves exactly what had not run. Not `workbench_intents`: that journal
//! is typed for sell plans and forces a review before the workbench can run again.

use rusqlite::{Connection, params};

use super::{ItemStatus, Job, JobItem, JobKind, JobState};

/// The lowercase word a unit enum serializes to.
fn text<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v)
        .unwrap_or_default()
        .trim_matches('"')
        .to_owned()
}

fn from_text<T: serde::de::DeserializeOwned>(s: &str, fallback: T) -> T {
    serde_json::from_str(&format!("\"{s}\"")).unwrap_or(fallback)
}

pub fn insert(c: &Connection, job: &Job) -> rusqlite::Result<()> {
    let tx = c.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO jobs (id, kind, state, created_at, finished_at) VALUES (?1, ?2, ?3, ?4, NULL)",
        params![
            job.id,
            serde_json::to_string(&job.kind).unwrap_or_default(),
            text(&job.state),
            job.created_at
        ],
    )?;
    for (seq, i) in job.items.iter().enumerate() {
        tx.execute(
            "INSERT INTO job_items (job_id, seq, item_id, name, platinum, status, reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![job.id, seq as i64, i.id, i.name, i.platinum, text(&i.status), i.reason],
        )?;
    }
    tx.commit()
}

pub fn set_item(
    c: &Connection,
    job: &str,
    seq: usize,
    status: ItemStatus,
    reason: Option<&str>,
) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE job_items SET status = ?3, reason = ?4 WHERE job_id = ?1 AND seq = ?2",
        params![job, seq as i64, text(&status), reason],
    )
    .map(|_| ())
}

pub fn set_state(
    c: &Connection,
    job: &str,
    state: JobState,
    finished_at: Option<f64>,
) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE jobs SET state = ?2, finished_at = ?3 WHERE id = ?1",
        params![job, text(&state), finished_at],
    )
    .map(|_| ())
}

pub fn delete(c: &Connection, job: &str) -> rusqlite::Result<()> {
    c.execute("DELETE FROM job_items WHERE job_id = ?1", [job])?;
    c.execute("DELETE FROM jobs WHERE id = ?1", [job])
        .map(|_| ())
}

/// Startup: whatever was running when the app died is interrupted, and an item that was in flight goes
/// back to pending (hide, show, delete and reprice are safe to repeat). Nothing is started.
pub fn interrupt_unfinished(c: &Connection) -> rusqlite::Result<()> {
    c.execute(
        "UPDATE job_items SET status = 'pending' WHERE status = 'running'",
        [],
    )?;
    c.execute(
        "UPDATE jobs SET state = 'interrupted' WHERE state IN ('running', 'stopping')",
        [],
    )
    .map(|_| ())
}

pub fn load(c: &Connection) -> rusqlite::Result<Vec<Job>> {
    let mut jobs = c
        .prepare("SELECT id, kind, state, created_at, finished_at FROM jobs ORDER BY created_at")?
        .query_map([], |r| {
            Ok(Job {
                id: r.get(0)?,
                kind: serde_json::from_str(&r.get::<_, String>(1)?)
                    .unwrap_or(JobKind::OrdersDelete),
                state: from_text(&r.get::<_, String>(2)?, JobState::Interrupted),
                total: 0,
                done: 0,
                failed: 0,
                items: vec![],
                created_at: r.get(3)?,
                finished_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items = c.prepare(
        "SELECT item_id, name, platinum, status, reason FROM job_items WHERE job_id = ?1 ORDER BY seq",
    )?;
    for job in &mut jobs {
        job.items = items
            .query_map([&job.id], |r| {
                Ok(JobItem {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    platinum: r.get(2)?,
                    status: from_text(&r.get::<_, String>(3)?, ItemStatus::Pending),
                    reason: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        job.total = job.items.len() as u32;
        job.recount();
    }
    Ok(jobs)
}

/// Finished jobs older than `cutoff_ms` go; one still waiting for the user (interrupted) stays.
pub fn prune(c: &Connection, cutoff_ms: f64) -> rusqlite::Result<usize> {
    c.execute(
        "DELETE FROM job_items WHERE job_id IN (SELECT id FROM jobs WHERE state = 'done' AND finished_at < ?1)",
        [cutoff_ms],
    )?;
    c.execute(
        "DELETE FROM jobs WHERE state = 'done' AND finished_at < ?1",
        [cutoff_ms],
    )
}
