//! Reward overlay backend: a trigger (the reward hotkey or EE.log's "Got rewards") captures the
//! game, reads the cards, joins prices/ducats/ownership and pushes a `RewardOverlayView`; the
//! window closes by itself. Policy (retries, timings, reopen suppression) is `reward_scan::flow`.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::join_all;
use tauri::{AppHandle, Emitter, Manager};
use tokio::time::{Instant, sleep, timeout};

use super::{OverlayKind, with};
use crate::db::Db;
use crate::services::bundled_data::BundledData;
use crate::services::capture::CaptureSource;
use crate::services::eelog::Source;
use crate::services::eelog_parsers::lines::{is_reward_screen_close_line, is_reward_trigger_line};
use crate::services::inventory::Inventory;
use crate::services::item_db::{ItemDb, ItemDbState};
use crate::services::mastery::{self, MasteryStatus};
use crate::services::relics::{ItemLookup, RelicRewardItem, RelicState};
use crate::services::reward_scan::enrich::{MasteredSet, Ownership, enrich};
use crate::services::reward_scan::flow::{
    DETECTING_MAX, FAILURE_HIDE, Outcome, ScanDebug, Session, Trigger, run_attempts, scan_once,
};
use crate::services::reward_scan::names::{PoolItem, RewardPool};
use crate::services::reward_scan::payload::{
    EVENT, RewardFailure, RewardOverlayPayload, RewardOverlayPhase, RewardOverlaySlot,
    RewardOverlayView,
};
use crate::services::reward_scan::{REFERENCE_UI_SCALE, RewardReader};
use crate::services::trackers::{Line, LineTracker};
use crate::services::wfm::prices::Prices;
use crate::services::wfm::scheduler::Priority;
use crate::services::wfm::session::Session as WfmSession;

/// A log line older than this is the tail of an earlier game session, not a reward screen.
const MAX_LINE_AGE_MS: f64 = 10_000.0;
/// Prices not cached by then are left out rather than delaying the card.
/// Settle time before the warm-up reads managed state that other setup steps fill in.
const WARM_UP_AFTER: Duration = Duration::from_secs(8);
const PRICE_WAIT: Duration = Duration::from_millis(2_500);

#[derive(Default)]
pub struct RewardOverlay {
    session: Mutex<Session>,
    pool: RewardPool,
    debug: Mutex<Option<ScanDebug>>,
    /// Bumps to cancel the pending auto-close (a newer timer, a close, or interaction).
    hide_gen: AtomicU64,
}

pub fn setup(app: &tauri::App) {
    app.manage(RewardOverlay::default());
    // The ONNX model, the reward pool and the readers build on first use (a second or more);
    // do it once the app is up so the first reward screen of a session does not pay for it.
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        sleep(WARM_UP_AFTER).await;
        let started = Instant::now();
        let _ = tauri::async_runtime::spawn_blocking(move || prepare(&handle)).await;
        tracing::debug!(elapsed = ?started.elapsed(), "reward scan: warmed up");
    });
}

fn publish(
    app: &AppHandle,
    token: u64,
    phase: RewardOverlayPhase,
    presentation: Option<RewardOverlayPayload>,
    failure: Option<RewardFailure>,
    auto_close: Duration,
) {
    let _ = app.emit(
        EVENT,
        RewardOverlayView {
            phase,
            presentation,
            failure,
            auto_close_ms: auto_close.as_millis() as u32,
            token,
        },
    );
}

/// Closes the card `after` from now unless something newer cancels it. Call from async code only.
fn schedule_hide(app: &AppHandle, after: Duration) {
    let Some(st) = app.try_state::<RewardOverlay>() else {
        return;
    };
    let generation = st.hide_gen.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        sleep(after).await;
        let Some(st) = app.try_state::<RewardOverlay>() else {
            return;
        };
        if st.hide_gen.load(Ordering::SeqCst) == generation {
            close(&app);
        }
    });
}

fn close(app: &AppHandle) {
    if let Some(st) = app.try_state::<RewardOverlay>() {
        st.hide_gen.fetch_add(1, Ordering::SeqCst);
        st.session.lock().unwrap().closed(Instant::now());
    }
    let _ = with(app, |m| {
        m.set_interactive(OverlayKind::Reward, false);
        m.hide(OverlayKind::Reward);
    });
}

fn show(app: &AppHandle) {
    let _ = with(app, |m| {
        if !m.is_shown(OverlayKind::Reward) {
            m.show(OverlayKind::Reward);
        }
    });
}

/// Starts a round. EE.log triggers wait out the draw delay and show the card only with a result.
pub fn trigger(app: &AppHandle, t: Trigger, staleness: Duration) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { run(app, t, staleness).await });
}

