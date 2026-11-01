//! Ported from WFHelper's statsTracker / statsResources tests. A "restart" is a fresh `Tracker`
//! restored from what the store holds.
use crate::db::mem_conn;
use rusqlite::Connection;
use serde_json::{Value, json};

use super::tracker::{DailyStatEntry, StatResourceDay, Tracker, is_known, resource_ids};
use super::{StatsImportEntry, sanitize, store};

const MISC: &str = "/Lotus/Types/Items/MiscItems/";

fn inventory(plat: f64, misc: &[(&str, f64)]) -> Value {
    let rows: Vec<Value> = misc
        .iter()
        .map(|(name, n)| json!({"ItemType": format!("{MISC}{name}"), "ItemCount": n}))
        .collect();
    json!({"PremiumCredits": plat, "RegularCredits": 1_000_000, "FusionPoints": 5000, "MiscItems": rows})
}

fn day(entry: &DailyStatEntry, id: &str) -> Option<(f64, Option<f64>)> {
    entry.resources.get(id).map(|d| (d.delta, d.abs))
}

fn restarted(conn: &Connection, today: &str) -> Tracker {
    let mut t = Tracker::default();
    let saved = store::load_all(conn).unwrap();
    t.restore(saved.iter().find(|e| e.date == today).unwrap());
    t
}

#[test]
fn counts_one_consumed_relic_once_when_collections_duplicate_it() {
    let relic = "/Lotus/Relics/LithA1Intact";
    let with = |n: u32| json!({"LevelKeys": [{"ItemType": relic, "ItemCount": n}], "MiscItems": [{"ItemType": relic, "ItemCount": n}]});
    let mut t = Tracker::default();
    t.on_inventory(&with(5), "2026-08-30");
    let e = t.on_inventory(&with(4), "2026-08-30");
    assert_eq!(e.relics_opened, 1);
}

#[test]
fn catalog_ids_are_unique_and_keep_the_persisted_keys() {
    let ids: Vec<_> = resource_ids().collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len());
    for id in ["plat", "credits", "endo", "ducats", "aya", "vitus", "kuva"] {
        assert!(is_known(id), "{id}");
    }
}

#[test]
fn tracks_a_kuva_delta_with_the_balance() {
    let mut t = Tracker::default();
    t.on_inventory(
        &inventory(100.0, &[("PrimeBucks", 300.0), ("Kuva", 12_000.0)]),
        "d",
    );
    let e = t.on_inventory(
        &inventory(130.0, &[("PrimeBucks", 260.0), ("Kuva", 24_500.0)]),
        "d",
    );
    assert_eq!(day(&e, "kuva"), Some((12_500.0, Some(24_500.0))));
    assert_eq!(day(&e, "plat"), Some((30.0, Some(130.0))));
    assert_eq!(day(&e, "ducats"), Some((-40.0, Some(260.0))));
    let s = t.session();
    assert_eq!(s.resources["kuva"].delta, 12_500.0);
    assert_eq!(s.resources["plat"].current, Some(130.0));
    assert!(s.has_data);
}

#[test]
fn a_resource_missing_from_a_reported_misc_array_reads_as_zero() {
    let mut t = Tracker::default();
    let e = t.on_inventory(&inventory(100.0, &[]), "d");
    assert_eq!(day(&e, "plat"), Some((0.0, Some(100.0))));
    assert_eq!(day(&e, "kuva"), Some((0.0, Some(0.0))));
    // A top-level field the payload never carried stays unreported.
    assert_eq!(day(&e, "regalAya"), None);
}

#[test]
fn keeps_the_delta_and_balance_when_a_resource_is_spent_to_zero() {
    let mut t = Tracker::default();
    t.on_inventory(&inventory(100.0, &[("Forma", 3.0)]), "d");
    let e = t.on_inventory(&inventory(100.0, &[("Forma", 1.0)]), "d");
    assert_eq!(day(&e, "forma"), Some((-2.0, Some(1.0))));
    let e = t.on_inventory(&inventory(100.0, &[]), "d");
    assert_eq!(day(&e, "forma"), Some((-3.0, Some(0.0))));
}

#[test]
fn resumes_the_delta_after_a_restart_that_finds_the_resource_at_zero() {
    let conn = mem_conn();
    let mut t = Tracker::default();
    t.on_inventory(&inventory(100.0, &[("Forma", 3.0)]), "d");
    let e = t.on_inventory(&inventory(100.0, &[]), "d");
    store::upsert(&conn, &e).unwrap();

    let mut t = restarted(&conn, "d");
    let e = t.on_inventory(&inventory(100.0, &[]), "d");
    assert_eq!(day(&e, "forma"), Some((-3.0, Some(0.0))));
}

#[test]
fn a_payload_without_misc_items_is_unknown_not_zero() {
    let mut t = Tracker::default();
    t.on_inventory(&inventory(100.0, &[("Forma", 3.0)]), "d");
    t.on_inventory(&inventory(100.0, &[("Forma", 1.0)]), "d");
    let e = t.on_inventory(&json!({"PremiumCredits": 100}), "d");
    assert_eq!(day(&e, "forma"), Some((-2.0, Some(1.0))));
}

#[test]
fn resumes_per_resource_deltas_after_a_restart() {
    let conn = mem_conn();
    let mut t = Tracker::default();
    t.on_inventory(&inventory(100.0, &[("Kuva", 10_000.0)]), "d");
    let e = t.on_inventory(&inventory(150.0, &[("Kuva", 15_000.0)]), "d");
    assert_eq!(day(&e, "kuva").unwrap().0, 5000.0);
    store::upsert(&conn, &e).unwrap();

    let mut t = restarted(&conn, "d");
    // Fresh baseline: this reading alone is worth nothing, the resumed total stands.
    t.on_inventory(&inventory(150.0, &[("Kuva", 15_000.0)]), "d");
    assert_eq!(t.session().resources["kuva"].delta, 5000.0);

    let e = t.on_inventory(&inventory(160.0, &[("Kuva", 17_000.0)]), "d");
    assert_eq!(day(&e, "kuva"), Some((7000.0, Some(17_000.0))));
    assert_eq!(day(&e, "plat").unwrap().0, 60.0);
}

