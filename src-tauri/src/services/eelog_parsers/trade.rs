//! Player-trade confirmation dialog: buffers the dialog's lines from its start
//! marker to the success line, then reads partner, platinum and items out of it.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::sanitize::{is_log_framework_line, strip_dialog_arg_tail, strip_platform_glyphs};

const TRADE_DIALOG_START: &str = "Are you sure you want to accept this trade?";
const TRADE_SUCCESS: &str = "The trade was successful!";
const OFFERING: &str = "You are offering:";
const CREATE_OK_CANCEL: &str = "Dialog::CreateOkCancel(description=";
const CREATE_OK: &str = "Dialog::CreateOk(description=";
const SEND_OK: &str = "Dialog::SendResult(4)";
/// Verified (start, success) texts per game language. Add a language by copying both from
/// an EE.log trade (a `Dialog::CreateOkCancel` confirmation and the `Dialog::CreateOk` after it).
/// A language missing here still works through the structural path below, but a failed trade
/// cannot be told from a completed one there.
const LOCALIZED: [(&str, &str); 1] = [(
    "Souhaitez-vous vraiment accepter cet échange?",
    "L'échange a réussi!",
)];
/// Trade.lua logs while the trade window is open; gates the language-independent start.
const TRADE_UI_MARK: &str = "Trade.lua";
/// Max time to wait for the confirmation dialog to resolve before discarding buffered lines.
const TRADE_DIALOG_TIMEOUT_MS: f64 = 60_000.0;

macro_rules! rx {
    ($name:ident, $p:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($p).unwrap());
    };
}

