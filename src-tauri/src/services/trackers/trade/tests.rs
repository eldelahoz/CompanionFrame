use crate::db::mem_conn;
use std::sync::{Arc, Mutex};

use super::*;
use crate::services::eelog::Source;
use crate::services::eelog_parsers::trade::ParsedLogTradeItem;

fn trade(partner: &str, plat: u32, stamp: Option<&str>) -> ParsedLogTrade {
    ParsedLogTrade {
        partner: partner.into(),
        plat_change: plat,
        kind: TradeKind::Sale,
        items: vec![ParsedLogTradeItem {
            display_name: "Vitus Essence".into(),
            count: 1,
            direction: TradeDirection::Given,
        }],
        log_stamp: stamp.map(Into::into),
    }
}

#[test]
fn a_redelivery_inside_the_window_is_suppressed_but_a_later_identical_trade_is_recorded() {
    let mut r = Recent::default();
    let t = trade("T4092", 15, None);
    assert!(r.is_new(&t, 0.0));
    assert!(!r.is_new(&t, 14_000.0));
    assert!(r.is_new(&t, 14_000.0 + 31_000.0));
}

#[test]
fn an_identical_repeat_inside_the_window_is_kept_when_the_stamps_differ() {
    let mut r = Recent::default();
    assert!(r.is_new(&trade("T4092", 15, Some("1200.500")), 0.0));
    assert!(!r.is_new(&trade("T4092", 15, Some("1200.500")), 1.0));
    assert!(r.is_new(&trade("T4092", 15, Some("1212.750")), 2.0));
}

#[test]
fn one_unstamped_side_still_counts_as_a_redelivery() {
    let mut r = Recent::default();
    assert!(r.is_new(&trade("T4092", 15, None), 0.0));
    assert!(!r.is_new(&trade("T4092", 15, Some("1200.500")), 1.0));
}

#[test]
fn distinct_back_to_back_trades_are_both_new() {
    let mut r = Recent::default();
    assert!(r.is_new(&trade("BuyerA", 10, None), 0.0));
    assert!(r.is_new(&trade("BuyerB", 25, None), 1.0));
}

const HEAD: &str = "2416.657 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=\
    Are you sure you want to accept this trade? You are offering:";
const TAIL: &str = ", title= leftItem=/Menu/Confirm_Item_Ok, rightItem=/Menu/Confirm_Item_Cancel)";
const SUCCESS: &str = "2420.118 Script [Info]: Dialog.lua: Dialog::CreateOk(description=\
    The trade was successful!, title= leftItem=/Menu/Confirm_Item_Ok)";

fn dialog() -> Vec<String> {
    vec![
        HEAD.into(),
        "\rVicious Bond (RARE RANK 0)".into(),
        "".into(),
        "and will receive from TraderName the following:".into(),
        format!("\rPlatinum x 12{TAIL}"),
        SUCCESS.into(),
    ]
}

fn feed(t: &mut TradeTracker, lines: &[String], now_ms: f64) {
    for l in lines {
        t.on_line(&Line {
            text: l,
            source: Source::File,
            now_ms,
            staleness_ms: 0.0,
            realtime_active: false,
        });
    }
}

fn collecting() -> (TradeTracker, Arc<Mutex<Vec<ParsedLogTrade>>>) {
    let seen = Arc::new(Mutex::new(vec![]));
    let sink = seen.clone();
    let t = TradeTracker::new(Box::new(move |p| sink.lock().unwrap().push(p)));
    (t, seen)
}

#[test]
fn a_logged_sale_is_delivered_once_even_when_the_log_repeats_it() {
    let (mut t, seen) = collecting();
    feed(&mut t, &dialog(), 1_000.0);
    feed(&mut t, &dialog(), 5_000.0);
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].partner, "TraderName");
    assert_eq!(seen[0].plat_change, 12);
}

#[test]
fn a_log_restart_does_not_rerecord_the_trade_it_rereads() {
    let (mut t, seen) = collecting();
    feed(&mut t, &dialog(), 1_000.0);
    t.on_reset();
    feed(&mut t, &dialog(), 2_000.0);
    assert_eq!(seen.lock().unwrap().len(), 1);
}

fn catalog() -> HashMap<String, Known> {
    HashMap::from([
        (
            "vicious bond".to_string(),
            (
                Some("/Lotus/Types/Vicious".to_string()),
                "vicious_bond".to_string(),
                Some("https://x/thumb.png".to_string()),
            ),
        ),
        (
            "forma".to_string(),
            (None, "forma_blueprint".to_string(), None),
        ),
    ])
}

fn parsed_with(name: &str) -> ParsedLogTrade {
    let mut t = trade("Kestrel", 12, Some("1.5"));
    t.items[0].display_name = name.into();
    t
}

#[test]
fn the_event_carries_catalog_refs_found_through_rank_and_blueprint_suffixes() {
    let e = to_event(
        &parsed_with("Vicious Bond (RARE RANK 0)"),
        &catalog(),
        "id1".into(),
        "2026-01-01T00:00:00.000Z".into(),
    );
    let item = &e.items[0];
    assert_eq!(item.internal_name, "/Lotus/Types/Vicious");
    assert_eq!(item.wfm_slug.as_deref(), Some("vicious_bond"));
    assert_eq!(item.wfm_thumb.as_deref(), Some("https://x/thumb.png"));
    assert_eq!(
        (e.source, e.schema_version),
        (Some(EventSource::Live), Some(2))
    );

    let e = to_event(
        &parsed_with("Forma Blueprint"),
        &catalog(),
        "id2".into(),
        "d".into(),
    );
    assert_eq!(e.items[0].wfm_slug.as_deref(), Some("forma_blueprint"));
    assert_eq!(e.items[0].internal_name, "");
}

