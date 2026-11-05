//! Trade notification overlay: a queue of `TradeDetected` shown one at a time, each closing by
//! itself after `AUTO_CLOSE` unless the user acts. The ledger row and the desktop toast are the
//! tracker's job and happen whether or not this overlay is on.
use crate::db::Db;
use crate::services::sync::LockExt;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use super::{OverlayKind, with};
use crate::services::gdpr_import::TradeType;
use crate::services::trackers::TradeDetected;
use crate::services::workbench::commands::WfmHost;
use crate::services::workbench::matcher::{Matcher, WorkbenchTradeMatch};
use crate::store::{Settings, Store};

/// `trade-overlay` carries a [`TradeOverlayView`]; `current: None` means the queue is empty.
pub const EVENT: &str = "trade-overlay";
const AUTO_CLOSE: Duration = Duration::from_secs(20);

static QUEUE: Mutex<Queue> = Mutex::new(Queue::new());

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TradeOverlayView {
    pub current: Option<TradeDetected>,
    /// Trades waiting behind `current`.
    pub queued: u32,
    /// Identifies `current`; actions quote it so a late click cannot hit the next trade.
    #[specta(type = f64)]
    pub token: u64,
    pub auto_close_ms: u32,
}

struct Queue {
    items: VecDeque<TradeDetected>,
    /// Bumps whenever the head changes.
    token: u64,
}

impl Queue {
    const fn new() -> Self {
        Self {
            items: VecDeque::new(),
            token: 0,
        }
    }

    /// True when `t` became the head (the queue was empty), i.e. a timer must start for it.
    fn push(&mut self, t: TradeDetected) -> bool {
        self.items.push_back(t);
        let head = self.items.len() == 1;
        if head {
            self.token += 1;
        }
        head
    }

    /// Drops the head if `token` still names it; true when the next one became the head.
    fn dismiss(&mut self, token: u64) -> Option<bool> {
        if token != self.token || self.items.pop_front().is_none() {
            return None;
        }
        let next = !self.items.is_empty();
        if next {
            self.token += 1;
        }
        Some(next)
    }

    fn view(&self) -> TradeOverlayView {
        TradeOverlayView {
            current: self.items.front().cloned(),
            queued: self.items.len().saturating_sub(1) as u32,
            token: self.token,
            auto_close_ms: AUTO_CLOSE.as_millis() as u32,
        }
    }
}

fn lock() -> std::sync::MutexGuard<'static, Queue> {
    QUEUE.lock_any()
}

/// Tells the window what to show and opens or closes it with the queue.
fn publish(app: &AppHandle, view: TradeOverlayView) {
    let open = view.current.is_some();
    let _ = with(app, |m| match (open, m.is_shown(OverlayKind::Trade)) {
        (true, false) => m.show(OverlayKind::Trade),
        (false, true) => m.hide(OverlayKind::Trade),
        _ => {}
    });
    let _ = app.emit(EVENT, view);
}

/// Closes `token`'s trade after the delay unless the user got there first. Only call from async
/// code (it spawns on the runtime).
fn arm(app: &AppHandle, token: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTO_CLOSE).await;
        dismiss(&app, token);
    });
}

fn dismiss(app: &AppHandle, token: u64) {
    let (next, view) = {
        let mut q = lock();
        let Some(next) = q.dismiss(token) else { return };
        (next, q.view())
    };
    let token = view.token;
    publish(app, view);
    if next {
        arm(app, token);
    }
}

/// Called by the trade tracker once the trade is in the ledger.
pub fn enqueue(app: &AppHandle, t: TradeDetected) {
    let (head, view) = {
        let mut q = lock();
        (q.push(t), q.view())
    };
    let token = view.token;
    publish(app, view);
    if head {
        arm(app, token);
    }
}

/// Ignore: drops the shown trade; the next queued one takes its place.
#[tauri::command]
#[specta::specta]
pub async fn trade_notification_dismiss(app: AppHandle, token: f64) -> Result<(), String> {
    dismiss(&app, token as u64);
    Ok(())
}

/// Closes one of the shown trade's matched listings (the user's explicit confirmation), then
/// dismisses it. False when WFM refused, so the overlay stays for a retry.
#[tauri::command]
#[specta::specta]
pub async fn trade_notification_close_match(
    app: AppHandle,
    token: f64,
    order_id: String,
) -> Result<bool, String> {
    let token = token as u64;
    let found = {
        let q = lock();
        q.items.front().filter(|_| q.token == token).and_then(|d| {
            let m = d.matches.iter().find(|m| m.order_id == order_id)?;
            Some((m.clone(), d.trade.id.clone(), !d.closed.is_empty()))
        })
    };
    let Some((m, event_id, already_closed)) = found else {
        return Err("that trade is no longer showing".into());
    };
    // Only the first close of a trade copies the thank-you (the others closed it automatically).
    let first = !already_closed;
    let closed = !close_trade_matches(&app, &event_id, &[m], first)
        .await
        .is_empty();
    if closed {
        dismiss(&app, token);
    }
    Ok(closed)
}

