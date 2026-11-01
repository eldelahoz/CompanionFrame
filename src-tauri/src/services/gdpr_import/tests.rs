use std::sync::Mutex;

use chrono::{Datelike, Local, TimeZone};
use serde_json::{Value, json};

use super::*;

const LAST_YEAR: i32 = 2024;

struct Catalog;
impl ItemCatalog for Catalog {
    fn lookup(&self, name: &str) -> Option<CatalogItem> {
        (name == "Ash Prime Chassis").then(|| CatalogItem {
            game_ref: None,
            url_name: Some("ash_prime_chassis".into()),
            thumb: Some("ash.png".into()),
        })
    }
}

fn parse(text: &str, file: &str, existing: &[TradeEvent]) -> ParseResult {
    parse_gdpr_trade_export(text, file, existing, &Catalog)
}

fn csv() -> String {
    [
        "Date,Type,Item,Platinum,Partner".to_string(),
        format!("{LAST_YEAR}-03-04,Sale,Ash Prime Chassis,45,Kestrel"),
        format!("{LAST_YEAR}-03-04,Sale,Ash Prime Chassis,45,Kestrel"),
        format!("{LAST_YEAR}-03-04,Purchase,Nikana Prime Blade,30,Vor"),
    ]
    .join("\n")
}

fn local(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> String {
    let t = Local.with_ymd_and_hms(y, m, d, h, min, s).unwrap();
    iso(t.with_timezone(&Utc))
}

/// GDPR wall-clock dates land on the local calendar day, not the UTC one.
fn local_day(iso: &str) -> (i32, u32, u32) {
    let t = DateTime::parse_from_rfc3339(iso)
        .unwrap()
        .with_timezone(&Local);
    (t.year(), t.month(), t.day())
}

#[derive(Default)]
struct MemLedger(Mutex<Vec<TradeEvent>>);
impl LedgerSink for MemLedger {
    fn add_events(&self, events: Vec<TradeEvent>) -> Result<ApplyOutcome, LedgerErrorCode> {
        let mut all = self.0.lock().unwrap();
        let (mut applied, mut skipped) = (0, 0);
        for e in events {
            if all.iter().any(|x| x.id == e.id) {
                skipped += 1;
            } else {
                all.push(e);
                applied += 1;
            }
        }
        Ok(ApplyOutcome {
            applied,
            skipped_duplicates: skipped,
        })
    }
}

struct TempFile(std::path::PathBuf);
impl TempFile {
    fn new(name: &str, body: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gdpr-test-{}-{name}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name);
        std::fs::write(&file, body).unwrap();
        Self(file)
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}

#[test]
fn parse_date_reads_the_shapes_an_export_plausibly_ships() {
    let p = |v: Value| parse_gdpr_date(&v);
    // A wall-clock date carries no offset, so it is read as the local day.
    assert_eq!(p(json!("2025-03-04")), Some(local(2025, 3, 4, 0, 0, 0)));
    assert_eq!(
        p(json!("2025-03-04 18:30:00")),
        Some(local(2025, 3, 4, 18, 30, 0))
    );
    assert_eq!(
        p(json!("2025-03-04T18:30:00Z")).as_deref(),
        Some("2025-03-04T18:30:00.000Z")
    );
    // Dotted is day-first; slashed defaults to US order unless day > 12.
    assert_eq!(p(json!("04.03.2025")), Some(local(2025, 3, 4, 0, 0, 0)));
    assert_eq!(p(json!("03/04/2025")), Some(local(2025, 3, 4, 0, 0, 0)));
    assert_eq!(p(json!("25/03/2025")), Some(local(2025, 3, 25, 0, 0, 0)));
    let epoch = Some("2023-11-14T22:13:20.000Z".to_string());
    assert_eq!(p(json!(1_700_000_000)), epoch);
    assert_eq!(p(json!("1700000000")), epoch);
    assert_eq!(p(json!("who knows")), None);
    assert_eq!(p(json!("")), None);
    assert_eq!(p(Value::Null), None);
}

#[test]
fn maps_headers_by_meaning_and_resolves_catalog_names() {
    let result = parse(&csv(), "trades.csv", &[]);
    assert_eq!(
        result.counts,
        Counts {
            parsed: 2,
            duplicates: 0,
            unresolved: 1,
            rejected: 0
        }
    );
    let first = &result.events[0];
    assert_eq!(local_day(&first.date), (LAST_YEAR, 3, 4));
    assert_eq!(first.kind, TradeType::Sale);
    assert_eq!(first.plat_change, 45);
    assert_eq!(first.partner.as_deref(), Some("Kestrel"));
    assert_eq!(first.source, Some(EventSource::Gdpr));
    assert_eq!(first.schema_version, Some(2));
    assert_eq!(
        first.items[0].wfm_slug.as_deref(),
        Some("ash_prime_chassis")
    );

    let unresolved = result
        .rows
        .iter()
        .find(|r| r.kind == RowKind::Unresolved)
        .unwrap();
    assert!(unresolved.summary.contains("Nikana Prime Blade"));
}

#[test]
fn keeps_identical_same_day_rows_distinct() {
    let result = parse(&csv(), "trades.csv", &[]);
    let ids: HashSet<_> = result.events.iter().map(|e| &e.id).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(result.counts.duplicates, 0);
}

#[test]
fn marks_rows_already_in_the_ledger_as_duplicates() {
    let first = parse(&csv(), "trades.csv", &[]);
    let second = parse(&csv(), "trades.csv", &first.events);
    assert!(second.events.is_empty());
    assert_eq!(second.counts.duplicates, 3);
}

#[test]
fn rejects_unreadable_rows_with_a_reason() {
    let body = [
        "Date,Type,Item,Platinum,Partner".to_string(),
        format!("{LAST_YEAR}-03-04,Sale,Ash Prime Chassis,45,Kestrel"),
        "who-knows,Sale,Ash Prime Chassis,45,Kestrel".to_string(),
        format!("{LAST_YEAR}-03-05,Sale,,,Ghost"),
    ]
    .join("\n");
    let result = parse(&body, "trades.csv", &[]);
    assert_eq!(result.counts.parsed, 1);
    assert_eq!(result.counts.rejected, 2);
    let reasons: Vec<_> = result
        .rows
        .iter()
        .filter(|r| r.kind == RowKind::Rejected)
        .filter_map(|r| r.reason.as_deref())
        .collect();
    assert!(reasons.contains(&"no readable date"));
    assert!(reasons.contains(&"no items and no platinum"));
}

#[test]
fn reads_json_with_nested_array_and_per_direction_item_lists() {
    let body = json!({
        "account": { "name": "Tenno" },
        "tradeHistory": [{
            "timestamp": format!("{LAST_YEAR}-04-05T12:30:00Z"),
            "direction": "Bought",
            "itemsReceived": [{ "name": "Ash Prime Chassis", "quantity": 2 }],
            "itemsGiven": "Forma",
            "platinum": 60,
            "otherPlayer": "Vor",
            "tradeTax": 8000
        }]
    })
    .to_string();
    let result = parse(&body, "export.json", &[]);
    assert_eq!(result.events.len(), 1);
    let e = &result.events[0];
    assert_eq!(e.kind, TradeType::Purchase);
    assert_eq!(e.plat_change, 60);
    assert_eq!(e.trade_tax, Some(8000));
    assert_eq!(e.partner.as_deref(), Some("Vor"));
    assert_eq!(
        e.items,
        vec![
            TradeItem {
                internal_name: "".into(),
                display_name: "Forma".into(),
                count: 1,
                direction: Direction::Given,
                wfm_slug: None,
                wfm_thumb: None,
            },
            TradeItem {
                internal_name: "".into(),
                display_name: "Ash Prime Chassis".into(),
                count: 2,
                direction: Direction::Received,
                wfm_slug: Some("ash_prime_chassis".into()),
                wfm_thumb: Some("ash.png".into()),
            },
        ]
    );
}

#[test]
fn infers_direction_from_signed_platinum_without_a_type_column() {
    let body = [
        "Date,Item,Platinum,Partner".to_string(),
        format!("{LAST_YEAR}-03-04,Ash Prime Chassis,45,Kestrel"),
        format!("{LAST_YEAR}-03-05,Ash Prime Chassis,-30,Vor"),
    ]
    .join("\n");
    let result = parse(&body, "trades.csv", &[]);
    let kinds: Vec<_> = result.events.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [TradeType::Sale, TradeType::Purchase]);
    let plat: Vec<_> = result.events.iter().map(|e| e.plat_change).collect();
    assert_eq!(plat, [45, 30]);
}

