//! Background jobs: long actions (hide, show, delete or reprice many orders) run here one item at a time
//! while the UI only watches. A job is always started by a click; an interrupted job (app closed
//! mid-run, or a dead session) never resumes by itself, the user continues it.
//!
//! Three sources feed the same list: jobs this engine runs and journals (`orders` kinds), the workbench
//! run (mirrored, its own engine and journal stay) and the price refresh worker (progress only).

pub mod books;
pub mod commands;
mod store;

pub use store::prune;

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::services::clock::now_ms_f64;

/// Seconds between tries while the connection is down (doubling, capped at six times this).
const OFFLINE_WAIT: Duration = Duration::from_secs(5);

pub const UPDATED_EVENT: &str = "job-updated";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JobKind {
    OrdersVisibility {
        visible: bool,
    },
    OrdersDelete,
    Reprice,
    /// Mirror of the workbench run; its engine stays in charge.
    Workbench,
    /// Mirror of the price refresh worker: progress only.
    PricesLoad,
    /// Reads the order book of each item (read only, no write to warframe.market); item id = slug.
    MarketLoad,
}

impl JobKind {
    /// Run and journaled by this engine (the others are mirrors).
    fn owned(&self) -> bool {
        !matches!(self, Self::Workbench | Self::PricesLoad)
    }

    /// Writes orders: one at a time, and the my-orders poll waits. A market load only reads.
    fn writes(&self) -> bool {
        self.owned() && *self != Self::MarketLoad
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Running,
    /// Stop asked: the item in flight finishes, none starts.
    Stopping,
    /// No connection: paused on the item that hit the wall, retried by itself until the network is back.
    Offline,
    Done,
    /// The app closed mid-run (or the session died): the pending items wait for the user.
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JobItem {
    /// The order id.
    pub id: String,
    pub name: String,
    /// The new price of a `Reprice` item.
    pub platinum: Option<u32>,
    pub status: ItemStatus,
    /// Why a failed item failed.
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub state: JobState,
    pub total: u32,
    /// Items that went through.
    pub done: u32,
    pub failed: u32,
    /// Empty for the price refresh, which has no per-item story.
    pub items: Vec<JobItem>,
    pub created_at: f64,
    pub finished_at: Option<f64>,
}

impl Job {
    fn recount(&mut self) {
        let n = |s| self.items.iter().filter(|i| i.status == s).count() as u32;
        (self.done, self.failed) = (n(ItemStatus::Done), n(ItemStatus::Failed));
    }

    fn active(&self) -> bool {
        matches!(
            self.state,
            JobState::Running | JobState::Stopping | JobState::Offline
        )
    }

    /// Worth showing after a restart: not a clean finish.
    fn needs_attention(&self) -> bool {
        self.state != JobState::Done || self.failed > 0
    }
}

/// What changed, small: the counts, the state and at most one item. `job` rides along only when the job
/// is new, so the UI learns its items once.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JobUpdate {
    pub id: String,
    pub state: JobState,
    pub total: u32,
    pub done: u32,
    pub failed: u32,
    pub item: Option<ItemUpdate>,
    pub job: Option<Job>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemUpdate {
    pub id: String,
    pub status: ItemStatus,
    pub reason: Option<String>,
}

fn update(job: &Job, item: Option<&JobItem>) -> JobUpdate {
    JobUpdate {
        id: job.id.clone(),
        state: job.state,
        total: job.total,
        done: job.done,
        failed: job.failed,
        item: item.map(|i| ItemUpdate {
            id: i.id.clone(),
            status: i.status,
            reason: i.reason.clone(),
        }),
        job: None,
    }
}

/// Why an item did not go through. `halt`: every further item would hit the same wall (a dead session).
pub struct ItemError {
    pub reason: String,
    pub halt: bool,
    /// The connection is down (not an HTTP error): the item is not failed, the job pauses and tries again.
    pub offline: bool,
}

/// What the engine needs from the outside world; tests inject fakes so nothing touches the network.
pub trait JobHost: Send + Sync + 'static {
    fn run_item(
        &self,
        kind: &JobKind,
        item: &JobItem,
    ) -> impl Future<Output = Result<(), ItemError>> + Send;
    fn with_db<R>(&self, f: impl FnOnce(&Connection) -> R) -> R;
    fn emit(&self, update: &JobUpdate);
    /// An orders job runs (`true`) or ended (`false`): the my-orders poll waits meanwhile.
    fn hold(&self, on: bool);
}

pub struct Jobs<H> {
    host: H,
    /// Pause between two items, on top of the scheduler's budget.
    delay: Duration,
    /// First wait before trying an item again after a lost connection; it doubles up to six times this.
    offline_wait: Duration,
    /// Wakes a paused job early (Stop).
    wake: tokio::sync::Notify,
    jobs: Mutex<Vec<Job>>,
}

impl<H: JobHost> Jobs<H> {
    /// Loads the journal. A job that was running when the app died becomes `interrupted` with its
    /// pending items kept; nothing is started.
    pub fn new(host: H, delay: Duration) -> Self {
        let jobs = host.with_db(|c| {
            store::interrupt_unfinished(c)
                .and_then(|_| store::load(c))
                .unwrap_or_else(|e| {
                    tracing::warn!("[Jobs] journal unreadable: {e}");
                    vec![]
                })
        });
        Self {
            host,
            delay,
            offline_wait: OFFLINE_WAIT,
            wake: tokio::sync::Notify::new(),
            jobs: Mutex::new(jobs),
        }
    }