#[test]
fn resumes_from_an_imported_legacy_entry() {
    let conn = mem_conn();
    let legacy: StatsImportEntry = serde_json::from_value(json!({
        "date": "2026-08-30", "platDelta": 40, "creditsDelta": 0, "endoDelta": 0, "ducatsDelta": 11,
        "ayaDelta": 0, "vitusDelta": 0, "relicsOpened": 0, "daysPlayed": 1, "dailyTrades": 0,
        "absPlat": 140, "absDucats": 311
    }))
    .unwrap();
    let entry = sanitize(vec![legacy]).unwrap().remove(0);
    store::upsert(&conn, &entry).unwrap();

    let mut t = restarted(&conn, "2026-08-30");
    t.on_inventory(
        &inventory(140.0, &[("PrimeBucks", 311.0), ("Kuva", 900.0)]),
        "2026-08-30",
    );
    let e = t.on_inventory(
        &inventory(145.0, &[("PrimeBucks", 311.0), ("Kuva", 1400.0)]),
        "2026-08-30",
    );
    assert_eq!(day(&e, "plat"), Some((45.0, Some(145.0))));
    assert_eq!(day(&e, "ducats").unwrap().0, 11.0);
    // Kuva had no flat field, so it counts from this session.
    assert_eq!(day(&e, "kuva"), Some((500.0, Some(1400.0))));
}

#[test]
fn rebaselines_every_resource_at_midnight_and_keeps_the_finished_day() {
    let conn = mem_conn();
    let mut t = Tracker::default();
    t.on_inventory(&inventory(100.0, &[("Kuva", 10_000.0)]), "2026-08-30");
    let finished = t.on_inventory(&inventory(180.0, &[("Kuva", 16_000.0)]), "2026-08-30");
    store::upsert(&conn, &finished).unwrap();

    let fresh = t.on_inventory(&inventory(180.0, &[("Kuva", 16_000.0)]), "2026-08-31");
    store::upsert(&conn, &fresh).unwrap();
    assert_eq!(day(&fresh, "kuva"), Some((0.0, Some(16_000.0))));
    assert_eq!(day(&fresh, "plat").unwrap().0, 0.0);

    let after = t.on_inventory(&inventory(175.0, &[("Kuva", 16_400.0)]), "2026-08-31");
    store::upsert(&conn, &after).unwrap();
    assert_eq!(day(&after, "kuva").unwrap().0, 400.0);
    assert_eq!(day(&after, "plat").unwrap().0, -5.0);

    let saved = store::load_all(&conn).unwrap();
    let old = saved.iter().find(|e| e.date == "2026-08-30").unwrap();
    assert_eq!(day(old, "kuva").unwrap().0, 6000.0);
    assert_eq!(day(old, "plat").unwrap().0, 80.0);
}

#[test]
fn counts_trades_for_the_running_day_and_resets_them_at_midnight() {
    let mut t = Tracker::default();
    t.add_trade("d1");
    assert_eq!(t.add_trade("d1").daily_trades, 2);
    assert_eq!(t.add_trade("d2").daily_trades, 1);
}

#[test]
fn store_keeps_the_newest_year_and_skips_today_on_import() {
    let mut conn = mem_conn();
    let rows: Vec<_> = (0..370)
        .map(|i| DailyStatEntry {
            date: (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() + chrono::Days::new(i))
                .to_string(),
            days_played: 1,
            ..Default::default()
        })
        .collect();
    let today = rows[369].date.clone();
    assert_eq!(store::import(&mut conn, &rows, &today).unwrap(), 369);
    let saved = store::load_all(&conn).unwrap();
    assert_eq!(saved.len() as i64, store::MAX_DAYS);
    assert_eq!(saved[0].date, rows[4].date);
    assert!(saved.iter().all(|e| e.date != today));
}

fn entry(v: Value) -> StatsImportEntry {
    serde_json::from_value(v).unwrap()
}

#[test]
fn import_validation_rejects_bad_dates_numbers_and_oversize_files() {
    let ok = json!({"date": "2026-02-28", "platDelta": 5});
    assert!(sanitize(vec![entry(ok.clone())]).is_some());
    for bad in [
        json!({"date": "2026-02-30"}),
        json!({"date": "26-02-28"}),
        json!({"date": "2026-02-28", "absPlat": -1}),
    ] {
        assert!(sanitize(vec![entry(ok.clone()), entry(bad)]).is_none());
    }
    let many = (0..10_001).map(|_| entry(ok.clone())).collect();
    assert!(sanitize(many).is_none());
}

#[test]
fn import_drops_unknown_resource_ids_and_prefers_the_map_over_flat_fields() {
    let rows = sanitize(vec![entry(json!({
        "date": "2026-02-28", "platDelta": 1, "resources": {
            "plat": {"delta": 99, "abs": 500}, "kuva": {"delta": 25000, "abs": 40000}, "bogus": {"delta": 1}
        }
    }))])
    .unwrap();
    let r = &rows[0].resources;
    assert_eq!(
        r["plat"],
        StatResourceDay {
            delta: 99.0,
            abs: Some(500.0)
        }
    );
    assert_eq!(r["kuva"].delta, 25_000.0);
    assert!(!r.contains_key("bogus"));
    assert_eq!(r["vitus"].abs, None);
}