/// The reward hotkey: closes a showing card, otherwise scans.
pub fn on_hotkey(app: &AppHandle) {
    let shown = with(app, |m| m.is_shown(OverlayKind::Reward)).unwrap_or(false);
    if shown {
        close(app);
    } else {
        trigger(app, Trigger::Hotkey, Duration::ZERO);
    }
}

struct Prep {
    readers: Arc<Vec<Reader>>,
    pool: Arc<Vec<PoolItem>>,
    rewards: Arc<HashMap<String, RelicRewardItem>>,
    db: Arc<ItemDb>,
    own: Ownership,
    mastered: Option<MasteredSet>,
}

/// Everything a round reads that does not depend on the frame; blocking (builds on first use).
fn prepare(app: &AppHandle) -> Result<Prep, String> {
    let st = app.state::<RewardOverlay>();
    let items = app.state::<ItemDbState>();
    let (db, locale) = items.snapshot();
    let relics = app
        .state::<RelicState>()
        .scan_database(&BundledData::from_app(app)?, &items);
    let (pool, rewards) = st.pool.built(
        &relics,
        &ItemLookup {
            db: &db,
            locale: &locale,
        },
    );
    let inventory = app.state::<Arc<Inventory>>().current();
    let own = Ownership::from_inventory(inventory.as_deref(), &db);
    // From the inventory, else from the public profile; None when neither is there.
    let mastered = mastery::current_progress(app).map(|p| {
        MasteredSet::new(p.items.into_iter().map(|i| {
            let done = i.status == MasteryStatus::Mastered;
            (i.unique_name, i.name, done)
        }))
    });
    Ok(Prep {
        readers: Arc::new(readers(app)),
        pool,
        rewards,
        db,
        own,
        mastered,
    })
}

#[cfg(windows)]
fn live() -> Result<Arc<dyn CaptureSource>, String> {
    Ok(Arc::new(crate::services::capture::GameWindow))
}

#[cfg(not(windows))]
fn live() -> Result<Arc<dyn CaptureSource>, String> {
    Err("windows only".into())
}

type Reader = Arc<dyn RewardReader + Send + Sync>;

/// Readers in priority order. The bundled Paddle model reads first (no Windows language pack
/// needed); Windows OCR plus Paddle again is the second vote for slots Paddle left unread.
/// Without the ONNX runtime it is Windows OCR alone. Loading takes a second the first time.
#[cfg(windows)]
fn readers(app: &AppHandle) -> Vec<Reader> {
    use crate::platform::windows::reward::WindowsReader;
    use crate::services::ocr_onnx::{paddle, resource_root};
    use crate::services::reward_scan::onnx_reader::{NoBands, OnnxReader};
    match resource_root(app).and_then(|root| paddle(&root)) {
        Ok(p) => vec![
            Arc::new(OnnxReader {
                inner: NoBands,
                paddle: p,
            }),
            Arc::new(OnnxReader {
                inner: WindowsReader,
                paddle: p,
            }),
        ],
        Err(e) => {
            tracing::info!("reward scan: ONNX reader unavailable ({e}); Windows OCR only");
            vec![Arc::new(WindowsReader)]
        }
    }
}

#[cfg(not(windows))]
fn readers(_: &AppHandle) -> Vec<Reader> {
    vec![Arc::new(crate::services::reward_scan::onnx_reader::NoBands)]
}

#[cfg(windows)]
fn game_running() -> bool {
    crate::platform::windows::game_window::find().is_some()
}

#[cfg(not(windows))]
fn game_running() -> bool {
    false
}

async fn prices_for(app: &AppHandle, slugs: Vec<String>) -> HashMap<String, u32> {
    let (session, db, prices) = (
        app.state::<WfmSession>(),
        app.state::<Db>(),
        app.state::<Prices>(),
    );
    let fetch = slugs.into_iter().map(|slug| {
        let (session, db, prices) = (&session, &db, &prices);
        async move {
            let r = timeout(
                PRICE_WAIT,
                prices.price(session.client(), db, &slug, Priority::Interactive),
            )
            .await;
            Some((slug, r.ok()?.ok()?.price?))
        }
    });
    join_all(fetch).await.into_iter().flatten().collect()
}

/// Cards matched and enriched; prices come after, so the card is not held up by the network.
struct Round {
    payload: RewardOverlayPayload,
    /// Per card: its WFM slug and its set's.
    slugs: Vec<(Option<String>, Option<String>)>,
}