    #[cfg(test)]
    fn with_offline_wait(mut self, wait: Duration) -> Self {
        self.offline_wait = wait;
        self
    }

    /// The jobs the panel shows at startup: running, interrupted or with failures.
    pub fn list(&self) -> Vec<Job> {
        let jobs = self.jobs.lock().unwrap();
        jobs.iter()
            .filter(|j| j.needs_attention())
            .cloned()
            .collect()
    }

    fn announce_new(&self, job: &Job) {
        self.host.emit(&JobUpdate {
            job: Some(job.clone()),
            ..update(job, None)
        });
    }

    /// Journals a new job and runs it in the background; returns its id.
    pub fn start(self: &Arc<Self>, kind: JobKind, items: Vec<JobItem>) -> Result<String, String> {
        if items.is_empty() {
            return Err("Nothing to do.".into());
        }
        let job = {
            let mut jobs = self.jobs.lock().unwrap();
            // One job per class at a time: orders writes stay in sequence, loads do not pile up.
            let same = |j: &Job| j.kind.owned() && j.kind.writes() == kind.writes();
            if kind.owned() && jobs.iter().any(|j| same(j) && j.active()) {
                return Err("Another job is still running.".into());
            }
            let mut job = Job {
                id: uuid::Uuid::new_v4().to_string(),
                kind,
                state: JobState::Running,
                total: items.len() as u32,
                done: 0,
                failed: 0,
                items,
                created_at: now_ms_f64(),
                finished_at: None,
            };
            job.recount();
            self.host
                .with_db(|c| store::insert(c, &job))
                .map_err(|e| format!("journal write failed: {e}"))?;
            jobs.push(job.clone());
            job
        };
        self.announce_new(&job);
        tauri::async_runtime::spawn(Arc::clone(self).run(job.id.clone()));
        Ok(job.id)
    }

    fn save_state(&self, job: &Job) {
        if let Err(e) = self
            .host
            .with_db(|c| store::set_state(c, &job.id, job.state, job.finished_at))
        {
            tracing::error!("[Jobs] failed to persist job state: {e}");
        }
    }

