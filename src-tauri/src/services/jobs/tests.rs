//! The engine against a fake host: no network, no Tauri. The delay between items is zero.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use tokio::sync::Notify;

use super::*;
use crate::db::mem_conn;

struct Fake {
    conn: Mutex<Connection>,
    /// Item id -> the error it fails with; the rest succeed.
    fail: HashMap<String, ItemError>,
    ran: Mutex<Vec<String>>,
    updates: Mutex<Vec<JobUpdate>>,
    holds: Mutex<Vec<bool>>,
    /// Items in this set wait for `release` while "in flight".
    slow: Vec<String>,
    release: Notify,
    flying: AtomicU32,
}

fn fake(fail: &[(&str, &str, bool)], slow: &[&str]) -> Fake {
    Fake {
        conn: Mutex::new(mem_conn()),
        fail: fail
            .iter()
            .map(|(id, reason, halt)| {
                (
                    id.to_string(),
                    ItemError {
                        reason: reason.to_string(),
                        halt: *halt,
                    },
                )
            })
            .collect(),
        ran: Mutex::default(),
        updates: Mutex::default(),
        holds: Mutex::default(),
        slow: slow.iter().map(|s| s.to_string()).collect(),
        release: Notify::new(),
        flying: AtomicU32::new(0),
    }
}

impl JobHost for std::sync::Arc<Fake> {
    async fn run_item(&self, _: &JobKind, item: &JobItem) -> Result<(), ItemError> {
        self.ran.lock().unwrap().push(item.id.clone());
        if self.slow.contains(&item.id) {
            self.flying.fetch_add(1, Ordering::SeqCst);
            self.release.notified().await;
        }
        match self.fail.get(&item.id) {
            Some(e) => Err(ItemError {
                reason: e.reason.clone(),
                halt: e.halt,
            }),
            None => Ok(()),
        }
    }

    fn with_db<R>(&self, f: impl FnOnce(&Connection) -> R) -> R {
        f(&self.conn.lock().unwrap())
    }

    fn emit(&self, u: &JobUpdate) {
        self.updates.lock().unwrap().push(u.clone());
    }

    fn hold(&self, on: bool) {
        self.holds.lock().unwrap().push(on);
    }
}

fn item(id: &str) -> JobItem {
    JobItem {
        id: id.into(),
        name: format!("Item {id}"),
        platinum: Some(7),
        status: ItemStatus::Pending,
        reason: None,
    }
}

fn items(ids: &[&str]) -> Vec<JobItem> {
    ids.iter().map(|i| item(i)).collect()
}

fn engine(f: Fake) -> (std::sync::Arc<Fake>, Arc<Jobs<std::sync::Arc<Fake>>>) {
    let f = std::sync::Arc::new(f);
    (f.clone(), Arc::new(Jobs::new(f, Duration::ZERO)))
}

