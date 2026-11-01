//! Reward scan orchestration policy (WFHelper `ipc/overlay/scan.ts`): retry a capture+read until
//! it matches the counted cards, and decide when the overlay closes. Pure over `tokio::time`, so
//! tests drive it with a paused clock and scripted attempts.
use std::future::Future;
use std::time::Duration;

use tokio::time::{Instant, sleep};

use super::RewardReader;
use super::names::PoolItem;
use super::slots::{FrameScan, fallback_names, match_slot, scan_frame};
use crate::services::capture::CaptureSource;

const RETRY_WINDOW: Duration = Duration::from_secs(5);
const RETRY_INTERVAL: Duration = Duration::from_millis(450);
const MAX_ATTEMPTS: u32 = 10;
/// Consecutive frames without a card layout before the trigger is written off as a false one.
const NO_LAYOUT_MAX: u32 = 3;
/// One more try when a fade leaves reward slots unread.
const PARTIAL_BONUS: u32 = 1;
/// "Got rewards" to the first capture. Earlier than the ~650 ms AlecaFrame waits: a frame caught
/// before the cards are drawn counts as no layout and is retried (`RETRY_INTERVAL`), so the wait
/// costs nothing when the cards come late.
pub const EELOG_SCAN_DELAY: Duration = Duration::from_millis(450);
/// The relic vote ends 15 s after the trigger; hide just before.
const VOTE_WINDOW: Duration = Duration::from_millis(14_500);
/// Never show the card for less than this, however late the scan finished.
const MIN_VISIBLE: Duration = Duration::from_secs(5);
const CLOSE_GRACE: Duration = Duration::from_millis(1_500);
/// A close line that arrives this late describes a screen long gone.
const CLOSE_STALE_MAX: Duration = Duration::from_secs(5);
pub const HOTKEY_HIDE: Duration = Duration::from_millis(8_500);
pub const FAILURE_HIDE: Duration = Duration::from_millis(3_500);
/// Longest a card sits in "detecting" before the window goes away on its own.
pub const DETECTING_MAX: Duration = Duration::from_secs(20);
/// A trigger right after a close is the same reward screen; do not reopen it.
const REOPEN_SUPPRESS: Duration = Duration::from_secs(3);

/// One capture+read. `items` has one entry per counted card, `None` where nothing matched.
pub struct Attempt<T> {
    pub cards: usize,
    pub items: Vec<Option<T>>,
}

impl<T> Attempt<T> {
    fn read(&self) -> usize {
        self.items.iter().flatten().count()
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome<T> {
    /// Every counted card was read.
    Read(Vec<Option<T>>),
    /// Cards were read but not all of them: showing it would be a wrong set.
    Partial {
        read: usize,
        cards: usize,
    },
    /// No card layout in the first frames: the trigger was not a reward screen.
    NotRewardScreen,
    Nothing,
}

/// Runs `scan` until a frame reads every counted card. `Ok(None)` from `scan` = no layout in the
/// frame. `Err` only when every attempt failed to even get a frame (the last error).
pub async fn run_attempts<T, E, F, Fut>(mut scan: F) -> Result<Outcome<T>, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Option<Attempt<T>>, E>>,
{
    let start = Instant::now();
    let (mut attempts, mut no_layout, mut partial_tries) = (0, 0, 0);
    let mut best: Option<Attempt<T>> = None;
    let mut last_err = None;
    let mut got_frame = false;
    let mut layout_gone = false;
    while attempts < MAX_ATTEMPTS && start.elapsed() < RETRY_WINDOW {
        attempts += 1;
        match scan().await {
            Ok(Some(a)) => {
                got_frame = true;
                no_layout = 0;
                let (read, cards) = (a.read(), a.cards);
                if read == cards && cards > 0 {
                    return Ok(Outcome::Read(a.items));
                }
                if read > 0 {
                    if partial_tries >= PARTIAL_BONUS {
                        return Ok(Outcome::Partial { read, cards });
                    }
                    partial_tries += 1;
                }
                if best.as_ref().is_none_or(|b| read > b.read()) {
                    best = Some(a);
                }
            }
            r => {
                match r {
                    Err(e) => last_err = Some(e),
                    _ => got_frame = true,
                }
                no_layout += 1;
                if no_layout >= NO_LAYOUT_MAX && best.is_none() {
                    layout_gone = got_frame;
                    break;
                }
            }
        }
        let remaining = RETRY_WINDOW.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            break;
        }
        sleep(RETRY_INTERVAL.min(remaining)).await;
    }
    match (best, last_err) {
        (Some(b), _) if b.read() > 0 => Ok(Outcome::Partial {
            read: b.read(),
            cards: b.cards,
        }),
        (_, Some(e)) if !got_frame => Err(e),
        _ if layout_gone => Ok(Outcome::NotRewardScreen),
        _ => Ok(Outcome::Nothing),
    }
}

/// One capture, card count, title reads and matching: the English names of the matched cards.
/// `readers` go in priority order over the same frame: a later one runs only while slots are
/// unread and fills just those (Paddle first, then Windows OCR as a second vote).
pub fn scan_once(
    src: &dyn CaptureSource,
    readers: &[&dyn RewardReader],
    pool: &[PoolItem],
    ui_scale: f64,
) -> Result<Option<(Attempt<String>, ScanDebug)>, String> {
    let t = std::time::Instant::now();
    let frame = src.capture()?;
    tracing::debug!(elapsed = ?t.elapsed(), "reward scan: captured");
    crate::services::scan_captures::keep(&frame.image);
    let mut scan: Option<FrameScan> = None;
    let mut items: Vec<Option<String>> = Vec::new();
    for &reader in readers {
        let t = std::time::Instant::now();
        let Some(next) = scan_frame(reader, &frame.image, ui_scale) else {
            continue;
        };
        tracing::debug!(elapsed = ?t.elapsed(), "reward scan: reader pass (count + OCR)");
        let t = std::time::Instant::now();
        let read: Vec<Option<String>> = next
            .slots
            .iter()
            .map(|s| {
                let best = match_slot(&s.candidates, pool).into_iter().next()?;
                Some(best.item?.english().to_owned())
            })
            .collect();
        tracing::debug!(elapsed = ?t.elapsed(), "reward scan: match");
        match &mut scan {
            None => {
                items = read;
                scan = Some(next);
            }
            // Another card count is another reading of the layout: the first reader's stands.
            Some(first) if first.cards == next.cards => {
                for ((have, got), (slot, more)) in items
                    .iter_mut()
                    .zip(read)
                    .zip(first.slots.iter_mut().zip(next.slots))
                {
                    if have.is_none() {
                        *have = got;
                    }
                    slot.candidates.extend(more.candidates);
                }
            }
            Some(_) => {}
        }
        if !items.is_empty() && items.iter().all(Option::is_some) {
            break;
        }
    }
    if items.iter().all(Option::is_none) {
        for &reader in readers.iter().filter(|r| r.text_fallback()) {
            let (text, names) = fallback_names(reader, &frame.image, pool);
            if !names.is_empty() {
                let debug = ScanDebug {
                    cards: names.len() as u32,
                    slots: vec![vec![text]],
                };
                let attempt = Attempt {
                    cards: names.len(),
                    items: names.into_iter().map(Some).collect(),
                };
                return Ok(Some((attempt, debug)));
            }
        }
    }
    let Some(scan) = scan else { return Ok(None) };
    let debug = ScanDebug {
        cards: scan.cards as u32,
        slots: scan.slots.into_iter().map(|s| s.candidates).collect(),
    };
    Ok(Some((
        Attempt {
            cards: scan.cards,
            items,
        },
        debug,
    )))
}

/// What the last attempt saw, for `reward_scan_debug`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanDebug {
    pub cards: u32,
    /// OCR candidate texts per slot.
    pub slots: Vec<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    Hotkey,
    EeLog,
}