    /// Runs the pending items in order. Returns the state it ended in.
    async fn run(self: Arc<Self>, id: String) {
        let writes = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .any(|j| j.id == id && j.kind.writes());
        if writes {
            self.host.hold(true);
        }
        let mut halted = false;
        let mut wait = self.offline_wait;
        loop {
            // Take the next pending item, unless a stop was asked.
            let next = {
                let jobs = self.jobs.lock().unwrap();
                jobs.iter().find(|j| j.id == id).and_then(|j| {
                    matches!(j.state, JobState::Running | JobState::Offline)
                        .then(|| j.items.iter().position(|i| i.status == ItemStatus::Pending))
                        .flatten()
                        .map(|idx| (idx, j.kind.clone(), j.items[idx].clone()))
                })
            };
            let Some((idx, kind, item)) = next else { break };

            // Write-ahead: `running` is durable before the request leaves.
            if let Err(e) = self
                .host
                .with_db(|c| store::set_item(c, &id, idx, ItemStatus::Running, None))
            {
                tracing::error!("[Jobs] journal write failed: {e}");
                halted = true;
                break;
            }
            self.set_item(&id, idx, ItemStatus::Running, None);
            let outcome = self.host.run_item(&kind, &item).await;
            if outcome.as_ref().is_err_and(|e| e.offline) {
                // Not a failure: the item goes back to the front and the job waits for the network.
                if let Err(e) = self
                    .host
                    .with_db(|c| store::set_item(c, &id, idx, ItemStatus::Pending, None))
                {
                    tracing::error!("[Jobs] failed to persist a paused item: {e}");
                }
                self.set_item(&id, idx, ItemStatus::Pending, None);
                if self.pause(&id) {
                    tokio::select! {
                        () = tokio::time::sleep(wait) => {}
                        () = self.wake.notified() => {}
                    }
                    wait = (wait * 2).min(self.offline_wait * 6);
                }
                continue;
            }
            wait = self.offline_wait;
            self.resume(&id);
            let (status, reason) = match &outcome {
                Ok(()) => (ItemStatus::Done, None),
                Err(e) => (ItemStatus::Failed, Some(e.reason.clone())),
            };
            if let Err(e) = self
                .host
                .with_db(|c| store::set_item(c, &id, idx, status, reason.as_deref()))
            {
                tracing::error!("[Jobs] failed to persist an item outcome: {e}");
            }
            self.set_item(&id, idx, status, reason);
            if outcome.as_ref().is_err_and(|e| e.halt) {
                halted = true;
                break;
            }
            tokio::time::sleep(self.delay).await;
        }
        if writes {
            self.host.hold(false);
        }
        self.finish(&id, halted);
        tracing::info!("[Jobs] job finished");
    }

    /// Running -> Offline. False when a stop was asked meanwhile (no waiting then).
    fn pause(&self, id: &str) -> bool {
        self.switch(id, JobState::Running, JobState::Offline)
    }

    /// Offline -> Running once an item went through again.
    fn resume(&self, id: &str) {
        self.switch(id, JobState::Offline, JobState::Running);
    }

    fn switch(&self, id: &str, from: JobState, to: JobState) -> bool {
        let job = {
            let mut jobs = self.jobs.lock().unwrap();
            match jobs.iter_mut().find(|j| j.id == id) {
                Some(j) if j.state == from => {
                    j.state = to;
                    j.clone()
                }
                Some(j) => return j.state == to,
                None => return false,
            }
        };
        self.save_state(&job);
        self.host.emit(&update(&job, None));
        true
    }

    /// Sets an item's status, keeps the counts and announces it.
    fn set_item(&self, id: &str, idx: usize, status: ItemStatus, reason: Option<String>) {
        let up = {
            let mut jobs = self.jobs.lock().unwrap();
            let Some(job) = jobs.iter_mut().find(|j| j.id == id) else {
                return;
            };
            job.items[idx].status = status;
            job.items[idx].reason = reason;
            job.recount();
            update(job, Some(&job.items[idx]))
        };
        self.host.emit(&up);
    }

    /// Ends a run. A dead session or a stop with items left makes it `interrupted`: the rest stays
    /// pending for a click on Continue, as after an app close.
    fn finish(&self, id: &str, halted: bool) {
        let job = {
            let mut jobs = self.jobs.lock().unwrap();
            let Some(job) = jobs.iter_mut().find(|j| j.id == id) else {
                return;
            };
            let stopped = job.state == JobState::Stopping;
            let left = job.items.iter().any(|i| i.status == ItemStatus::Pending);
            job.state = if halted || (stopped && left) {
                JobState::Interrupted
            } else {
                JobState::Done
            };
            job.finished_at = Some(now_ms_f64());
            job.clone()
        };
        self.save_state(&job);
        self.host.emit(&update(&job, None));
    }

    /// Stop after the item in flight. Returns the kind so the caller can also stop a mirrored engine.
    pub fn stop(&self, id: &str) -> Result<JobKind, String> {
        let job = {
            let mut jobs = self.jobs.lock().unwrap();
            let job = jobs.iter_mut().find(|j| j.id == id).ok_or("No such job.")?;
            if !matches!(job.state, JobState::Running | JobState::Offline) {
                return Err("The job is not running.".into());
            }
            job.state = JobState::Stopping;
            job.clone()
        };
        self.host.emit(&update(&job, None));
        self.wake.notify_one();
        Ok(job.kind)
    }