async fn read_round(app: &AppHandle) -> Result<Round, RewardFailure> {
    let prep_app = app.clone();
    let prep = tauri::async_runtime::spawn_blocking(move || prepare(&prep_app));
    let src = live().map_err(|e| {
        tracing::warn!("reward scan: {e}");
        RewardFailure::CaptureUnavailable
    })?;
    if let Some(st) = app.try_state::<RewardOverlay>() {
        let wait = st.session.lock().unwrap().scan_delay(Instant::now());
        sleep(wait).await;
    }
    let prep = prep
        .await
        .map_err(|e| e.to_string())
        .and_then(|p| p)
        .map_err(|e| {
            tracing::warn!("reward scan: item data not ready: {e}");
            RewardFailure::NothingRead
        })?;

    let outcome = run_attempts(|| {
        let (src, readers, pool, app) = (
            src.clone(),
            prep.readers.clone(),
            prep.pool.clone(),
            app.clone(),
        );
        async move {
            let read = tauri::async_runtime::spawn_blocking(move || {
                let readers: Vec<&dyn RewardReader> =
                    readers.iter().map(|r| &**r as &dyn RewardReader).collect();
                scan_once(&*src, &readers, &pool, REFERENCE_UI_SCALE)
            })
            .await
            .map_err(|e| e.to_string())??;
            Ok::<_, String>(read.map(|(attempt, debug)| {
                if let Some(st) = app.try_state::<RewardOverlay>() {
                    *st.debug.lock().unwrap() = Some(debug);
                }
                attempt
            }))
        }
    })
    .await;
    let names = match outcome {
        Ok(Outcome::Read(names)) => names,
        Ok(Outcome::NotRewardScreen) => return Err(RewardFailure::NotRewardScreen),
        Ok(Outcome::Partial { read, cards }) => {
            tracing::warn!("reward scan: gave up at {read}/{cards} cards");
            return Err(RewardFailure::NothingRead);
        }
        Ok(Outcome::Nothing) => return Err(RewardFailure::NothingRead),
        Err(e) => {
            tracing::warn!("reward scan: capture failed: {e}");
            return Err(RewardFailure::CaptureUnavailable);
        }
    };

    let cards: Vec<_> = names
        .iter()
        .flatten()
        .filter_map(|n| prep.rewards.get(n))
        .collect();
    if cards.len() != names.len() {
        return Err(RewardFailure::NothingRead);
    }
    let items: Vec<_> = cards
        .iter()
        .map(|r| enrich(r, &prep.db, &prep.own, prep.mastered.as_ref()))
        .collect();
    let slugs = cards
        .iter()
        .zip(&items)
        .map(|(r, i)| (r.url_name.clone(), i.set_url_name.clone()))
        .collect();
    let mut slots: Vec<Option<RewardOverlaySlot>> = items
        .into_iter()
        .map(|item| {
            Some(RewardOverlaySlot {
                item,
                price: None,
                set_price: None,
            })
        })
        .collect();
    let count = slots.len() as u8;
    slots.resize(4, None);
    Ok(Round {
        payload: RewardOverlayPayload { count, slots },
        slugs,
    })
}

async fn run(app: AppHandle, t: Trigger, staleness: Duration) {
    let started = Instant::now();
    let Some(st) = app.try_state::<RewardOverlay>() else {
        return;
    };
    let Some(token) = st
        .session
        .lock()
        .unwrap()
        .begin(t, staleness, Instant::now())
    else {
        tracing::info!("reward scan: {t:?} trigger skipped (scan running or card just closed)");
        return;
    };
    st.hide_gen.fetch_add(1, Ordering::SeqCst);
    publish(
        &app,
        token,
        RewardOverlayPhase::Scanning,
        None,
        None,
        Duration::ZERO,
    );
    if t == Trigger::Hotkey {
        show(&app);
        // A scan that never answers must not leave "detecting" up for good.
        schedule_hide(&app, DETECTING_MAX);
    }

    let result = if t == Trigger::EeLog && !game_running() {
        Err(RewardFailure::GameNotOpen)
    } else {
        read_round(&app).await
    };

    let now = Instant::now();
    // A scope, not `drop`: the guard must not look held across the awaits below.
    let (phase, presentation, failure, hide, slugs) = {
        let mut session = st.session.lock().unwrap();
        session.finish();
        let result = match result {
            Err(RewardFailure::NothingRead)
                if t == Trigger::Hotkey && session.reward_screen_gone(now) =>
            {
                Err(RewardFailure::ScreenClosed)
            }
            r => r,
        };
        match result {
            Ok(r) => (
                RewardOverlayPhase::Ready,
                Some(r.payload),
                None,
                session.success_hide(now, false),
                r.slugs,
            ),
            Err(f) => (
                RewardOverlayPhase::Failed,
                None,
                Some(f),
                FAILURE_HIDE,
                vec![],
            ),
        }
    };
    let cards = presentation.as_ref().map_or(0, |p| p.count);
    match failure {
        None => tracing::info!(
            "reward scan: {t:?} -> {cards} card(s) read in {:?} (+{staleness:?} log lag)",
            started.elapsed()
        ),
        Some(f) => tracing::info!("reward scan: {t:?} -> failed: {f:?}"),
    }
    if crate::services::scan_captures::enabled() {
        let debug = st.debug.lock().unwrap().clone().unwrap_or_default();
        let text = format!(
            "trigger: {t:?}\nresult: {}\ncards: {}\n{}",
            failure.map_or_else(|| format!("{cards} card(s) read"), |f| format!("{f:?}")),
            debug.cards,
            debug
                .slots
                .iter()
                .enumerate()
                .map(|(i, c)| format!("slot {i}: {}", c.join(" | ")))
                .collect::<Vec<_>>()
                .join("\n")
        );
        tauri::async_runtime::spawn_blocking(move || {
            crate::services::scan_captures::save_kept("reward", &text)
        });
    }
    publish(&app, token, phase, presentation.clone(), failure, hide);
    // A log trigger on a plain pause (or with the game closed) never opens a card.
    if t == Trigger::Hotkey
        || !matches!(
            failure,
            Some(RewardFailure::NotRewardScreen | RewardFailure::GameNotOpen)
        )
    {
        show(&app);
        schedule_hide(&app, hide);
    }
    if let Some(payload) = presentation {
        fill_prices(&app, token, payload, slugs, hide).await;
    }
}

