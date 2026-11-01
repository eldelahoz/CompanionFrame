//! Relic planner overlay (the reward window's `planner` mode): the EE.log tells us the relic
//! selection screen opened, the era is read from the fissure tag or off the screen, and the
//! overlay window ranks the player's relics for that era (the ranking itself is frontend code
//! shared with the Relics view). See [`era`] for the OCR half.

pub mod era;

use crate::services::clock::now_ms_f64;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::services::eelog::Source;
use crate::services::eelog_parsers::lines::{
    is_mission_end_line, is_relic_picker_close_line, is_relic_picker_open_line,
    parse_active_mission_tag,
};
use crate::services::overlay::{OverlayKind, overlay_hide, overlay_show};
use crate::services::trackers::{Line, LineTracker};
use era::{EraDetection, FILTER_LABEL};

pub const EVENT: &str = "relic-planner";

const MIN_EELOG_GAP_MS: f64 = 900.0;
const REOPEN_SUPPRESS_MS: f64 = 3_000.0;
/// InitMapping also fires on picker entry; lines this soon after the open are entry, not exit.
const ENTRY_WINDOW_MS: f64 = 800.0;
const CLOSE_COOLDOWN_MS: f64 = 500.0;
/// A tier read off the screen is trusted this long (WFHelper `RELIC_MISSION_TIER_CACHE_TTL_MS`).
const TIER_TTL_MS: f64 = 25.0 * 60.0 * 1000.0;
const CONFIDENT: f64 = 0.9;

const START_DELAY: Duration = Duration::from_millis(100);
const ERA_TIMEOUT: Duration = Duration::from_millis(2_400);
const LABEL_TIMEOUT: Duration = Duration::from_millis(1_500);
const ERA_TOTAL: Duration = Duration::from_millis(3_000);
const RETRY_DELAY: Duration = Duration::from_millis(700);
const MIN_RETRY: Duration = Duration::from_millis(600);
const HIDE_DETECTING: Duration = Duration::from_secs(20);
const HIDE_SUCCESS: Duration = Duration::from_secs(120);
const HIDE_FAILURE: Duration = Duration::from_millis(4_500);

/// EE.log `activeMissionTag` -> era; VoidT6 is an omnia fissure, which takes any era but Requiem.
fn era_of_tag(tag: &str) -> Option<&'static str> {
    Some(match tag.trim().to_ascii_uppercase().as_str() {
        "VOIDT1" => "lith",
        "VOIDT2" => "meso",
        "VOIDT3" => "neo",
        "VOIDT4" => "axi",
        "VOIDT5" => "requiem",
        "VOIDT6" => "omnia",
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum PlannerPhase {
    /// The window is up; the era is still being read.
    Detecting,
    Ready,
    /// The picker closed; the window should clear.
    Closed,
}

/// `relic-planner` event. `era` is null when none could be read: the overlay then lists every era.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RelicPlannerEvent {
    pub token: u32,
    pub phase: PlannerPhase,
    pub era: Option<String>,
    pub confidence: f64,
    /// No capture or no OCR engine: the era could not even be tried.
    pub ocr_unavailable: bool,
}

/// What decides whether and how a scan runs. Pure: times are passed in.
#[derive(Default)]
pub struct Planner {
    token: u32,
    timer: u32,
    last_eelog_ms: f64,
    dismissed_until_ms: f64,
    log_tier: Option<&'static str>,
    screen_tier: Option<(&'static str, f64)>,
    session_open: bool,
    last_open_ms: f64,
    last_close_ms: f64,
}

impl Planner {
    /// A new scan's token, or `None` when an EE.log trigger is too soon or the player just closed it.
    pub fn trigger(&mut self, from_log: bool, now: f64) -> Option<u32> {
        if from_log {
            if self.dismissed_until_ms > now || now - self.last_eelog_ms < MIN_EELOG_GAP_MS {
                return None;
            }
            self.last_eelog_ms = now;
            self.session_open = true;
            self.last_open_ms = now;
        }
        self.token = self.token.wrapping_add(1);
        Some(self.token)
    }

    pub fn is_current(&self, token: u32) -> bool {
        self.token == token
    }

    /// The era the log tag or a recent screen read already gives, without touching the screen.
    pub fn known_era(&self, now: f64) -> Option<&'static str> {
        self.log_tier.or_else(|| {
            self.screen_tier
                .filter(|(_, at)| now - at < TIER_TTL_MS)
                .map(|(e, _)| e)
        })
    }

    pub fn has_log_tier(&self) -> bool {
        self.log_tier.is_some()
    }

    pub fn set_tag(&mut self, tag: &str) {
        self.log_tier = era_of_tag(tag);
    }

    pub fn remember_screen_era(&mut self, era: &'static str, now: f64) {
        self.screen_tier = Some((era, now));
    }

    /// The picker closed: drop the scan, the screen tier (the log tag outlives the picker, since it
    /// fires once per mission load) and mute EE.log reopens for a moment.
    pub fn close(&mut self, now: f64) {
        self.token = self.token.wrapping_add(1);
        self.screen_tier = None;
        self.session_open = false;
        self.dismissed_until_ms = now + REOPEN_SUPPRESS_MS;
    }

    /// An InitMapping line: true when it is a real exit rather than the picker's own entry echo.
    pub fn is_exit(&mut self, now: f64) -> bool {
        if !self.session_open
            || now - self.last_close_ms < CLOSE_COOLDOWN_MS
            || now - self.last_open_ms < ENTRY_WINDOW_MS
        {
            return false;
        }
        self.last_close_ms = now;
        true
    }

    fn next_timer(&mut self) -> u32 {
        self.timer = self.timer.wrapping_add(1);
        self.timer
    }
}

/// The era a log tag gave, unless the filter tab on screen says otherwise: an omnia pick screen
/// opens Lith-first and a tag can be stale. Requiem relics open only in a Requiem fissure, so no
/// other label can sit on a screen that offers them.
fn tag_or_label(tag: &'static str, label: &EraDetection) -> (String, f64) {
    match label.era.as_deref() {
        Some(e)
            if label.confidence >= CONFIDENT
                && label.candidate.as_deref() == Some(FILTER_LABEL)
                && e != tag
                && e != "requiem" =>
        {
            (e.into(), label.confidence)
        }
        _ => (tag.into(), 1.0),
    }
}

/// Era text -> the static name the planner keeps; unknown reads are dropped.
fn static_era(era: &str) -> Option<&'static str> {
    ["lith", "meso", "neo", "axi", "requiem", "omnia"]
        .into_iter()
        .find(|e| *e == era)
}