/// Round bookkeeping: one scan at a time, EE.log timing for the auto-close, reopen suppression.
#[derive(Default)]
pub struct Session {
    in_flight: bool,
    token: u64,
    eelog_at: Option<Instant>,
    screen_closed: bool,
    closed_at: Option<Instant>,
    /// When the last EE.log round's screen appeared; kept through hotkey rounds.
    last_eelog: Option<Instant>,
}

impl Session {
    /// `Some(token)` when a scan should start now. `staleness` = how old the trigger line is.
    pub fn begin(&mut self, t: Trigger, staleness: Duration, now: Instant) -> Option<u64> {
        let suppressed = t == Trigger::EeLog
            && self
                .closed_at
                .is_some_and(|c| now.saturating_duration_since(c) < REOPEN_SUPPRESS);
        if self.in_flight || suppressed {
            return None;
        }
        self.in_flight = true;
        self.token += 1;
        if t == Trigger::EeLog {
            // Backdated by the log flush lag, so the close tracks when the screen really appeared.
            self.eelog_at = Some(now.checked_sub(staleness).unwrap_or(now));
            self.last_eelog = self.eelog_at;
            self.screen_closed = false;
        } else {
            self.eelog_at = None;
        }
        Some(self.token)
    }

    /// The last EE.log reward screen is over (its close line came, or its vote window ran out),
    /// so a hotkey scan that reads nothing is looking at no reward screen, not a bad read.
    pub fn reward_screen_gone(&self, now: Instant) -> bool {
        self.last_eelog
            .is_some_and(|at| self.screen_closed || now.saturating_duration_since(at) > VOTE_WINDOW)
    }

    pub fn finish(&mut self) {
        self.in_flight = false;
    }

    pub fn token(&self) -> u64 {
        self.token
    }

    /// The overlay went away (user, auto-close or hotkey).
    pub fn closed(&mut self, now: Instant) {
        self.closed_at = Some(now);
    }

    /// Wait before capturing: the fixed post-trigger delay, counted from the trigger itself.
    pub fn scan_delay(&self, now: Instant) -> Duration {
        self.eelog_at.map_or(Duration::ZERO, |at| {
            (at + EELOG_SCAN_DELAY).saturating_duration_since(now)
        })
    }

    /// How long a successful read stays up. `omnia` fissures crack back-to-back, so they hide sooner.
    pub fn success_hide(&self, now: Instant, omnia: bool) -> Duration {
        let Some(at) = self.eelog_at else {
            return HOTKEY_HIDE;
        };
        if self.screen_closed {
            return MIN_VISIBLE;
        }
        let vote = if omnia {
            Duration::from_secs(10)
        } else {
            VOTE_WINDOW
        };
        (at + vote).saturating_duration_since(now).max(MIN_VISIBLE)
    }

    /// The game's reward screen closed. `Some(delay)`: hide the overlay after it; `None`: not for
    /// this round, or a scan is running (its success path shortens its own delay).
    pub fn on_screen_closed(&mut self, staleness: Duration, now: Instant) -> Option<Duration> {
        let at = self.eelog_at?;
        if staleness > CLOSE_STALE_MAX || self.screen_closed {
            return None;
        }
        self.screen_closed = true;
        if self.in_flight {
            return None;
        }
        Some(CLOSE_GRACE.max((at + MIN_VISIBLE).saturating_duration_since(now)))
    }
}