#[test]
fn an_item_the_catalog_does_not_know_is_still_recorded() {
    let e = to_event(
        &parsed_with("Mystery Thing"),
        &catalog(),
        "id".into(),
        "d".into(),
    );
    assert_eq!(e.items[0].display_name, "Mystery Thing");
    assert_eq!(
        (
            e.items[0].wfm_slug.as_deref(),
            e.items[0].internal_name.as_str()
        ),
        (None, "")
    );
}

#[test]
fn the_live_event_lands_in_the_ledger_and_a_second_insert_is_skipped() {
    let mut conn = mem_conn();
    let e = to_event(
        &parsed_with("Vicious Bond"),
        &catalog(),
        "2026-01-01T00:00:00.000Z-ab12".into(),
        "2026-01-01T00:00:00.000Z".into(),
    );
    assert_eq!(
        ledger::add_events(&mut conn, std::slice::from_ref(&e))
            .unwrap()
            .applied,
        1
    );
    assert_eq!(
        ledger::add_events(&mut conn, std::slice::from_ref(&e))
            .unwrap()
            .skipped_duplicates,
        1
    );
    let page = ledger::query(&conn, &ledger::LedgerQuery::default()).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.events[0], e);
}

#[test]
fn the_notification_names_the_traded_item_quantity_price_and_partner() {
    let mut t = parsed_with("Serration");
    t.items[0].count = 2;
    let e = to_event(&t, &HashMap::new(), "i".into(), "d".into());
    assert_eq!(notification_body(&e), "2x Serration 12p with Kestrel");

    // A purchase names what came in, and extras are counted.
    let mut e = e;
    e.kind = TradeType::Purchase;
    e.items = vec![
        TradeItem {
            internal_name: String::new(),
            display_name: "Rhino Chassis".into(),
            count: 1,
            direction: Direction::Received,
            wfm_slug: None,
            wfm_thumb: None,
        },
        TradeItem {
            internal_name: String::new(),
            display_name: "Rhino Neuroptics".into(),
            count: 1,
            direction: Direction::Received,
            wfm_slug: None,
            wfm_thumb: None,
        },
    ];
    assert_eq!(notification_body(&e), "Rhino Chassis +1 12p with Kestrel");
}

#[test]
fn the_workbench_trade_mirrors_the_event() {
    let e = to_event(
        &parsed_with("Vicious Bond"),
        &catalog(),
        "i".into(),
        "d".into(),
    );
    let w = workbench_trade(&e);
    assert_eq!(
        (w.partner.as_str(), w.plat_change, w.kind),
        ("Kestrel", 12, TradeType::Sale)
    );
    assert_eq!(
        (w.items[0].display_name.as_str(), w.items[0].count),
        ("Vicious Bond", 1)
    );
}

#[test]
fn dialog_names_are_translated_to_english_and_unknown_ones_kept() {
    let english = HashMap::from([(
        "nova prime - systèmes (schéma)".to_string(),
        "Nova Prime Systems Blueprint".to_string(),
    )]);
    let mut t = parsed_with("Nova Prime - Systèmes (Schéma)");
    to_english(&mut t, &english);
    assert_eq!(t.items[0].display_name, "Nova Prime Systems Blueprint");
    let mut t = parsed_with("Forma inconnue");
    to_english(&mut t, &english);
    assert_eq!(t.items[0].display_name, "Forma inconnue");
}

#[test]
fn interleaved_file_and_dbwin_halves_record_one_trade() {
    let start = "Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Souhaitez-vous vraiment accepter cet échange? Vous proposez: ";
    let whole = format!(
        "{start}\n\rAfuris Prime - Culasse\r\n\r\net vous recevrez de TraderName\u{E000} ceci: \n\rPlatinum x 35, title= leftItem=/Menu/Confirm_Item_Ok, rightItem=/Menu/Confirm_Item_Cancel)"
    );
    let ok = "Script [Info]: Dialog.lua: Dialog::CreateOk(description=L'échange a réussi!, title= leftItem=/Menu/Confirm_Item_Ok)";
    let seq = [
        (Source::Dbwin, whole),
        (Source::File, format!("1082.718 {start}")),
        (Source::File, "\rAfuris Prime - Culasse\r".into()),
        (Source::File, "\r".into()),
        (Source::File, "et vous recevrez de TraderName\u{E000} ceci: ".into()),
        (Source::Dbwin, "Script [Info]: Dialog.lua: Dialog::SendResult(4)".into()),
        (Source::Dbwin, ok.into()),
        (Source::File, "\rPlatinum x 35, title= leftItem=/Menu/Confirm_Item_Ok, rightItem=/Menu/Confirm_Item_Cancel)".into()),
        (Source::File, "1086.036 Script [Info]: Dialog.lua: Dialog::SendResult(4)".into()),
        (Source::File, format!("1086.957 {ok}")),
    ];
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let mut t = TradeTracker::new(Box::new(move |p| sink.lock().unwrap().push(p)));
    for (n, (source, text)) in seq.iter().enumerate() {
        t.on_line(&Line {
            text,
            source: *source,
            now_ms: n as f64,
            staleness_ms: 0.0,
            realtime_active: false,
        });
    }
    let got = seen.lock().unwrap();
    assert_eq!(got.len(), 1);
    let g = &got[0];
    assert_eq!(
        (g.kind, g.plat_change, g.partner.as_str()),
        (TradeKind::Sale, 35, "TraderName")
    );
    assert_eq!(g.items.len(), 1);
    assert_eq!(g.items[0].display_name, "Afuris Prime - Culasse");
}