/// The one close path, for the click and the automatic close: closes `matches` of trade
/// `event_id`, tells the Market view, flags the ledger row, and (when `thank`) copies the
/// thank-you once. Returns the order ids WFM accepted.
pub async fn close_trade_matches(
    app: &AppHandle,
    event_id: &str,
    matches: &[WorkbenchTradeMatch],
    thank: bool,
) -> Vec<String> {
    let closed = app
        .state::<Matcher<WfmHost>>()
        .close_all(matches, |m| {
            crate::services::workbench::commands::announce_order_closed(app, m);
        })
        .await;
    if closed.is_empty() {
        return closed;
    }
    if let Err(e) =
        crate::services::ledger::mark_wfm_closed(&app.state::<Db>().0.lock().unwrap(), event_id)
    {
        tracing::warn!("[Trade] could not flag ledger row {event_id} as closed: {e:?}");
    }
    let kinds = matches
        .iter()
        .filter(|m| closed.contains(&m.order_id))
        .map(|m| m.trade_type);
    let text = app
        .state::<Store>()
        .read(|s| if thank { thanks_text(s, kinds) } else { None });
    if let Some(text) = text {
        match app.clipboard().write_text(text) {
            Ok(()) => tracing::info!("[Trade] thank-you message copied to the clipboard"),
            Err(e) => tracing::warn!("[Trade] could not copy the thank-you message: {e}"),
        }
    }
    closed
}

/// The message to copy: only for a closed sale, when the user enabled it and wrote something.
fn thanks_text(s: &Settings, mut closed: impl Iterator<Item = TradeType>) -> Option<String> {
    let msg = s.trade_thanks_message.trim();
    (s.trade_thanks_enabled && !msg.is_empty() && closed.any(|k| k == TradeType::Sale))
        .then(|| msg.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gdpr_import::{TradeEvent, TradeType};

    fn trade(id: &str) -> TradeDetected {
        TradeDetected {
            trade: TradeEvent {
                id: id.into(),
                date: String::new(),
                kind: TradeType::Sale,
                plat_change: 10,
                items: vec![],
                partner: None,
                schema_version: None,
                source: None,
                source_record_id: None,
                import_batch_id: None,
                wfm_closed: None,
                credits: None,
                trade_tax: None,
            },
            matches: vec![],
            closed: vec![],
        }
    }

    fn thanks(enabled: bool, msg: &str, kinds: &[TradeType]) -> Option<String> {
        let s = Settings {
            trade_thanks_enabled: enabled,
            trade_thanks_message: msg.into(),
            ..Settings::default()
        };
        thanks_text(&s, kinds.iter().copied())
    }

    #[test]
    fn the_thank_you_is_copied_only_for_a_closed_sale_when_enabled() {
        use TradeType::{Purchase, Sale};
        assert_eq!(
            thanks(true, " ty ", &[Purchase, Sale]).as_deref(),
            Some("ty")
        );
        assert_eq!(thanks(true, "ty", &[Purchase]), None);
        assert_eq!(thanks(true, "ty", &[]), None);
        assert_eq!(thanks(false, "ty", &[Sale]), None);
        assert_eq!(thanks(true, "  ", &[Sale]), None);
    }

    fn head(q: &Queue) -> Option<String> {
        q.view().current.map(|d| d.trade.id)
    }

    #[test]
    fn two_trades_queue_and_dismiss_in_order() {
        let mut q = Queue::new();
        assert!(q.push(trade("a")), "first becomes the head");
        assert!(!q.push(trade("b")), "second waits");
        let v = q.view();
        assert_eq!((head(&q).as_deref(), v.queued), (Some("a"), 1));

        assert_eq!(q.dismiss(v.token), Some(true));
        assert_eq!((head(&q).as_deref(), q.view().queued), (Some("b"), 0));
        assert_eq!(q.dismiss(q.token), Some(false));
        assert_eq!(head(&q), None);
    }

    #[test]
    fn a_stale_token_cannot_dismiss_the_next_trade() {
        let mut q = Queue::new();
        q.push(trade("a"));
        q.push(trade("b"));
        let stale = q.view().token;
        q.dismiss(stale);
        // The timer of "a" fires after the user already dismissed it.
        assert_eq!(q.dismiss(stale), None);
        assert_eq!(head(&q).as_deref(), Some("b"));
    }

    #[test]
    fn dismissing_an_empty_queue_is_a_no_op() {
        let mut q = Queue::new();
        assert_eq!(q.dismiss(0), None);
        q.push(trade("a"));
        q.dismiss(q.token);
        assert_eq!(q.dismiss(q.token), None);
    }

    #[test]
    fn a_new_trade_after_the_queue_drained_gets_a_fresh_token() {
        let mut q = Queue::new();
        q.push(trade("a"));
        let old = q.token;
        q.dismiss(old);
        assert!(q.push(trade("b")));
        assert_ne!(q.token, old);
    }
}
