//! The engine against a fake host: no network, no Tauri. The delay between items is zero.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    /// The connection is down: every item answers with a network error.
    down: AtomicBool,
    tries: AtomicU32,
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
                        offline: false,
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
        down: AtomicBool::new(false),
        tries: AtomicU32::new(0),
    }
}

impl JobHost for std::sync::Arc<Fake> {
    async fn run_item(&self, _: &JobKind, item: &JobItem) -> Result<(), ItemError> {
        if self.down.load(Ordering::SeqCst) {
            self.tries.fetch_add(1, Ordering::SeqCst);
            return Err(ItemError {
                reason: "network error".into(),
                halt: false,
                offline: true,
            });
        }
        self.ran.lock().unwrap().push(item.id.clone());
        if self.slow.contains(&item.id) {
            self.flying.fetch_add(1, Ordering::SeqCst);
            self.release.notified().await;
        }
        match self.fail.get(&item.id) {
            Some(e) => Err(ItemError {
                reason: e.reason.clone(),
                halt: e.halt,
                offline: false,
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

fn engine_offline(f: Fake) -> (std::sync::Arc<Fake>, Arc<Jobs<std::sync::Arc<Fake>>>) {
    let f = std::sync::Arc::new(f);
    let jobs = Jobs::new(f.clone(), Duration::ZERO).with_offline_wait(Duration::from_millis(10));
    (f, Arc::new(jobs))
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
            ItemStatus::Pending,
            ItemStatus::Pending
        ]
    );
    assert_eq!(job.state, JobState::Interrupted, "as after an app close");
    assert!(jobs.stop(&id).is_err(), "an ended job cannot be stopped");
    // The journal agrees, and Continue runs what was left.
    let kept = f.with_db(store::load).unwrap();
    assert_eq!(kept[0].items[3].status, ItemStatus::Pending);
    assert_eq!(kept[0].state, JobState::Interrupted);
    let next = jobs.continue_job(&id).unwrap();
    finished(&jobs, &next).await;
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b", "c", "d"]);
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

#[tokio::test]
async fn a_market_load_reads_in_order_records_reasons_and_never_holds_the_poll() {
    let (f, jobs) = engine(fake(&[("b", "HTTP 503", false)], &[]));
    let id = jobs
        .start(JobKind::MarketLoad, items(&["a", "b", "c"]))
        .unwrap();
    let job = finished(&jobs, &id).await;
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b", "c"]);
    assert_eq!((job.done, job.failed), (2, 1));
    assert_eq!(job.items[1].reason.as_deref(), Some("HTTP 503"));
    assert!(
        f.holds.lock().unwrap().is_empty(),
        "a read does not hold the poll"
    );

    // Retry runs only the failed one, as a new job.
    let again = jobs.retry_failed(&id).unwrap();
    finished(&jobs, &again).await;
    assert_eq!(f.ran.lock().unwrap()[3..], ["b"]);
}

#[tokio::test]
async fn a_market_load_runs_beside_an_orders_job_but_not_beside_another_load() {
    let (f, jobs) = engine(fake(&[], &["w"]));
    let orders = jobs.start(HIDE, items(&["w"])).unwrap();
    until("the orders job in flight", || {
        f.flying.load(Ordering::SeqCst) == 1
    })
    .await;
    let load = jobs.start(JobKind::MarketLoad, items(&["a"])).unwrap();
    finished(&jobs, &load).await;
    assert!(
        jobs.start(JobKind::MarketLoad, items(&["slow"])).is_ok(),
        "the first load ended"
    );
    f.release.notify_one();
    finished(&jobs, &orders).await;
}

#[tokio::test]
async fn a_market_load_stops_after_the_item_in_flight() {
    let (f, jobs) = engine(fake(&[], &["a"]));
    let id = jobs
        .start(JobKind::MarketLoad, items(&["a", "b", "c"]))
        .unwrap();
    until("the first read in flight", || {
        f.flying.load(Ordering::SeqCst) == 1
    })
    .await;
    assert!(
        jobs.start(JobKind::MarketLoad, items(&["z"])).is_err(),
        "one load at a time"
    );
    jobs.stop(&id).unwrap();
    f.release.notify_one();
    let job = finished(&jobs, &id).await;
    assert_eq!(*f.ran.lock().unwrap(), ["a"]);
    assert_eq!((job.done, job.state), (1, JobState::Interrupted));
    assert_eq!(job.items[2].status, ItemStatus::Pending);
}

#[tokio::test]
async fn a_lost_connection_pauses_the_job_without_failing_the_item_and_resumes_by_itself() {
    let (f, jobs) = engine_offline(fake(&[], &[]));
    f.down.store(true, Ordering::SeqCst);
    let id = jobs.start(HIDE, items(&["a", "b"])).unwrap();
    until("the job to pause", || {
        job_of(&jobs, &id).is_some_and(|j| j.state == JobState::Offline)
            && f.tries.load(Ordering::SeqCst) >= 2
    })
    .await;
    let paused = job_of(&jobs, &id).unwrap();
    assert_eq!((paused.done, paused.failed), (0, 0), "nothing failed");
    assert_eq!(
        paused.items[0].status,
        ItemStatus::Pending,
        "the item waits at the front"
    );
    assert!(f.ran.lock().unwrap().is_empty());

    // The connection is back: it goes on without a click, in order, and ends clean.
    f.down.store(false, Ordering::SeqCst);
    let job = finished(&jobs, &id).await;
    assert_eq!((job.state, job.done, job.failed), (JobState::Done, 2, 0));
    assert_eq!(*f.ran.lock().unwrap(), ["a", "b"]);
    assert!(
        f.updates
            .lock()
            .unwrap()
            .iter()
            .any(|u| u.state == JobState::Offline),
        "the pause was announced"
    );
}

#[tokio::test]
async fn stop_while_offline_ends_the_job_at_once() {
    // A long wait: only the wake-up from Stop can end it in time.
    let f = std::sync::Arc::new(fake(&[], &[]));
    let jobs =
        Arc::new(Jobs::new(f.clone(), Duration::ZERO).with_offline_wait(Duration::from_secs(30)));
    f.down.store(true, Ordering::SeqCst);
    let id = jobs.start(HIDE, items(&["a", "b"])).unwrap();
    until("the job to pause", || {
        job_of(&jobs, &id).is_some_and(|j| j.state == JobState::Offline)
    })
    .await;
    jobs.stop(&id).unwrap();
    let job = finished(&jobs, &id).await;
    assert_eq!((job.failed, job.state), (0, JobState::Interrupted));
    assert!(job.items.iter().all(|i| i.status == ItemStatus::Pending));
    assert!(f.ran.lock().unwrap().is_empty());
}

#[tokio::test]
async fn startup_turns_an_offline_job_into_an_interrupted_one() {
    let conn = mem_conn();
    let job = Job {
        id: "j1".into(),
        kind: HIDE,
        state: JobState::Offline,
        total: 2,
        done: 0,
        failed: 0,
        items: items(&["a", "b"]),
        created_at: 1.0,
        finished_at: None,
    };
    store::insert(&conn, &job).unwrap();
    store::set_state(&conn, "j1", JobState::Offline, None).unwrap();
    let f = std::sync::Arc::new(Fake {
        conn: Mutex::new(conn),
        ..fake(&[], &[])
    });
    let jobs = Jobs::new(f.clone(), Duration::ZERO);
    assert_eq!(jobs.list()[0].state, JobState::Interrupted);
}