fn live(day: &str, partner: &str, plat: i64) -> TradeEvent {
    TradeEvent {
        id: format!("live-{day}-{plat}"),
        date: format!("{day}T19:14:03.000Z"),
        kind: TradeType::Sale,
        plat_change: plat,
        items: vec![TradeItem {
            internal_name: "".into(),
            display_name: "Ash Prime Chassis".into(),
            count: 1,
            direction: Direction::Given,
            wfm_slug: None,
            wfm_thumb: None,
        }],
        partner: Some(partner.into()),
        schema_version: None,
        source: Some(EventSource::Live),
        source_record_id: None,
        import_batch_id: None,
        wfm_closed: None,
        credits: None,
        trade_tax: None,
    }
}

fn day(d: u32) -> String {
    format!("{LAST_YEAR}-03-{d:02}")
}

#[test]
fn live_tracked_trade_is_a_duplicate() {
    let body = format!(
        "Date,Type,Item,Platinum,Partner\n{},Sale,Ash Prime Chassis,45,Kestrel",
        day(4)
    );
    let result = parse(&body, "trades.csv", &[live(&day(4), "Kestrel", 45)]);
    assert_eq!(result.counts.duplicates, 1);
    assert_eq!(result.counts.parsed, 0);
    assert!(result.events.is_empty());
    assert_eq!(result.rows[0].kind, RowKind::Duplicate);
}

