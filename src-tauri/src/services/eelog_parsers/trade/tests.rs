use super::*;

fn s(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

// Warframe logs each trade item on its own line; stacked items as "Name x N".
fn sale_buffer(offer_lines: &[&str]) -> Vec<String> {
    let mut v = vec![
        "Are you sure you want to accept this trade?",
        "You are offering:",
    ];
    v.extend(offer_lines);
    v.push("and will receive from BuyerName the following:");
    v.push("Platinum x 45, leftItem=/Menu/Confirm_Item_Ok");
    s(&v)
}

fn count_of(t: &ParsedLogTrade, name: &str) -> Option<u32> {
    t.items
        .iter()
        .find(|i| i.display_name == name)
        .map(|i| i.count)
}

fn names(t: &ParsedLogTrade) -> Vec<&str> {
    t.items.iter().map(|i| i.display_name.as_str()).collect()
}

fn item(name: &str, count: u32, direction: TradeDirection) -> ParsedLogTradeItem {
    ParsedLogTradeItem {
        display_name: name.into(),
        count,
        direction,
    }
}

#[test]
fn parses_a_stacked_item_as_count_n() {
    let t = parse_trade_dialog(&sale_buffer(&["Arcane Pistoleer x 3"])).unwrap();
    assert_eq!(t.kind, TradeKind::Sale);
    assert_eq!(t.partner, "BuyerName");
    assert_eq!(t.plat_change, 45);
    assert_eq!(count_of(&t, "Arcane Pistoleer"), Some(3));
    assert_eq!(t.items[0].direction, TradeDirection::Given);
}

#[test]
fn counts_non_stacking_duplicate_lines() {
    let t = parse_trade_dialog(&sale_buffer(&["Arcane Pistoleer"; 3])).unwrap();
    assert_eq!(count_of(&t, "Arcane Pistoleer"), Some(3));
}

#[test]
fn handles_mixed_stacked_and_duplicate_lines_for_the_same_item() {
    let t = parse_trade_dialog(&sale_buffer(&["Vitus Essence x 2", "Vitus Essence"])).unwrap();
    assert_eq!(count_of(&t, "Vitus Essence"), Some(3));
}

#[test]
fn treats_received_platinum_as_plat_not_an_item() {
    let t = parse_trade_dialog(&sale_buffer(&["Arcane Pistoleer x 3"])).unwrap();
    assert!(
        !t.items
            .iter()
            .any(|i| i.display_name.to_lowercase().contains("platinum"))
    );
}

#[test]
fn classifies_purchases_and_swaps() {
    let purchase = parse_trade_dialog(&s(&[
        "Are you sure you want to accept this trade?",
        "You are offering:",
        "Platinum x 30",
        "and will receive from Seller the following:",
        "Axi A12 Relic, leftItem=/x",
    ]))
    .unwrap();
    assert_eq!(purchase.kind, TradeKind::Purchase);
    assert_eq!(purchase.plat_change, 30);
    assert_eq!(
        purchase.items,
        [item("Axi A12 Relic", 1, TradeDirection::Received)]
    );

    let swap = parse_trade_dialog(&s(&[
        "You are offering:",
        "A",
        "and will receive from Seller the following:",
        "B, leftItem=/x",
    ]))
    .unwrap();
    assert_eq!(swap.kind, TradeKind::Trade);
    assert_eq!(swap.plat_change, 0);
}

#[test]
fn returns_none_without_the_offering_or_receive_markers() {
    assert!(parse_trade_dialog(&s(&["nothing here"])).is_none());
    assert!(parse_trade_dialog(&s(&["You are offering:", "A"])).is_none());
}

fn partner_of(offer: &str, partner_line: &str) -> String {
    parse_trade_dialog(&s(&[
        "Are you sure you want to accept this trade?",
        "You are offering:",
        offer,
        partner_line,
        "Platinum x 45, leftItem=/Menu/Confirm_Item_Ok",
    ]))
    .unwrap()
    .partner
}

#[test]
fn strips_a_trailing_platform_glyph_from_the_partner_name() {
    let p = partner_of(
        "Arcane Pistoleer",
        "and will receive from Kestrel\u{E000} the following:",
    );
    assert_eq!(p, "Kestrel");
}

#[test]
fn strips_the_dbwin_latin1_mojibake_form_of_a_platform_glyph_from_the_partner() {
    let p = partner_of(
        "Arcane Pistoleer",
        "and will receive from Kestrel\u{EE}\u{80}\u{80} the following:",
    );
    assert_eq!(p, "Kestrel");
}

#[test]
fn drops_glyph_only_lines_instead_of_recording_them_as_items() {
    let t = parse_trade_dialog(&sale_buffer(&[
        "Zid-an Asheir",
        "\u{E000}\u{E001}\u{E002}\u{E003}",
        "\u{E000}\u{E000}",
    ]))
    .unwrap();
    assert_eq!(names(&t), ["Zid-an Asheir"]);
}

#[test]
fn strips_platform_glyphs_embedded_in_an_item_name() {
    let t = parse_trade_dialog(&sale_buffer(&["Zid-an Asheir\u{E000}\u{E001}"])).unwrap();
    assert_eq!(names(&t), ["Zid-an Asheir"]);
}

#[test]
fn does_not_capture_dialog_key_value_tails_into_the_item_name() {
    let t = parse_trade_dialog(&sale_buffer(&[
        "Zid-an Asheir, title= leftItem=/Menu/Confirm_Item_Ok",
    ]))
    .unwrap();
    assert_eq!(names(&t), ["Zid-an Asheir"]);
}

#[test]
fn rejects_raw_framework_lines_leaked_into_the_buffer() {
    let t = parse_trade_dialog(&sale_buffer(&[
        "Zid-an Asheir",
        "11828.904 Script [Info]: Dialog.lua: Dialog::",
        "11829.001 Sys [Info]: whatever",
    ]))
    .unwrap();
    assert_eq!(names(&t), ["Zid-an Asheir"]);
}

#[test]
fn stops_item_parsing_at_a_bare_title_arg_line() {
    let t = parse_trade_dialog(&sale_buffer(&[
        "Zid-an Asheir",
        "title=",
        "leftItem=/Menu/Confirm_Item_Ok",
    ]))
    .unwrap();
    assert_eq!(names(&t), ["Zid-an Asheir"]);
}

// The dialog description separates items with a bare CR, which the log reader
// does not treat as a line break, so the engine can flush the next entry behind one.
const GLYPH: char = '\u{E000}';
const DIALOG_HEAD: &str = "2416.657 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Are you sure you want to accept this trade? You are offering:";
const DIALOG_TAIL_ARGS: &str =
    ", title= leftItem=/Menu/Confirm_Item_Ok, rightItem=/Menu/Confirm_Item_Cancel)";
const NEXT_ENTRY: &str = "2416.702 Sys [Info]: Fade out complete";

fn from_raw_log(raw: &str) -> Vec<String> {
    raw.split(['\n'])
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

#[test]
fn reads_the_reported_vicious_bond_sale_as_12p() {
    let raw = [
        DIALOG_HEAD.to_string(),
        "\rVicious Bond (RARE RANK 0)".into(),
        String::new(),
        format!("and will receive from RendiW{GLYPH} the following:"),
        format!("\rPlatinum x 12{DIALOG_TAIL_ARGS}\r{NEXT_ENTRY}"),
    ]
    .join("\n");
    let t = parse_trade_dialog(&from_raw_log(&raw)).unwrap();
    assert_eq!(t.kind, TradeKind::Sale);
    assert_eq!(t.plat_change, 12);
    assert_eq!(t.partner, "RendiW");
    assert_eq!(
        t.items,
        [item("Vicious Bond (RARE RANK 0)", 1, TradeDirection::Given)]
    );
    assert_eq!(t.log_stamp.as_deref(), Some("2416.657"));
}

#[test]
fn reads_the_same_dialog_when_cr_is_its_only_separator() {
    let one = [
        DIALOG_HEAD.to_string(),
        "Vicious Bond (RARE RANK 0)".into(),
        format!("and will receive from RendiW{GLYPH} the following:"),
        format!("Platinum x 12{DIALOG_TAIL_ARGS}"),
        NEXT_ENTRY.into(),
    ]
    .join("\r");
    let t = parse_trade_dialog(&[one]).unwrap();
    assert_eq!(t.kind, TradeKind::Sale);
    assert_eq!(t.plat_change, 12);
    assert_eq!(t.partner, "RendiW");
    assert_eq!(names(&t), ["Vicious Bond (RARE RANK 0)"]);
}

#[test]
fn keeps_cr_separated_items_apart_instead_of_merging_their_names() {
    let t = parse_trade_dialog(&s(&[
        "Are you sure you want to accept this trade?",
        "You are offering:",
        "\rAxi A12 Relic\rAxi A12 Relic\rVitus Essence x 2",
        "and will receive from BuyerName the following:",
        &format!("\rPlatinum x 45{DIALOG_TAIL_ARGS}"),
    ]))
    .unwrap();
    assert_eq!(t.plat_change, 45);
    assert_eq!(
        t.items,
        [
            item("Axi A12 Relic", 2, TradeDirection::Given),
            item("Vitus Essence", 2, TradeDirection::Given)
        ]
    );
}

#[test]
fn reads_the_platinum_when_the_next_entry_is_glued_on_with_no_separator() {
    let mut buf = sale_buffer(&["Arcane Pistoleer"]);
    buf.pop();
    buf.push(format!("Platinum x 45{DIALOG_TAIL_ARGS}{NEXT_ENTRY}"));
    let t = parse_trade_dialog(&buf).unwrap();
    assert_eq!(t.plat_change, 45);
    assert_eq!(t.kind, TradeKind::Sale);
}

#[test]
fn still_rejects_a_framework_line_that_arrives_on_its_own_cr_piece() {
    let t = parse_trade_dialog(&sale_buffer(&[
        "Zid-an Asheir\r11828.904 Script [Info]: Dialog.lua: Dialog::",
    ]))
    .unwrap();
    assert_eq!(names(&t), ["Zid-an Asheir"]);
    assert_eq!(t.plat_change, 45);
}

// -- live dispatch through the line buffer ---------------------------------

const PREVIOUS_DIALOG: &str = "2410.104 Script [Info]: Dialog.lua: Dialog::CreateOk(description=Trade request accepted, title= leftItem=/Menu/Confirm_Item_Ok)";
const SUCCESS_LINE: &str = "2420.118 Script [Info]: Dialog.lua: Dialog::CreateOk(description=The trade was successful!, title= leftItem=/Menu/Confirm_Item_Ok)";

// The reported sale as the log reader hands it over: the game separates
// description lines with a bare CR and flushed the next entry behind the
// platinum line, so that entry shares the platinum line's file line.
fn reported_sale_lines() -> Vec<String> {
    vec![
        DIALOG_HEAD.to_string(),
        "\rVicious Bond (RARE RANK 0)".into(),
        String::new(),
        format!("and will receive from RendiW{GLYPH} the following:"),
        format!("\rPlatinum x 12{DIALOG_TAIL_ARGS}\r{NEXT_ENTRY}"),
    ]
}

fn assert_reported_sale(t: &ParsedLogTrade) {
    assert_eq!(t.kind, TradeKind::Sale);
    assert_eq!(t.plat_change, 12);
    assert_eq!(t.partner, "RendiW");
    assert_eq!(
        t.items,
        [item("Vicious Bond (RARE RANK 0)", 1, TradeDirection::Given)]
    );
}

/// Feeds every line 1ms apart and collects the trades.
fn emit_all<S: AsRef<str>>(lines: &[S]) -> Vec<ParsedLogTrade> {
    let mut p = TradeDialogParser::new();
    lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| p.feed_line(l.as_ref(), i as f64))
        .collect()
}

#[test]
fn reads_the_platinum_line_even_when_the_next_log_entry_is_glued_behind_it() {
    let mut lines = reported_sale_lines();
    lines.push(SUCCESS_LINE.into());
    let trades = emit_all(&lines);
    assert_eq!(trades.len(), 1);
    assert_reported_sale(&trades[0]);
}

#[test]
fn still_seals_the_dialog_at_a_log_entry_on_its_own_line() {
    let trades = emit_all(&[
        DIALOG_HEAD.to_string(),
        "\rVicious Bond (RARE RANK 0)".into(),
        "and will receive from RendiW the following:".into(),
        format!("\rPlatinum x 12{DIALOG_TAIL_ARGS}"),
        NEXT_ENTRY.into(),
        "Arcane Energize (LEGENDARY RANK 5)".into(),
        SUCCESS_LINE.into(),
    ]);
    assert_eq!(trades.len(), 1);
    assert_eq!(
        (trades[0].kind, trades[0].plat_change),
        (TradeKind::Sale, 12)
    );
    assert_eq!(names(&trades[0]), ["Vicious Bond (RARE RANK 0)"]);
}

#[test]
fn keeps_an_item_line_that_the_next_entry_was_glued_behind() {
    let trades = emit_all(&[
        DIALOG_HEAD.to_string(),
        format!("\rVicious Bond (RARE RANK 0)\r{NEXT_ENTRY}"),
        "and will receive from RendiW the following:".into(),
        format!("\rPlatinum x 12{DIALOG_TAIL_ARGS}"),
        SUCCESS_LINE.into(),
    ]);
    assert_eq!(trades.len(), 1);
    assert_eq!(
        (trades[0].kind, trades[0].plat_change),
        (TradeKind::Sale, 12)
    );
    assert_eq!(names(&trades[0]), ["Vicious Bond (RARE RANK 0)"]);
}

#[test]
fn does_not_take_what_follows_a_glued_entry_on_the_same_line_for_items() {
    let trades = emit_all(&[
        DIALOG_HEAD.to_string(),
        format!("\rVicious Bond (RARE RANK 0)\r{NEXT_ENTRY}\rArcane Energize (LEGENDARY RANK 5)"),
        "and will receive from RendiW the following:".into(),
        format!("\rPlatinum x 12{DIALOG_TAIL_ARGS}"),
        SUCCESS_LINE.into(),
    ]);
    assert_eq!(trades.len(), 1);
    assert_eq!(
        (trades[0].kind, trades[0].plat_change),
        (TradeKind::Sale, 12)
    );
    assert_eq!(names(&trades[0]), ["Vicious Bond (RARE RANK 0)"]);
}

#[test]
fn cuts_an_unsealed_dialog_at_the_glue_when_its_tail_never_arrives() {
    let trades = emit_all(&[
        DIALOG_HEAD.to_string(),
        "\rVicious Bond (RARE RANK 0)".into(),
        "and will receive from RendiW the following:".into(),
        format!("\rPlatinum x 12\r{NEXT_ENTRY}"),
        "Arcane Energize (LEGENDARY RANK 5)".into(),
        SUCCESS_LINE.into(),
    ]);
    assert_eq!(trades.len(), 1);
    assert_eq!(
        (
            trades[0].kind,
            trades[0].plat_change,
            trades[0].partner.as_str()
        ),
        (TradeKind::Sale, 12, "RendiW")
    );
    assert_eq!(names(&trades[0]), ["Vicious Bond (RARE RANK 0)"]);
}

#[test]
fn reads_a_dialog_whose_start_line_follows_a_previous_dialogs_tail() {
    let mut lines = vec![format!("{PREVIOUS_DIALOG}\r{DIALOG_HEAD}")];
    lines.extend(reported_sale_lines().into_iter().skip(1));
    lines.push(SUCCESS_LINE.into());
    let trades = emit_all(&lines);
    assert_eq!(trades.len(), 1);
    assert_reported_sale(&trades[0]);
}

#[test]
fn seals_a_single_line_dialog_at_once() {
    let single = [
        DIALOG_HEAD.to_string(),
        "Vicious Bond (RARE RANK 0)".into(),
        "and will receive from RendiW the following:".into(),
        format!("Platinum x 12{DIALOG_TAIL_ARGS}"),
    ]
    .join("\r");
    let trades = emit_all(&[
        single,
        "Arcane Energize (LEGENDARY RANK 5)".into(),
        SUCCESS_LINE.into(),
    ]);
    assert_eq!(trades.len(), 1);
    assert_reported_sale(&trades[0]);
}

#[test]
fn drops_a_dialog_that_never_resolves_within_the_timeout() {
    let mut p = TradeDialogParser::new();
    p.feed_line(DIALOG_HEAD, 0.0);
    p.feed_line("and will receive from RendiW the following:", 1.0);
    assert!(p.feed_line(SUCCESS_LINE, 61_000.0).is_none());
}

// -- localized dialogs (French EE.log, partner anonymised) -----------------

const FR_HEAD: &str = "1328.961 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Souhaitez-vous vraiment accepter cet échange? Vous proposez: ";
const FR_TAIL: &str =
    ", title= leftItem=/Menu/Confirm_Item_Ok, rightItem=/Menu/Confirm_Item_Cancel)";
const FR_OK: &str = "1333.177 Script [Info]: Dialog.lua: Dialog::CreateOk(description=L'échange a réussi!, title= leftItem=/Menu/Confirm_Item_Ok)";
const SEND_4: &str = "1332.221 Script [Info]: Dialog.lua: Dialog::SendResult(4)";

fn fr_dialog(item: &str, plat: u32) -> Vec<String> {
    vec![
        "1327.951 Script [Info]: Trade.lua: Missing icon data!".into(),
        FR_HEAD.into(),
        item.into(),
        String::new(),
        "et vous recevrez de TraderName ceci: ".into(),
        format!("Platinum x {plat}{FR_TAIL}"),
    ]
}

fn fr_trades(item: &str, plat: u32, rest: &[&str]) -> Vec<ParsedLogTrade> {
    let mut lines = fr_dialog(item, plat);
    lines.extend(rest.iter().map(|l| l.to_string()));
    emit_all(&lines)
}

#[test]
fn reads_the_three_french_sales() {
    for (item, plat) in [
        ("Nova Prime (Schéma)", 8),
        ("Wukong Prime - Systèmes (Schéma)", 11),
        ("Nova Prime - Systèmes (Schéma)", 15),
    ] {
        let trades = fr_trades(item, plat, &[SEND_4, FR_OK]);
        assert_eq!(trades.len(), 1, "{item}");
        let t = &trades[0];
        assert_eq!(
            (t.kind, t.partner.as_str(), t.plat_change),
            (TradeKind::Sale, "TraderName", plat)
        );
        assert_eq!(t.items, [item_of(item)]);
    }
}

fn item_of(name: &str) -> ParsedLogTradeItem {
    item(name, 1, TradeDirection::Given)
}

#[test]
fn a_french_trade_without_the_success_dialog_does_not_count() {
    assert!(fr_trades("Nova Prime (Schéma)", 8, &[SEND_4]).is_empty());
    assert!(fr_trades("Nova Prime (Schéma)", 8, &[]).is_empty());
}

#[test]
fn a_french_trade_that_ends_in_another_dialog_does_not_count() {
    let failed = "1333.177 Script [Info]: Dialog.lua: Dialog::CreateOk(description=L'échange a échoué., title= leftItem=/Menu/Confirm_Item_Ok)";
    assert!(fr_trades("Nova Prime (Schéma)", 8, &[SEND_4, failed]).is_empty());
}

#[test]
fn a_cancelled_french_trade_does_not_leak_into_the_next_dialog() {
    let other = "1400.000 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Acheter ceci?, title= leftItem=/x)";
    assert!(fr_trades("Nova Prime (Schéma)", 8, &[other, SEND_4, FR_OK]).is_empty());
}

#[test]
fn a_confirmation_that_is_not_a_trade_does_not_count() {
    // No Trade.lua line: an unlisted language is read only while the trade window is live.
    let lines = [
        "9.0 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Sicher? Du gibst: ",
        "Rivenmod",
        "",
        "und erhältst von Name: ",
        "Platinum x 5, title= leftItem=/x)",
        SEND_4,
        "9.5 Script [Info]: Dialog.lua: Dialog::CreateOk(description=Fertig, title= leftItem=/x)",
    ];
    assert!(emit_all(&lines).is_empty());
}

#[test]
fn an_unlisted_language_is_read_by_structure_while_the_trade_window_is_live() {
    let lines = [
        "9.0 Script [Info]: Trade.lua: Missing icon data!",
        "9.1 Script [Info]: Dialog.lua: Dialog::CreateOkCancel(description=Sicher? Du gibst: ",
        "Nova Prime (Bauplan)",
        "",
        "und erhältst von Handler ein: ",
        "Platinum x 5, title= leftItem=/x)",
        SEND_4,
        "9.5 Script [Info]: Dialog.lua: Dialog::CreateOk(description=Fertig, title= leftItem=/x)",
    ];
    let trades = emit_all(&lines);
    assert_eq!(trades.len(), 1);
    assert_eq!(
        (trades[0].partner.as_str(), trades[0].plat_change),
        ("Handler", 5)
    );
}

#[test]
fn the_english_flow_still_completes_on_its_success_line() {
    let mut lines = reported_sale_lines();
    lines.push(SUCCESS_LINE.into());
    assert_eq!(emit_all(&lines).len(), 1);
}
