//! Live trade tracker (port of WFHelper `tradeTracker.recordTradeFromLog` + `tradeWorkflow`): a
//! confirmed trade from the EE.log dialog lands in the ledger once, is matched read-only against
//! the user's WFM listings, and is announced as `trade-detected`. It closes a listing only when the
//! user turned on `auto_close_trade_listings`; otherwise that stays a click (`workbench_close_match`,
//! WFM rules in `docs/wfm-auth.md` §4).

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{Line, LineTracker};
use crate::db::Db;
use crate::services::eelog::Source;
use crate::services::eelog_parsers::trade::{
    ParsedLogTrade, TradeDialogParser, TradeDirection, TradeKind,
};
use crate::services::gdpr_import::{Direction, EventSource, TradeEvent, TradeItem, TradeType};
use crate::services::item_db::ItemDbState;
use crate::services::ledger;
use crate::services::notify::channels::NotificationSource;
use crate::services::notify::{NotificationDispatch, Notifications, Notifier};
use crate::services::wfm::{catalog, session::Session};
use crate::services::workbench::commands::WfmHost;
use crate::services::workbench::matcher::{
    Matcher, WorkbenchTrade, WorkbenchTradeItem, WorkbenchTradeMatch, normalize_name,
    parse_traded_name, split_paren_suffix,
};
use crate::store::Store;

/// The same log entry reaches us from the file and the live stream; stamped ones dedupe exactly.
const DUPLICATE_WINDOW_MS: f64 = 30_000.0;
const SCHEMA_VERSION: u32 = 2;
pub const EVENT: &str = "trade-detected";

/// A recorded trade and the listings it would close (empty when signed out or nothing matches).
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TradeDetected {
    pub trade: TradeEvent,
    pub matches: Vec<WorkbenchTradeMatch>,
    /// Order ids of `matches` already closed (automatically); the popup shows them without a button.
    pub closed: Vec<String>,
    /// The automatic close copied the thank-you message; the popup says so.
    pub thanks_copied: bool,
}

#[derive(Default)]
struct Recent(HashMap<String, (f64, HashSet<String>)>);

impl Recent {
    /// False when `t` is a re-delivery of a trade seen in the last 30 s.
    fn is_new(&mut self, t: &ParsedLogTrade, now_ms: f64) -> bool {
        let mut parts: Vec<String> = t
            .items
            .iter()
            .map(|i| {
                format!(
                    "{:?}:{}:{}",
                    i.direction,
                    i.display_name.to_lowercase(),
                    i.count
                )
            })
            .collect();
        parts.sort();
        let signature = format!(
            "{:?}|{}|{}|{}",
            t.kind,
            t.plat_change,
            t.partner.to_lowercase(),
            parts.join("|")
        );
        self.0
            .retain(|_, (at, _)| now_ms - *at <= DUPLICATE_WINDOW_MS);
        let stamp = t.log_stamp.clone();
        let Some((at, stamps)) = self.0.get_mut(&signature) else {
            self.0
                .insert(signature, (now_ms, stamp.into_iter().collect()));
            return true;
        };
        // Two identical trades in a row differ by their stamp; an unstamped repeat is an echo.
        let distinct = stamp
            .as_ref()
            .is_some_and(|s| !stamps.is_empty() && !stamps.contains(s));
        if distinct {
            stamps.extend(stamp);
            *at = now_ms;
        }
        distinct
    }
}

pub struct TradeTracker {
    // One parser per source: interleaved half-dialogs from the file and DBWIN corrupt each other.
    file: TradeDialogParser,
    dbwin: TradeDialogParser,
    recent: Recent,
    on_trade: Box<dyn FnMut(ParsedLogTrade) + Send>,
}

impl TradeTracker {
    pub fn new(on_trade: Box<dyn FnMut(ParsedLogTrade) + Send>) -> Self {
        Self {
            file: TradeDialogParser::new(),
            dbwin: TradeDialogParser::new(),
            recent: Recent::default(),
            on_trade,
        }
    }
}

impl LineTracker for TradeTracker {
    fn on_line(&mut self, line: &Line) {
        let parser = match line.source {
            Source::File => &mut self.file,
            Source::Dbwin => &mut self.dbwin,
        };
        if let Some(t) = parser.feed_line(line.text, line.now_ms)
            && self.recent.is_new(&t, line.now_ms)
        {
            (self.on_trade)(t);
        }
    }

    // `recent` survives: a restart of the log must not re-record the trade it re-reads.
    fn on_reset(&mut self) {
        self.file.reset();
        self.dbwin.reset();
    }
}

/// What the WFM catalog knows about a traded name: game ref, slug, thumb.
type Known = (Option<String>, String, Option<String>);

fn to_event(
    t: &ParsedLogTrade,
    catalog: &HashMap<String, Known>,
    id: String,
    date: String,
) -> TradeEvent {
    let items = t
        .items
        .iter()
        .map(|i| {
            let known = catalog.get(&normalize_name(
                &parse_traded_name(&i.display_name).base_name,
            ));
            TradeItem {
                internal_name: known.and_then(|k| k.0.clone()).unwrap_or_default(),
                display_name: i.display_name.clone(),
                count: i.count,
                direction: match i.direction {
                    TradeDirection::Given => Direction::Given,
                    TradeDirection::Received => Direction::Received,
                },
                wfm_slug: known.map(|k| k.1.clone()),
                wfm_thumb: known.and_then(|k| k.2.clone()),
            }
        })
        .collect();
    TradeEvent {
        id,
        date,
        kind: match t.kind {
            TradeKind::Sale => TradeType::Sale,
            TradeKind::Purchase => TradeType::Purchase,
            TradeKind::Trade => TradeType::Trade,
        },
        plat_change: i64::from(t.plat_change),
        items,
        partner: Some(t.partner.clone()).filter(|p| !p.is_empty()),
        schema_version: Some(SCHEMA_VERSION),
        source: Some(EventSource::Live),
        source_record_id: None,
        import_batch_id: None,
        wfm_closed: None,
        credits: None,
        trade_tax: None,
    }
}