    /// Takes a finished or interrupted job out of the list and returns it.
    fn take(&self, id: &str) -> Result<Job, String> {
        let mut jobs = self.jobs.lock().unwrap();
        let at = jobs.iter().position(|j| j.id == id).ok_or("No such job.")?;
        if jobs[at].active() {
            return Err("The job is still running.".into());
        }
        let job = jobs.remove(at);
        if let Err(e) = self.host.with_db(|c| store::delete(c, id)) {
            tracing::error!("[Jobs] failed to delete a job: {e}");
        }
        Ok(job)
    }

    pub fn dismiss(&self, id: &str) -> Result<(), String> {
        self.take(id).map(|_| ())
    }

    /// A new job with only the items in `status` of `id`, which leaves the list.
    fn redo(self: &Arc<Self>, id: &str, status: ItemStatus) -> Result<String, String> {
        let job = {
            let jobs = self.jobs.lock().unwrap();
            jobs.iter()
                .find(|j| j.id == id)
                .filter(|j| !j.active() && j.kind.owned())
                .cloned()
                .ok_or("That job cannot be repeated.")?
        };
        let items: Vec<JobItem> = job
            .items
            .iter()
            .filter(|i| i.status == status)
            .map(|i| JobItem {
                status: ItemStatus::Pending,
                reason: None,
                ..i.clone()
            })
            .collect();
        let new = self.start(job.kind, items)?;
        let _ = self.take(id);
        Ok(new)
    }

    /// Retry: a new job with only the failed items.
    pub fn retry_failed(self: &Arc<Self>, id: &str) -> Result<String, String> {
        self.redo(id, ItemStatus::Failed)
    }

    /// Continue an interrupted job: a new job with the items that never ran. Only ever on a click.
    pub fn continue_job(self: &Arc<Self>, id: &str) -> Result<String, String> {
        let interrupted = self
            .jobs
            .lock()
            .unwrap()
            .iter()
            .any(|j| j.id == id && j.state == JobState::Interrupted);
        if !interrupted {
            return Err("The job was not interrupted.".into());
        }
        self.redo(id, ItemStatus::Pending)
    }

    /// Publishes a job another engine runs (the workbench, the price refresh): not journaled, no
    /// per-item story beyond `items`. Announces what changed since the last call.
    /// `done` counts when there are no `items` to count.
    pub fn mirror(
        &self,
        id: &str,
        kind: JobKind,
        state: JobState,
        (done, total): (u32, u32),
        items: Vec<JobItem>,
    ) {
        let mut job = Job {
            id: id.into(),
            kind,
            state,
            total,
            done,
            failed: 0,
            items,
            created_at: now_ms_f64(),
            finished_at: None,
        };
        if !job.items.is_empty() {
            job.recount();
        }
        let ups = {
            let mut jobs = self.jobs.lock().unwrap();
            let ups = match jobs.iter().position(|j| j.id == id) {
                None => {
                    job.finished_at = (state == JobState::Done).then(now_ms_f64);
                    jobs.push(job.clone());
                    vec![JobUpdate {
                        job: Some(job.clone()),
                        ..update(&job, None)
                    }]
                }
                Some(at) => {
                    let old = &mut jobs[at];
                    let mut ups: Vec<_> = old
                        .items
                        .iter()
                        .zip(&job.items)
                        .filter(|(o, n)| (o.status, &o.reason) != (n.status, &n.reason))
                        .map(|(_, n)| update(&job, Some(n)))
                        .collect();
                    if ups.is_empty()
                        && (old.state, old.total, old.done) != (state, total, job.done)
                    {
                        ups.push(update(&job, None));
                    }
                    job.created_at = old.created_at;
                    job.finished_at = old
                        .finished_at
                        .or((state == JobState::Done).then(now_ms_f64));
                    *old = job.clone();
                    ups
                }
            };
            // A finished price refresh leaves nothing to show.
            if job.kind == JobKind::PricesLoad && state == JobState::Done {
                jobs.retain(|j| j.id != id);
            }
            ups
        };
        ups.iter().for_each(|u| self.host.emit(u));
    }

    /// Whether a mirrored job is in the list (so a finished, dismissed run is not brought back).
    pub fn has(&self, id: &str) -> bool {
        self.jobs.lock().unwrap().iter().any(|j| j.id == id)
    }
}

#[cfg(test)]
mod tests;
