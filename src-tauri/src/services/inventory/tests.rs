//! Import, persistence and delta behaviour of the inventory core.

use crate::db::mem_db;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::delta::{Delta, DeltaKind};
use super::*;

#[derive(Default)]
struct Rec {
    updates: Mutex<Vec<Option<Value>>>,
}

impl Sink for Rec {
    fn updated(&self, data: Option<&Value>) {
        self.updates.lock().unwrap().push(data.cloned());
    }
    fn status(&self, _: &InventoryStatus) {}
}

fn dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "cf-inv-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn inv() -> (Inventory, Arc<Rec>) {
    let rec = Arc::new(Rec::default());
    (Inventory::new(rec.clone()), rec)
}

fn sample(count: i64) -> Value {
    json!({"Suits": [], "MiscItems": [{"ItemType": "/Lotus/A", "ItemCount": count}], "PremiumCredits": 10})
}

fn pick(name: &str) -> Delta {
    delta::from_pick(name, name, 1.0)
}

fn write_sample(count: i64) -> PathBuf {
    let f = dir().join("inventory.json");
    fs::write(&f, sample(count).to_string()).unwrap();
    f
}

#[test]
fn imports_a_json_file_and_survives_a_restart_with_its_deltas() {
    let (db, f) = (mem_db(), write_sample(3));
    let (a, _) = inv();
    assert!(a.import_path(&db, &f).is_some());
    assert!(a.add_deltas(&db, vec![pick("/Lotus/A")]));

    let (b, _) = inv();
    let loaded = b.load(&db).unwrap();
    assert_eq!(loaded["MiscItems"][0]["ItemCount"], 4);
    let st = b.status();
    assert_eq!(
        (st.source, st.found, st.picks_applied, st.trades_applied),
        (InventorySource::Manual, true, 1, 0)
    );
    assert!(st.imported_at.is_some());
}

#[test]
fn helper_fetch_becomes_the_snapshot_and_same_content_keeps_deltas() {
    let (db, f) = (mem_db(), write_sample(3));
    let (i, _) = inv();
    i.import_helper(&db, &f);
    i.add_deltas(&db, vec![pick("/Lotus/A")]);
    i.import_helper(&db, &f);
    let st = i.status();
    assert_eq!((st.source, st.picks_applied), (InventorySource::Helper, 1));
    fs::write(&f, sample(9).to_string()).unwrap();
    i.import_helper(&db, &f);
    assert_eq!(i.status().picks_applied, 0);
    assert_eq!(i.current().unwrap()["MiscItems"][0]["ItemCount"], 9);
    let (b, _) = inv();
    b.load(&db);
    assert_eq!(b.source(), InventorySource::Helper);
}

#[test]
fn a_new_import_drops_the_old_deltas() {
    let (db, f) = (mem_db(), write_sample(3));
    let (i, _) = inv();
    i.import_path(&db, &f);
    i.add_deltas(&db, vec![pick("/Lotus/A")]);
    i.import_path(&db, &f);
    assert_eq!(i.status().picks_applied, 0);
    assert_eq!(i.current().unwrap()["MiscItems"][0]["ItemCount"], 3);
}

#[test]
fn deltas_need_a_snapshot_and_clear_forgets_everything() {
    let (db, f) = (mem_db(), write_sample(1));
    let (i, rec) = inv();
    assert!(!i.add_deltas(&db, vec![pick("/Lotus/A")]));
    i.import_path(&db, &f);
    i.clear(&db);
    assert!(i.current().is_none() && !i.status().found);
    assert_eq!(rec.updates.lock().unwrap().last(), Some(&None));
    assert!(inv().0.load(&db).is_none());
}

#[test]
fn rejects_files_that_are_not_an_inventory() {
    let (db, d) = (mem_db(), dir());
    let (i, _) = inv();
    let bad = d.join("inventory.json");
    fs::write(&bad, r#"{"hello":"world"}"#).unwrap();
    assert!(i.import_path(&db, &bad).is_none());
    assert_eq!(i.status().last_error.unwrap().kind, ErrorKind::Parse);
    assert!(i.import_path(&db, &d.join("missing.json")).is_none());
    assert_eq!(i.status().last_error.unwrap().kind, ErrorKind::Read);
    assert!(!i.status().found);
}

#[test]
fn unwraps_an_inventory_envelope() {
    let (db, d) = (mem_db(), dir());
    let f = d.join("inventory.json");
    fs::write(
        &f,
        json!({"InventoryJson": sample(2).to_string()}).to_string(),
    )
    .unwrap();
    let (i, _) = inv();
    assert_eq!(
        i.import_path(&db, &f).unwrap()["MiscItems"][0]["ItemCount"],
        2
    );
}

#[test]
fn an_alecaframe_file_is_imported_and_reimported_when_it_changes() {
    let (db, d) = (mem_db(), dir());
    let f = d.join("lastData.dat");
    fs::write(&f, aleca::encrypt(&sample(1))).unwrap();
    let (i, rec) = inv();
    i.import_path(&db, &f).unwrap();
    assert_eq!(i.status().source, InventorySource::Aleca);

    // The size changes, so the stamp differs even on a coarse mtime clock.
    fs::write(&f, aleca::encrypt(&sample(7_000_000))).unwrap();
    // The first tick sees the change, the second confirms the writer is done.
    for _ in 0..2 {
        i.poll(&db);
    }
    assert_eq!(i.current().unwrap()["MiscItems"][0]["ItemCount"], 7_000_000);
    assert!(rec.updates.lock().unwrap().len() >= 2);
}

#[test]
fn a_game_sync_after_the_import_is_flagged_but_one_before_it_is_not() {
    let (db, f) = (mem_db(), write_sample(1));
    let (i, _) = inv();
    i.note_game_sync(1.0);
    i.import_path(&db, &f);
    assert!(!i.status().game_synced_since_import);
    i.note_game_sync(now_ms_f64() + 1000.0);
    assert!(i.status().game_synced_since_import);
}

#[test]
fn trades_count_once_per_trade() {
    let (db, f) = (mem_db(), write_sample(5));
    let (i, _) = inv();
    i.import_path(&db, &f);
    let row = |n: &str, count| Delta {
        at: 1.0,
        kind: DeltaKind::Trade,
        group: "t1".into(),
        unique_name: Some(n.into()),
        label: n.into(),
        count,
        platinum: 0,
        estimated: false,
    };
    i.add_deltas(&db, vec![row("/Lotus/A", -2), row("/Lotus/B", 1)]);
    assert_eq!(i.status().trades_applied, 1);
    assert_eq!(i.current().unwrap()["MiscItems"][0]["ItemCount"], 3);
}

#[test]
fn export_is_indented_inventory_json() {
    let (db, f) = (mem_db(), write_sample(1));
    let (i, _) = inv();
    assert_eq!(i.export_text(), Err(ExportError::NoInventory));
    i.import_path(&db, &f);
    assert!(i.export_text().unwrap().contains("\n  "));
}