fn workbench_trade(e: &TradeEvent) -> WorkbenchTrade {
    WorkbenchTrade {
        partner: e.partner.clone().unwrap_or_default(),
        plat_change: e.plat_change,
        kind: e.kind,
        items: e
            .items
            .iter()
            .map(|i| WorkbenchTradeItem {
                display_name: i.display_name.clone(),
                count: i.count,
                direction: i.direction,
            })
            .collect(),
    }
}

/// "2x Serration +1 15p with Kestrel": the traded-away item of a sale, the bought one of a
/// purchase, as WFHelper's `summarizeTrade` picks it.
fn notification_body(e: &TradeEvent) -> String {
    let on_side = |i: &&TradeItem| match e.kind {
        TradeType::Trade => !i.display_name.eq_ignore_ascii_case("platinum"),
        TradeType::Sale => i.direction == Direction::Given,
        TradeType::Purchase => i.direction == Direction::Received,
    };
    let mut items: Vec<&TradeItem> = e.items.iter().filter(on_side).collect();
    if items.is_empty() {
        items = e.items.iter().collect();
    }
    let mut body = match items.first() {
        Some(i) => {
            let qty = if i.count > 1 {
                format!("{}x ", i.count)
            } else {
                String::new()
            };
            let extra = match items.len() {
                1 => String::new(),
                n => format!(" +{}", n - 1),
            };
            format!("{qty}{}{extra}", i.display_name)
        }
        None => "Trade".into(),
    };
    if e.kind != TradeType::Trade && e.plat_change > 0 {
        body += &format!(" {}p", e.plat_change);
    }
    match &e.partner {
        Some(p) => body + " with " + p,
        None => body,
    }
}

async fn known_items(app: &AppHandle) -> HashMap<String, Known> {
    let (session, db) = (app.state::<Session>(), app.state::<Db>());
    // Offline or signed out: the trade is still recorded, just without slugs.
    let items = catalog::items(session.client(), &db)
        .await
        .unwrap_or_default();
    items
        .into_iter()
        .map(|i| (normalize_name(&i.name), (i.game_ref, i.slug, i.thumb)))
        .collect()
}

/// The dialog prints names in the game language; the ledger, WFM catalog and inventory use English.
/// Unknown names (no dict for the language, or not an item) stay as printed.
fn to_english(trade: &mut ParsedLogTrade, english: &HashMap<String, String>) {
    for i in &mut trade.items {
        if let Some(name) = english.get(&i.display_name.to_lowercase()) {
            i.display_name.clone_from(name);
        } else if let Some((base, suffix)) = split_paren_suffix(&i.display_name) {
            // Mods/arcanes carry a rank suffix the dictionary keys lack; keep it as printed.
            if let Some(name) = english.get(&base.to_lowercase()) {
                i.display_name = format!("{name} {suffix}");
            }
        }
    }
}

async fn record(app: AppHandle, mut parsed: ParsedLogTrade) {
    let (db, locale) = app.state::<ItemDbState>().snapshot();
    to_english(&mut parsed, &db.english_by_localized(&locale));
    let date = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let id = format!("{date}-{}", &uuid::Uuid::new_v4().simple().to_string()[..4]);
    let event = to_event(&parsed, &known_items(&app).await, id, date);
    let saved = {
        let db = app.state::<Db>();
        ledger::add_events(&mut db.0.lock().unwrap(), std::slice::from_ref(&event))
    };
    match saved {
        Ok(o) if o.applied == 1 => {}
        Ok(_) => return,
        Err(e) => return tracing::warn!("[Trade] ledger write failed: {e}"),
    }
    tracing::info!(
        "[Trade] EE.log trade: {:?} {}p with {}, {} item(s)",
        event.kind,
        event.plat_change,
        parsed.partner,
        event.items.len()
    );
    crate::services::inventory::apply_trade(&app, &event);
    let matches = app
        .state::<Matcher<WfmHost>>()
        .match_trade(&workbench_trade(&event))
        .await;
    app.state::<Notifications>()
        .notify(NotificationDispatch::new(
            NotificationSource::TradeToast,
            if matches.is_empty() {
                "Trade detected"
            } else {
                "Trade matches a listing"
            },
            notification_body(&event),
        ));
    let auto_close = app.state::<Store>().read(|s| s.auto_close_trade_listings);
    let (closed, thanks_copied) = if auto_close {
        crate::services::overlay::trade::close_trade_matches(&app, &event.id, &matches, true).await
    } else {
        (Vec::new(), false)
    };
    let detected = TradeDetected {
        trade: event,
        matches,
        closed,
        thanks_copied,
    };
    crate::services::overlay::trade::enqueue(&app, detected.clone());
    let _ = app.emit(EVENT, detected);
}

/// The live tracker for the hub.
pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    let app = app.clone();
    Box::new(TradeTracker::new(Box::new(move |t| {
        tauri::async_runtime::spawn(record(app.clone(), t));
    })))
}

#[cfg(test)]
mod tests;