async fn until(what: &str, mut ok: impl FnMut() -> bool) {
    for _ in 0..500 {
        if ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

fn job_of(jobs: &Jobs<std::sync::Arc<Fake>>, id: &str) -> Option<Job> {
    jobs.jobs
        .lock()
        .unwrap()
        .iter()
        .find(|j| j.id == id)
        .cloned()
}

async fn finished(jobs: &Jobs<std::sync::Arc<Fake>>, id: &str) -> Job {
    until("the job to end", || {
        job_of(jobs, id).is_some_and(|j| !j.active())
    })
    .await;
    job_of(jobs, id).unwrap()
}

const HIDE: JobKind = JobKind::OrdersVisibility { visible: false };

#[tokio::test]
async fn runs_items_in_order_and_emits_a_small_update_per_change() {
    let (f, jobs) = engine(fake(&[], &[]));
    let id = jobs.start(HIDE, items(&["a", "b", "c"])).unwrap();
    let job = finished(&jobs, &id).await;
    // The state flips before the last announcement goes out.
    until("the last announcement", || {
        f.updates
            .lock()
            .unwrap()
            .last()
            .is_some_and(|u| u.state == JobState::Done)
    })
    .await;
    assert_eq!(
        (job.state, job.done, job.failed, job.total),
        (JobState::Done, 3, 0, 3)
    );
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b", "c"]);
    assert_eq!(
        *f.holds.lock().unwrap(),
        [true, false],
        "the poll waits for the run"
    );

    let ups = f.updates.lock().unwrap();
    assert!(ups[0].job.is_some(), "the job travels once, when it starts");
    assert!(ups[1..].iter().all(|u| u.job.is_none()));
    let seen: Vec<_> = ups
        .iter()
        .filter_map(|u| u.item.as_ref().map(|i| (i.id.as_str(), i.status)))
        .collect();
    assert_eq!(
        seen,
        [
            ("a", ItemStatus::Running),
            ("a", ItemStatus::Done),
            ("b", ItemStatus::Running),
            ("b", ItemStatus::Done),
            ("c", ItemStatus::Running),
            ("c", ItemStatus::Done),
        ]
    );
    assert_eq!(ups.last().unwrap().state, JobState::Done);
}

#[tokio::test]
async fn stop_lets_the_item_in_flight_finish_and_starts_no_other() {
    let (f, jobs) = engine(fake(&[], &["b"]));
    let id = jobs.start(HIDE, items(&["a", "b", "c", "d"])).unwrap();
    until("b to be in flight", || f.flying.load(Ordering::SeqCst) == 1).await;
    assert_eq!(jobs.stop(&id).unwrap(), HIDE);
    assert_eq!(job_of(&jobs, &id).unwrap().state, JobState::Stopping);
    f.release.notify_one();

    let job = finished(&jobs, &id).await;
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b"]);
    let st: Vec<_> = job.items.iter().map(|i| i.status).collect();
    assert_eq!(
        st,
        [
            ItemStatus::Done,
            ItemStatus::Done,
            ItemStatus::Skipped,
            ItemStatus::Skipped
        ]
    );
    assert_eq!(job.state, JobState::Done);
    assert!(jobs.stop(&id).is_err(), "a finished job cannot be stopped");
    // The journal agrees: nothing is left pending to resume.
    let kept = f.with_db(store::load).unwrap();
    assert_eq!(kept[0].items[3].status, ItemStatus::Skipped);
}

#[tokio::test]
async fn a_failed_item_keeps_its_reason_and_the_job_finishes_with_failures() {
    let (f, jobs) = engine(fake(&[("b", "order not found", false)], &[]));
    let id = jobs.start(HIDE, items(&["a", "b", "c"])).unwrap();
    let job = finished(&jobs, &id).await;
    assert_eq!((job.state, job.done, job.failed), (JobState::Done, 2, 1));
    assert_eq!(job.items[1].reason.as_deref(), Some("order not found"));
    assert_eq!(
        *f.ran.lock().unwrap(),
        ["a", "b", "c"],
        "one failure does not stop the rest"
    );
    assert_eq!(
        jobs.list().len(),
        1,
        "a job with failures stays for the user"
    );
}

#[tokio::test]
async fn retry_makes_a_new_job_with_only_the_failed_items() {
    let (f, jobs) = engine(fake(&[("b", "boom", false), ("d", "boom", false)], &[]));
    let id = jobs.start(HIDE, items(&["a", "b", "c", "d"])).unwrap();
    finished(&jobs, &id).await;
    f.ran.lock().unwrap().clear();

    let retry = jobs.retry_failed(&id).unwrap();
    assert_ne!(retry, id);
    assert!(job_of(&jobs, &id).is_none(), "the old job is replaced");
    let job = finished(&jobs, &retry).await;
    assert_eq!(*f.ran.lock().unwrap(), ["b", "d"]);
    assert_eq!((job.total, job.kind.clone()), (2, HIDE));
    assert!(jobs.retry_failed("nope").is_err());
}

#[tokio::test]
async fn a_dead_session_interrupts_the_job_and_continue_runs_what_is_left() {
    let (f, jobs) = engine(fake(&[("b", "signed out", true)], &[]));
    let id = jobs
        .start(JobKind::Reprice, items(&["a", "b", "c", "d"]))
        .unwrap();
    let job = finished(&jobs, &id).await;
    assert_eq!(job.state, JobState::Interrupted);
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b"]);
    assert_eq!(job.items[1].status, ItemStatus::Failed);

    // Nothing resumes by itself.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(f.ran.lock().unwrap().len(), 2);

    let next = jobs.continue_job(&id).unwrap();
    let job = finished(&jobs, &next).await;
    assert_eq!((job.total, job.kind), (2, JobKind::Reprice));
    assert_eq!(
        job.items[0].platinum,
        Some(7),
        "the new price travels with the item"
    );
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b", "c", "d"]);
    assert!(
        jobs.continue_job(&next).is_err(),
        "only an interrupted job continues"
    );
}

#[tokio::test]
async fn startup_marks_unfinished_jobs_interrupted_and_never_runs_them() {
    let conn = mem_conn();
    let mut job = Job {
        id: "j1".into(),
        kind: HIDE,
        state: JobState::Running,
        total: 3,
        done: 0,
        failed: 0,
        items: items(&["a", "b", "c"]),
        created_at: 1.0,
        finished_at: None,
    };
    job.items[0].status = ItemStatus::Done;
    job.items[1].status = ItemStatus::Running; // in flight when the app died
    store::insert(&conn, &job).unwrap();

    let f = std::sync::Arc::new(Fake {
        conn: Mutex::new(conn),
        ..fake(&[], &[])
    });
    let jobs = Arc::new(Jobs::new(f.clone(), Duration::ZERO));
    tokio::time::sleep(Duration::from_millis(50)).await;

    let list = jobs.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].state, JobState::Interrupted);
    let st: Vec<_> = list[0].items.iter().map(|i| i.status).collect();
    assert_eq!(
        st,
        [ItemStatus::Done, ItemStatus::Pending, ItemStatus::Pending]
    );
    assert_eq!((list[0].done, list[0].total), (1, 3));
    assert!(f.ran.lock().unwrap().is_empty(), "nothing ran");
    assert!(f.holds.lock().unwrap().is_empty());

    // Only a click continues it, with the two that never finished.
    let next = jobs.continue_job("j1").unwrap();
    finished(&jobs, &next).await;
    assert_eq!(*f.ran.lock().unwrap(), ["b", "c"]);
}