rx!(DIALOG_TAIL, r"\b(?:leftItem|rightItem)=");
// JS `.` stops at CR as well as LF, and the dialog is CR-separated in places.
rx!(
    RECEIVE,
    r"(?i)and will receive from\s+([^\n\r]+?)\s+the following:"
);
rx!(RECEIVE_FR, r"(?i)de\s+(\S+)\s+ceci\s*:");
rx!(ARG_LINE, r"^(?:leftItem|rightItem|title)=");
rx!(PLATINUM, r"(?i)^Platinum(?:\s+x\s+(\d+))?$");
rx!(STACK, r"(?i)^(.+?)\s+x\s+(\d+)$");
rx!(LOG_STAMP, r"^(\d+\.\d+)\b");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeKind {
    Sale,
    Purchase,
    Trade,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeDirection {
    Given,
    Received,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLogTradeItem {
    pub display_name: String,
    pub count: u32,
    pub direction: TradeDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedLogTrade {
    pub partner: String,
    pub plat_change: u32,
    #[serde(rename = "type")]
    pub kind: TradeKind,
    pub items: Vec<ParsedLogTradeItem>,
    /// Game uptime stamp of the dialog line - identifies this exact trade.
    pub log_stamp: Option<String>,
}

/// Items plus platinum of one side of the dialog.
fn parse_item_block(block: &str, direction: TradeDirection) -> (Vec<ParsedLogTradeItem>, u32) {
    let mut plat = 0;
    let mut counts: Vec<(String, u32)> = Vec::new();
    // The description separates its items with a bare CR as well as LF, and the
    // engine can flush the next log entry behind one without starting a line.
    for raw in block.split(['\r', '\n']) {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        // Stop if we hit the closing part of Dialog args
        if ARG_LINE.is_match(line) {
            break;
        }
        // Cut the Dialog arg tail before the framework check, or a log entry
        // flushed onto that tail rejects the item line too.
        let without_tail = strip_dialog_arg_tail(line);
        if is_log_framework_line(&without_tail) {
            continue;
        }
        let cleaned = strip_platform_glyphs(&without_tail);
        if cleaned.is_empty() {
            continue;
        }
        if let Some(m) = PLATINUM.captures(&cleaned) {
            plat += m.get(1).map_or(1, |n| n.as_str().parse().unwrap_or(0));
            continue;
        }
        // Stacked items log as "Name x N" (one slot); non-stacking items repeat one line each.
        let (name, qty) = match STACK.captures(&cleaned) {
            Some(m) => (
                m[1].trim().to_string(),
                m[2].parse::<u32>().ok().filter(|&q| q > 0).unwrap_or(1),
            ),
            None => (cleaned, 1),
        };
        match counts.iter_mut().find(|(n, _)| *n == name) {
            Some((_, c)) => *c += qty,
            None => counts.push((name, qty)),
        }
    }
    let items = counts
        .into_iter()
        .map(|(display_name, count)| ParsedLogTradeItem {
            display_name,
            count,
            direction,
        })
        .collect();
    (items, plat)
}

/// Language-independent split: the first line ending in ":" opens the offered items and the
/// next one is the divider naming the partner. Returns (offered block, partner, received block).
fn split_by_structure(text: &str) -> Option<(&str, String, &str)> {
    let mut colons = Vec::new();
    let mut at = 0;
    for raw in text.split_inclusive(['\r', '\n']) {
        let line = strip_dialog_arg_tail(raw).trim().to_string();
        if line.ends_with(':') {
            colons.push((at, at + raw.len(), line));
        }
        at += raw.len();
    }
    let ((_, offer_start, _), (div_start, div_end, div)) = (colons.first()?, colons.get(1)?);
    let partner = RECEIVE_FR
        .captures(div)
        .map(|c| c[1].to_string())
        // Unknown language: the name is the one word that is not all lowercase.
        .or_else(|| {
            div.split_whitespace()
                .find(|w| w.chars().any(|c| !c.is_lowercase() && !"':-".contains(c)))
                .map(str::to_string)
        })
        .unwrap_or_default();
    Some((
        &text[*offer_start..*div_start],
        strip_platform_glyphs(&partner),
        &text[*div_end..],
    ))
}

pub fn parse_trade_dialog(lines: &[String]) -> Option<ParsedLogTrade> {
    let text = lines.join("\n");
    let (offer_block, partner, receive_block) = match text.find(OFFERING) {
        Some(from) => {
            let desc = &text[from..];
            // Split on the "and will receive from <partner> the following:" divider
            let receive = RECEIVE.captures(desc)?;
            let divider = receive.get(0)?;
            // The game may append a platform glyph to the username (same as whispers).
            (
                &desc[OFFERING.len()..divider.start()],
                strip_platform_glyphs(&receive[1]),
                &desc[divider.end()..],
            )
        }
        None => split_by_structure(&text)?,
    };

    let (offered, plat_spent) = parse_item_block(offer_block, TradeDirection::Given);
    let (received, plat_gained) = parse_item_block(receive_block, TradeDirection::Received);

    let kind = if plat_gained > 0 && plat_spent == 0 {
        TradeKind::Sale
    } else if plat_spent > 0 && plat_gained == 0 {
        TradeKind::Purchase
    } else {
        TradeKind::Trade
    };

    Some(ParsedLogTrade {
        partner,
        plat_change: plat_gained.max(plat_spent),
        kind,
        items: offered.into_iter().chain(received).collect(),
        log_stamp: lines
            .first()
            .and_then(|l| LOG_STAMP.captures(l))
            .map(|c| c[1].to_string()),
    })
}

/// Buffers dialog lines as they arrive; `feed_line` yields the trade when its success line lands.
#[derive(Default)]
pub struct TradeDialogParser {
    buffer: Option<Vec<String>>,
    start_at_ms: f64,
    /// Set once the dialog's log entry ends - later log lines must not leak into the buffer.
    sealed: bool,
    /// Buffer length when a glued entry was skipped. The dialog's own tail clears it;
    /// a dialog that never got its tail is cut there, so later lines cannot join it.
    glue_mark: Option<usize>,
    /// Last time Trade.lua logged: a confirmation in an unlisted language is only a trade while it is fresh.
    trade_ui_at: Option<f64>,
    /// The dialog is not in the English wording: its success is the `CreateOk` after the confirm click.
    localized: bool,
    /// Wording whose `CreateOk` text is known; `None` accepts any (structural path).
    success_text: Option<&'static str>,
    /// `Dialog::SendResult(4)` came after the sealed dialog.
    confirmed: bool,
}

impl TradeDialogParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The engine can flush its next log entry behind a description line after a
    /// bare CR, so the seal check runs per CR segment: the dialog's own arg tail or
    /// a fresh framework line seals, a glued entry ends the line without sealing.
    fn buffer_line(&mut self, line: &str) {
        let Some(buffer) = self.buffer.as_mut() else {
            return;
        };
        for (index, segment) in line.split('\r').enumerate() {
            if is_log_framework_line(&strip_dialog_arg_tail(segment)) {
                if index == 0 {
                    self.sealed = true;
                } else {
                    self.glue_mark.get_or_insert(buffer.len());
                }
                return;
            }
            if !segment.trim().is_empty() {
                buffer.push(segment.to_string());
            }
            if DIALOG_TAIL.is_match(segment) {
                self.sealed = true;
                self.glue_mark = None;
                return;
            }
        }
    }

    /// Which wording opens this line's trade dialog: English, a verified language, or (only while
    /// the trade window is live) any other `CreateOkCancel`, read by structure.
    #[allow(clippy::type_complexity)]
    fn dialog_start(&self, line: &str, now_ms: f64) -> Option<(usize, bool, Option<&'static str>)> {
        if let Some(i) = line.find(TRADE_DIALOG_START) {
            return Some((i, false, None));
        }
        let i = line.find(CREATE_OK_CANCEL)?;
        if let Some((_, ok)) = LOCALIZED.iter().find(|(start, _)| line.contains(start)) {
            return Some((i, true, Some(ok)));
        }
        let live = self
            .trade_ui_at
            .is_some_and(|at| now_ms - at <= TRADE_DIALOG_TIMEOUT_MS);
        live.then_some((i, true, None))
    }

    fn reset_dialog(&mut self) {
        self.buffer = None;
        self.sealed = false;
        self.localized = false;
        self.confirmed = false;
        self.success_text = None;
    }

    /// A localized trade completes at the `CreateOk` that follows the confirm click; any other
    /// dialog in between means it was not completed.
    fn finish_localized(&mut self, line: &str) -> Option<ParsedLogTrade> {
        if !self.localized {
            return None;
        }
        if line.contains(SEND_OK) {
            self.confirmed = self.sealed;
        } else if line.contains(CREATE_OK) && self.confirmed {
            let ok = self.success_text.is_none_or(|t| line.contains(t));
            let buffer = self.buffer.take()?;
            let cut = self.glue_mark.take().unwrap_or(buffer.len());
            self.reset_dialog();
            return ok.then(|| parse_trade_dialog(&buffer[..cut])).flatten();
        }
        None
    }

    pub fn feed_line(&mut self, line: &str, now_ms: f64) -> Option<ParsedLogTrade> {
        if line.is_empty() {
            return None;
        }
        if line.contains(TRADE_UI_MARK) {
            self.trade_ui_at = Some(now_ms);
        }
        if let Some(t) = self.finish_localized(line) {
            return Some(t);
        }
        // A single-line dialog already carries its arg tail, so it seals at once. Only
        // the tail after the start marker counts: a previous dialog's tail shares the
        // line when the engine flushed this entry behind it.
        if let Some((start, localized, success_text)) = self.dialog_start(line, now_ms) {
            self.buffer = Some(vec![line.to_string()]);
            self.start_at_ms = now_ms;
            self.glue_mark = None;
            self.confirmed = false;
            self.localized = localized;
            self.success_text = success_text;
            self.sealed = DIALOG_TAIL.is_match(&line[start..]);
        } else if self.localized && line.contains(CREATE_OK_CANCEL) {
            self.reset_dialog(); // another confirmation took over: that trade was not confirmed
        } else if self.buffer.is_some() {
            if now_ms - self.start_at_ms > TRADE_DIALOG_TIMEOUT_MS {
                self.reset_dialog();
            } else if !self.sealed {
                self.buffer_line(line);
            }
        }

        if self.localized || !line.contains(TRADE_SUCCESS) {
            return None;
        }
        let buffer = self.buffer.take()?;
        let cut = self.glue_mark.take().unwrap_or(buffer.len());
        self.sealed = false;
        parse_trade_dialog(&buffer[..cut])
    }
}

#[cfg(test)]
mod tests;