#[test]
fn live_match_works_without_partner_column_and_ignores_case() {
    let body = format!(
        "Date,Type,Item,Platinum\n{},Sale,ash prime chassis,45",
        day(4)
    );
    let result = parse(&body, "trades.csv", &[live(&day(4), "KESTREL", 45)]);
    assert_eq!(result.counts.duplicates, 1);
}

#[test]
fn one_live_row_absorbs_only_one_imported_row() {
    let row = format!("{},Sale,Ash Prime Chassis,45,Kestrel", day(4));
    let body = format!("Date,Type,Item,Platinum,Partner\n{row}\n{row}");
    let result = parse(&body, "trades.csv", &[live(&day(4), "Kestrel", 45)]);
    assert_eq!((result.counts.parsed, result.counts.duplicates), (1, 1));
    assert_eq!(result.events.len(), 1);
}

#[test]
fn distinct_same_day_trades_stay_apart() {
    let body = [
        "Date,Type,Item,Platinum,Partner".to_string(),
        format!("{},Sale,Ash Prime Chassis,60,Kestrel", day(4)),
        format!("{},Purchase,Ash Prime Chassis,45,Kestrel", day(4)),
        format!("{},Sale,Ash Prime Chassis,45,Vor", day(4)),
    ]
    .join("\n");
    let result = parse(&body, "trades.csv", &[live(&day(4), "Kestrel", 45)]);
    assert_eq!(result.counts.duplicates, 0);
    assert_eq!(result.events.len(), 3);
}

#[test]
fn imported_rows_never_fold_into_each_other_by_heuristic() {
    let row = format!("{},Sale,Ash Prime Chassis,45,Kestrel", day(4));
    let body = format!("Date,Type,Item,Platinum,Partner\n{row}\n{row}");
    let gdpr = parse(&body, "trades.csv", &[]);
    assert_eq!(gdpr.events.len(), 2);
    // The same rows as prior gdpr-sourced entries still only dedup by id.
    let again = parse(&body, "trades.csv", &gdpr.events);
    assert_eq!(again.counts.duplicates, 2);
}

#[test]
fn live_row_more_than_a_day_away_does_not_match() {
    let body = format!(
        "Date,Type,Item,Platinum,Partner\n{},Sale,Ash Prime Chassis,45,Kestrel",
        day(4)
    );
    let result = parse(&body, "trades.csv", &[live(&day(8), "Kestrel", 45)]);
    assert_eq!(result.counts.duplicates, 0);
}

#[test]
fn second_import_of_the_same_file_is_a_no_op() {
    let file = TempFile::new("trades.csv", &csv());
    let staging = GdprStaging::default();
    let ledger = MemLedger::default();

    let preview = staging.preview_file(&file.0, &[], &Catalog).unwrap();
    assert_eq!(preview.counts.duplicates, 0);
    let applied = staging.apply(&preview.batch_id, &ledger).unwrap();
    assert_eq!(
        applied,
        ApplyOutcome {
            applied: 3,
            skipped_duplicates: 0
        }
    );

    let existing = ledger.0.lock().unwrap().clone();
    let second = staging.preview_file(&file.0, &existing, &Catalog).unwrap();
    assert_eq!(
        second.counts,
        Counts {
            parsed: 0,
            duplicates: 3,
            unresolved: 0,
            rejected: 0
        }
    );
    let again = staging.apply(&second.batch_id, &ledger).unwrap();
    assert_eq!(
        again,
        ApplyOutcome {
            applied: 0,
            skipped_duplicates: 0
        }
    );
    assert_eq!(ledger.0.lock().unwrap().len(), 3);
}

#[test]
fn applying_the_same_rows_twice_still_dedups() {
    let file = TempFile::new("trades.csv", &csv());
    let staging = GdprStaging::default();
    let ledger = MemLedger::default();
    let preview = staging.preview_file(&file.0, &[], &Catalog).unwrap();
    let staged = staging.take(&preview.batch_id).unwrap();
    assert_eq!(ledger.add_events(staged.clone()).unwrap().applied, 3);
    assert_eq!(
        ledger.add_events(staged).unwrap(),
        ApplyOutcome {
            applied: 0,
            skipped_duplicates: 3
        }
    );
}