/// Managed state shared by the tracker and the commands.
#[derive(Default)]
pub struct PlannerState(Mutex<Planner>);

fn with_planner<R>(app: &AppHandle, f: impl FnOnce(&mut Planner) -> R) -> R {
    let state: State<PlannerState> = app.state();
    let mut guard = state.0.lock().unwrap();
    f(&mut guard)
}

/// Captures the game and reads the era; Err when there is no capture or no OCR engine.
fn read_era(label_only: bool, budget: Duration) -> Result<EraDetection, String> {
    #[cfg(windows)]
    {
        use crate::platform::windows::{ocr, reward::WindowsReader};
        use crate::services::capture::{CaptureSource, GameWindow};
        if !ocr::available() {
            return Err("Windows OCR has no English language pack".into());
        }
        let frame = GameWindow.capture()?;
        Ok(era::detect(
            &WindowsReader,
            &frame.image,
            label_only,
            budget,
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = (label_only, budget);
        Err("windows only".into())
    }
}

async fn read_era_off_thread(label_only: bool, budget: Duration) -> Result<EraDetection, String> {
    tauri::async_runtime::spawn_blocking(move || read_era(label_only, budget))
        .await
        .map_err(|e| e.to_string())?
}

fn emit(app: &AppHandle, ev: RelicPlannerEvent) {
    let _ = app.emit(EVENT, ev);
}

/// Hides the window after `delay` unless a later scan or close rescheduled it.
fn hide_later(app: &AppHandle, delay: Duration) {
    let timer = with_planner(app, Planner::next_timer);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        if with_planner(&app, |p| p.timer) == timer {
            let _ = overlay_hide(app.clone(), OverlayKind::Reward).await;
        }
    });
}