/// Republishes the shown card with prices once they are in; skipped when a newer round took over.
async fn fill_prices(
    app: &AppHandle,
    token: u64,
    mut payload: RewardOverlayPayload,
    slugs: Vec<(Option<String>, Option<String>)>,
    hide: Duration,
) {
    let started = Instant::now();
    let wanted: std::collections::BTreeSet<String> = slugs
        .iter()
        .flat_map(|(a, b)| [a, b])
        .flatten()
        .cloned()
        .collect();
    let prices = prices_for(app, wanted.into_iter().collect()).await;
    tracing::debug!(elapsed = ?started.elapsed(), found = prices.len(), "reward scan: prices");
    let current = app
        .try_state::<RewardOverlay>()
        .is_some_and(|s| s.session.lock().unwrap().token() == token);
    if prices.is_empty() || !current {
        return;
    }
    for (slot, (own, set)) in payload.slots.iter_mut().zip(&slugs) {
        if let Some(slot) = slot {
            slot.price = own.as_ref().and_then(|s| prices.get(s)).copied();
            slot.set_price = set.as_ref().and_then(|s| prices.get(s)).copied();
        }
    }
    publish(
        app,
        token,
        RewardOverlayPhase::Ready,
        Some(payload),
        None,
        hide.saturating_sub(started.elapsed()),
    );
}

fn screen_closed(app: &AppHandle, staleness: Duration) {
    let Some(st) = app.try_state::<RewardOverlay>() else {
        return;
    };
    let delay = st
        .session
        .lock()
        .unwrap()
        .on_screen_closed(staleness, Instant::now());
    if let Some(delay) = delay {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { schedule_hide(&app, delay) });
    }
}

pub struct RewardTracker(AppHandle);

pub fn tracker(app: &AppHandle) -> Box<dyn LineTracker> {
    Box::new(RewardTracker(app.clone()))
}

impl LineTracker for RewardTracker {
    fn on_line(&mut self, line: &Line) {
        let trigger_line = is_reward_trigger_line(line.text);
        if !trigger_line && !is_reward_screen_close_line(line.text) {
            return;
        }
        // The live stream already delivered this line; the file copy arrives seconds later.
        if line.source == Source::File && line.realtime_active {
            return;
        }
        if line.staleness_ms > MAX_LINE_AGE_MS {
            tracing::info!(
                "reward scan: EE.log line ignored, {:.0} ms old",
                line.staleness_ms
            );
            return;
        }
        if trigger_line {
            tracing::info!("reward scan: EE.log trigger ({:?})", line.source);
        }
        let age = Duration::from_millis(line.staleness_ms.max(0.0) as u64);
        if trigger_line {
            trigger(&self.0, Trigger::EeLog, age);
        } else {
            screen_closed(&self.0, age);
        }
    }

    fn on_reset(&mut self) {}
}

/// Closes the card. A `token` of an older round is ignored, so a late click cannot close the next.
#[tauri::command]
#[specta::specta]
pub async fn reward_overlay_close(app: AppHandle, token: Option<f64>) -> Result<(), String> {
    let current = app
        .try_state::<RewardOverlay>()
        .map(|s| s.session.lock().unwrap().token());
    if token.is_none_or(|t| Some(t as u64) == current) {
        close(&app);
    }
    Ok(())
}

/// What the last frame read (card count and OCR candidates per slot), for diagnosing a miss.
#[tauri::command]
#[specta::specta]
pub async fn reward_scan_debug(app: AppHandle) -> Result<Option<ScanDebug>, String> {
    Ok(app
        .try_state::<RewardOverlay>()
        .and_then(|s| s.debug.lock().unwrap().clone()))
}