#[tokio::test]
async fn one_orders_job_runs_at_a_time_and_a_running_one_cannot_be_dismissed() {
    let (f, jobs) = engine(fake(&[], &["a"]));
    let id = jobs.start(HIDE, items(&["a"])).unwrap();
    until("a to be in flight", || f.flying.load(Ordering::SeqCst) == 1).await;
    assert!(jobs.start(JobKind::OrdersDelete, items(&["z"])).is_err());
    assert!(jobs.dismiss(&id).is_err());
    assert!(
        jobs.start(HIDE, vec![]).is_err(),
        "nothing to do is refused"
    );
    f.release.notify_one();
    finished(&jobs, &id).await;
    jobs.dismiss(&id).unwrap();
    assert!(jobs.list().is_empty());
    assert!(
        f.with_db(store::load).unwrap().is_empty(),
        "dismissed from the journal too"
    );
}

#[test]
fn pruning_drops_finished_jobs_after_a_week_and_keeps_the_waiting_ones() {
    let conn = mem_conn();
    let mk = |id: &str, state, at: Option<f64>| Job {
        id: id.into(),
        kind: HIDE,
        state,
        total: 1,
        done: 0,
        failed: 0,
        items: items(&["a"]),
        created_at: 0.0,
        finished_at: at,
    };
    for j in [
        mk("old", JobState::Done, Some(10.0)),
        mk("new", JobState::Done, Some(1000.0)),
        mk("wait", JobState::Interrupted, Some(10.0)),
    ] {
        store::insert(&conn, &j).unwrap();
        store::set_state(&conn, &j.id, j.state, j.finished_at).unwrap();
    }
    assert_eq!(store::prune(&conn, 500.0).unwrap(), 1);
    let left: Vec<_> = store::load(&conn)
        .unwrap()
        .into_iter()
        .map(|j| j.id)
        .collect();
    assert_eq!(left, ["new", "wait"]);
}

#[test]
fn a_mirrored_job_announces_only_what_changed_and_a_dismissed_one_stays_gone() {
    let (f, jobs) = engine(fake(&[], &[]));
    let mut rows = items(&["a", "b"]);
    jobs.mirror(
        "w",
        JobKind::Workbench,
        JobState::Running,
        (0, 2),
        rows.clone(),
    );
    rows[0].status = ItemStatus::Done;
    jobs.mirror(
        "w",
        JobKind::Workbench,
        JobState::Running,
        (0, 2),
        rows.clone(),
    );
    jobs.mirror(
        "w",
        JobKind::Workbench,
        JobState::Running,
        (0, 2),
        rows.clone(),
    );
    {
        let ups = f.updates.lock().unwrap();
        assert_eq!(ups.len(), 2, "the repeat said nothing new");
        assert!(ups[0].job.is_some() && ups[1].job.is_none());
        assert_eq!(
            (ups[1].done, ups[1].item.as_ref().unwrap().status),
            (1, ItemStatus::Done)
        );
    }
    assert!(
        jobs.stop("w").is_ok(),
        "stopping a mirrored job marks it, its owner does the rest"
    );
    jobs.mirror("w", JobKind::Workbench, JobState::Done, (0, 2), rows);
    assert!(
        jobs.retry_failed("w").is_err(),
        "a mirror is not repeated from here"
    );
    jobs.dismiss("w").unwrap();
    assert!(!jobs.has("w"));

    // The price refresh has counts only, and vanishes when done.
    jobs.mirror("p", JobKind::PricesLoad, JobState::Running, (3, 9), vec![]);
    assert_eq!(job_of(&jobs, "p").unwrap().done, 3);
    jobs.mirror("p", JobKind::PricesLoad, JobState::Done, (0, 0), vec![]);
    assert!(!jobs.has("p"));
}