#[test]
fn stamps_batch_id_and_serves_a_batch_once() {
    let file = TempFile::new("trades.csv", &csv());
    let staging = GdprStaging::default();
    let preview = staging.preview_file(&file.0, &[], &Catalog).unwrap();
    let staged = staging.take(&preview.batch_id).unwrap();
    assert!(
        staged
            .iter()
            .all(|e| e.import_batch_id.as_deref() == Some(&preview.batch_id))
    );
    assert!(staging.take(&preview.batch_id).is_none());
    assert_eq!(
        staging.apply(&preview.batch_id, &MemLedger::default()),
        Err(LedgerErrorCode::BatchGone)
    );
}

#[test]
fn reports_an_error_code_for_a_file_with_no_rows() {
    let file = TempFile::new("empty.csv", "nothing to see here");
    let err = GdprStaging::default()
        .preview_file(&file.0, &[], &Catalog)
        .unwrap_err();
    assert_eq!(err, LedgerErrorCode::NoRows);
}

#[test]
fn stages_rows_whose_item_names_are_outside_the_catalog() {
    let body = format!(
        "Date,Type,Item,Platinum,Partner\n{},Sale,Nikana Prime Blade,30,Vor",
        day(4)
    );
    let parsed = parse(&body, "trades.csv", &[]);
    assert_eq!((parsed.counts.parsed, parsed.counts.unresolved), (0, 1));
    assert_eq!(
        parsed.events.len() as u32,
        parsed.counts.parsed + parsed.counts.unresolved
    );

    let file = TempFile::new("unresolved.csv", &body);
    let staging = GdprStaging::default();
    let preview = staging.preview_file(&file.0, &[], &Catalog).unwrap();
    assert_eq!(preview.counts.parsed, 0);
    let out = staging
        .apply(&preview.batch_id, &MemLedger::default())
        .unwrap();
    assert_eq!(
        out,
        ApplyOutcome {
            applied: 1,
            skipped_duplicates: 0
        }
    );
}

// ---- gdprImportNumbers ----

fn num_csv(rows: &[&str]) -> String {
    format!(
        "Date;Type;Item;Platinum;Credits;Tax;Partner\n{}",
        rows.join("\n")
    )
}

fn plat(r: &ParseResult) -> Vec<i64> {
    r.events.iter().map(|e| e.plat_change).collect()
}

#[test]
fn reads_dot_grouped_thousands_instead_of_a_fraction() {
    let r = parse(
        &num_csv(&["04.03.2025;Sale;Ash Prime Chassis;1.234;2.500.000;8.000;Kestrel"]),
        "trades.csv",
        &[],
    );
    assert_eq!(r.counts.rejected, 0);
    let e = &r.events[0];
    assert_eq!(e.plat_change, 1234);
    assert_eq!(e.credits, Some(2_500_000));
    assert_eq!(e.trade_tax, Some(8000));
}

#[test]
fn takes_the_rightmost_separator_as_the_decimal_point() {
    let r = parse(
        &num_csv(&[
            "05.03.2025;Sale;Ash Prime Chassis;1.234,00;9.999,20;0;Vor",
            "06.03.2025;Sale;Ash Prime Chassis;1,234.00;9,999.20;0;Vor",
            "07.03.2025;Sale;Ash Prime Chassis;1.234,100;9.999,200;0;Vor",
        ]),
        "trades.csv",
        &[],
    );
    assert_eq!(plat(&r), [1234, 1234, 1234]);
    let credits: Vec<_> = r.events.iter().map(|e| e.credits).collect();
    assert_eq!(credits, [Some(9999); 3]);
}

#[test]
fn repeated_separator_is_thousands_even_when_a_group_is_short() {
    let r = parse(
        &num_csv(&[
            "08.03.2025;Sale;Ash Prime Chassis;1,234,56;7.654,32;0;Vor",
            "09.03.2025;Sale;Ash Prime Chassis;12.345.6;0;0;Vor",
        ]),
        "trades.csv",
        &[],
    );
    assert_eq!(r.counts.rejected, 0);
    assert_eq!(plat(&r), [123456, 123456]);
    assert_eq!(r.events[0].credits, Some(7654));
}

#[test]
fn still_reads_comma_grouped_and_plain_numbers() {
    let r = parse(
        &num_csv(&[
            "2025-03-07;Sale;Ash Prime Chassis;1,234;2,500,000;8,000;Vor",
            "2025-03-08;Sale;Ash Prime Chassis;45;;;Vor",
        ]),
        "trades.csv",
        &[],
    );
    assert_eq!(plat(&r), [1234, 45]);
    assert_eq!(r.events[0].credits, Some(2_500_000));
    assert_eq!(r.events[0].trade_tax, Some(8000));
    assert_eq!(r.events[1].credits, None);
}