async fn scan(app: AppHandle, token: u32) {
    let ev = |phase, era: Option<String>, confidence, ocr_unavailable| RelicPlannerEvent {
        token,
        phase,
        era,
        confidence,
        ocr_unavailable,
    };
    let _ = overlay_show(app.clone(), OverlayKind::Reward).await;
    emit(&app, ev(PlannerPhase::Detecting, None, 0.0, false));
    hide_later(&app, HIDE_DETECTING);

    let (known, has_tag) = with_planner(&app, |p| (p.known_era(now_ms_f64()), p.has_log_tier()));
    if let Some(era) = known {
        // Rows right away; a refinement may still change the era.
        emit(&app, ev(PlannerPhase::Ready, Some(era.into()), 1.0, false));
    }
    tokio::time::sleep(START_DELAY).await;

    let started = std::time::Instant::now();
    let mut unavailable = false;
    let (era, confidence) = match known {
        Some(tag) if has_tag => match read_era_off_thread(true, LABEL_TIMEOUT).await {
            Ok(label) => tag_or_label(tag, &label),
            Err(_) => (tag.into(), 1.0),
        },
        Some(cached) => (cached.into(), 1.0),
        None => {
            let mut read = read_era_off_thread(false, ERA_TIMEOUT).await;
            let left = ERA_TOTAL.saturating_sub(started.elapsed() + RETRY_DELAY);
            if matches!(&read, Ok(d) if d.era.is_none()) && left >= MIN_RETRY {
                tokio::time::sleep(RETRY_DELAY).await;
                read = read_era_off_thread(false, ERA_TIMEOUT.min(left)).await;
            }
            match read {
                Ok(d) if d.confidence >= CONFIDENT && d.era.is_some() => {
                    let era = d.era.unwrap_or_default();
                    if let (Some(e), true) = (
                        static_era(&era),
                        d.candidate.as_deref() == Some(FILTER_LABEL),
                    ) {
                        with_planner(&app, |p| p.remember_screen_era(e, now_ms_f64()));
                    }
                    (era, d.confidence)
                }
                Ok(d) => (String::new(), d.confidence),
                Err(e) => {
                    tracing::warn!(error = %e, "relic planner: era unreadable");
                    unavailable = true;
                    (String::new(), 0.0)
                }
            }
        }
    };
    if !with_planner(&app, |p| p.is_current(token)) {
        return;
    }
    let era = (!era.is_empty()).then_some(era);
    let delay = if unavailable {
        HIDE_FAILURE
    } else {
        HIDE_SUCCESS
    };
    emit(&app, ev(PlannerPhase::Ready, era, confidence, unavailable));
    hide_later(&app, delay);
}

fn start(app: &AppHandle, from_log: bool) -> Option<u32> {
    let token = with_planner(app, |p| p.trigger(from_log, now_ms_f64()))?;
    tauri::async_runtime::spawn(scan(app.clone(), token));
    Some(token)
}

fn close(app: &AppHandle) {
    let token = with_planner(app, |p| {
        p.close(now_ms_f64());
        p.token
    });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        emit(
            &app,
            RelicPlannerEvent {
                token,
                phase: PlannerPhase::Closed,
                era: None,
                confidence: 0.0,
                ocr_unavailable: false,
            },
        );
        let _ = overlay_hide(app, OverlayKind::Reward).await;
    });
}

/// Opens the planner as the hotkey or a "test" button would. Returns false when suppressed.
#[tauri::command]
#[specta::specta]
pub async fn relic_planner_trigger(app: AppHandle) -> Result<bool, String> {
    Ok(start(&app, false).is_some())
}

/// The player closed the planner: mutes EE.log reopens for 3 s and forgets the screen's era.
#[tauri::command]
#[specta::specta]
pub async fn relic_planner_close(app: AppHandle) -> Result<(), String> {
    close(&app);
    Ok(())
}

pub struct PlannerTracker(AppHandle);

pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    app.manage(PlannerState::default());
    Box::new(PlannerTracker(app.clone()))
}

impl LineTracker for PlannerTracker {
    fn on_line(&mut self, line: &Line) {
        // File lines echo the live stream; reading both would reopen the picker.
        if line.source == Source::File && line.realtime_active {
            return;
        }
        let app = &self.0;
        if let Some(tag) = parse_active_mission_tag(line.text) {
            with_planner(app, |p| p.set_tag(&tag));
        } else if is_mission_end_line(line.text) {
            with_planner(app, |p| p.set_tag("EndOfMission"));
        }
        if is_relic_picker_open_line(line.text) {
            start(app, true);
        } else if is_relic_picker_close_line(line.text)
            && with_planner(app, |p| p.is_exit(line.now_ms))
        {
            close(app);
        }
    }

    fn on_reset(&mut self) {
        with_planner(&self.0, |p| *p = Planner::default());
    }
}

#[cfg(test)]
mod tests;
