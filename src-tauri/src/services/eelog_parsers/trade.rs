//! Player-trade confirmation dialog: buffers the dialog's lines from its start
//! marker to the success line, then reads partner, platinum and items out of it.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::sanitize::{is_log_framework_line, strip_dialog_arg_tail, strip_platform_glyphs};

const TRADE_DIALOG_START: &str = "Are you sure you want to accept this trade?";
const TRADE_SUCCESS: &str = "The trade was successful!";
const OFFERING: &str = "You are offering:";
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

pub fn parse_trade_dialog(lines: &[String]) -> Option<ParsedLogTrade> {
    let text = lines.join("\n");
    let desc = &text[text.find(OFFERING)?..];

    // Split on the "and will receive from <partner> the following:" divider
    let receive = RECEIVE.captures(desc)?;
    let divider = receive.get(0)?;
    // The game may append a platform glyph to the username (same as whispers).
    let partner = strip_platform_glyphs(&receive[1]);

    let (offered, plat_spent) = parse_item_block(
        &desc[OFFERING.len()..divider.start()],
        TradeDirection::Given,
    );
    let (received, plat_gained) =
        parse_item_block(&desc[divider.end()..], TradeDirection::Received);

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

    pub fn feed_line(&mut self, line: &str, now_ms: f64) -> Option<ParsedLogTrade> {
        if line.is_empty() {
            return None;
        }
        // A single-line dialog already carries its arg tail, so it seals at once. Only
        // the tail after the start marker counts: a previous dialog's tail shares the
        // line when the engine flushed this entry behind it.
        if let Some(start) = line.find(TRADE_DIALOG_START) {
            self.buffer = Some(vec![line.to_string()]);
            self.start_at_ms = now_ms;
            self.glue_mark = None;
            self.sealed = DIALOG_TAIL.is_match(&line[start..]);
        } else if self.buffer.is_some() {
            if now_ms - self.start_at_ms > TRADE_DIALOG_TIMEOUT_MS {
                self.buffer = None;
            } else if !self.sealed {
                self.buffer_line(line);
            }
        }

        if !line.contains(TRADE_SUCCESS) {
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
