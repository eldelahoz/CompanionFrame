//! Riven overlay: EE.log cycle screen, hotkey or command -> capture the game -> read the card(s)
//! (`riven_scan`) -> grade (`rivens`) -> show them, then fill in the market analysis (`riven_analyzer`)
//! from similar warframe.market listings. On the compare screen both cards are read: the New roll
//! beside the Current one, which falls back to the card remembered from before the roll. The window
//! is reused between scans; a scan that finishes after a newer one began (or after a close) is dropped.
use crate::services::sync::LockExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use image::RgbaImage;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use super::riven_analyzer::{
    CardStat, RivenAnalysis, RivenVerdict, analyze, compare_grades, verdict,
};
use super::{OverlayKind, with};
use rusqlite::Connection;

use crate::db::Db;
use crate::services::capture::CaptureSource;
use crate::services::eelog_parsers::riven::RivenEvent;
use crate::services::ocr_onnx;
use crate::services::reward_scan::fold::fold_accents;
use crate::services::riven_scan::footer::Footer;
use crate::services::riven_scan::locale::CardLabels;
use crate::services::riven_scan::session::{RollTally, ScanGeneration, kuva_spent};
use crate::services::riven_scan::text::RivenStat;
use crate::services::riven_scan::weapon_label::{
    WeaponIndex, WeaponLabelMatch, WeaponSource, should_apply_label_weapon,
};
use crate::services::riven_scan::{self, CHAT_CARD, CropRect, ROLL_CARD, SINGLE_CARD, ScanError};
use crate::services::rivens::best_attributes::{GoodRolls, RivenBestAttributes};
use crate::services::rivens::data::RivenData;
use crate::services::rivens::grading::{self, RivenGradeResult, RivenStatInput};
use crate::services::rivens::memory::{self, RememberedCard};
use crate::services::rivens::vocab::tag_to_wfm_url_name;
use crate::services::wfm::errors::WfmError;
use crate::services::wfm::riven_search::{RivenSearchOptions, SearchCache, search_similar};
use crate::services::wfm::session::Session;

/// `riven-overlay` carries a [`RivenOverlayView`]; `Closed` means hide whatever is drawn.
pub const EVENT: &str = "riven-overlay";
const AUTO_CLOSE: Duration = Duration::from_secs(60);
/// Listings pulled per search; the analyzer weighs them all and shows five.
const POOL_LIMIT: u32 = 200;
/// The search is loosened until the pool holds this many listings.
const ENOUGH_POOL: usize = 5;

static GENERATION: ScanGeneration = ScanGeneration::new();

/// The card on screen and the roll session around it; what a fits-in label read is compared with.
#[derive(Clone)]
struct Current {
    weapon: Option<String>,
    source: WeaponSource,
    /// As read; corrected against the weapon whenever a view is built.
    stats: Vec<RivenStat>,
    unsure: bool,
    footer: Footer,
}

#[derive(Default)]
struct SessionState {
    current: Option<Current>,
    /// The last card read on its own (cycle screen, after a pick): the compare screen's Current,
    /// kept because the dimmed card reads worse. Cleared when a single read fails or the screen closes.
    kept: Option<Current>,
    /// Some while a roll session runs.
    tally: Option<RollTally>,
    /// True while the compare screen is up: the rescan key then rereads only the new card.
    compare: bool,
}

static SESSION: Mutex<SessionState> = Mutex::new(SessionState {
    current: None,
    kept: None,
    tally: None,
    compare: false,
});

fn session() -> std::sync::MutexGuard<'static, SessionState> {
    SESSION.lock_any()
}

/// Rolls done and Kuva spent in the running session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenRollTally {
    pub kuva_per_roll: u32,
    pub roll_count: u32,
    #[specta(type = f64)]
    pub total_kuva_spent: u64,
}

impl From<RollTally> for RivenRollTally {
    fn from(t: RollTally) -> Self {
        Self {
            kuva_per_roll: t.kuva_per_roll,
            roll_count: t.roll_count,
            total_kuva_spent: t.total_kuva_spent,
        }
    }
}

/// Where the card sits on screen; the crop rect differs per screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RivenCardLayout {
    /// Inventory, or the first look after a roll: the centred card.
    #[default]
    Single,
    Chat,
    /// The compare-and-pick screen after a reroll.
    Roll,
}

impl RivenCardLayout {
    fn rect(self) -> CropRect {
        match self {
            Self::Single => SINGLE_CARD,
            Self::Chat => CHAT_CARD,
            Self::Roll => ROLL_CARD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RivenOverlayStatus {
    Scanning,
    Ready,
    /// The ONNX runtime or a model is missing.
    Unavailable,
    /// Nothing readable (no card on screen, capture failed).
    Failed,
    Closed,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenOverlayStat {
    pub name: String,
    pub positive: bool,
    /// The sign the card prints, when it differs from `positive` (a recoil curse prints "+").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub printed_positive: Option<bool>,
    pub value: Option<f64>,
    pub multiplier: bool,
    /// None when the weapon is unknown, so the stat cannot be graded.
    pub grade: Option<String>,
    /// The value a roll of 0 and of 1 would show for this weapon, disposition and stat count.
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// 0-100: how good the roll is within its range (a smaller curse is better).
    pub quality: Option<u32>,
}

/// One riven: the graded card and, once the market answers, its analysis.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenCardView {
    pub weapon: Option<String>,
    pub stats: Vec<RivenOverlayStat>,
    pub overall_grade: Option<String>,
    pub attribute_grade: Option<String>,
    pub best: Option<RivenBestAttributes>,
    /// The read may have lost a stat or a line was hard to read.
    pub unsure: bool,
    /// Mastery rank and cycle count read off the card footer.
    pub mr: Option<u32>,
    pub card_rolls: Option<u32>,
    /// Kuva the card's cycles cost, from the cycle cost table.
    #[specta(type = Option<f64>)]
    pub card_kuva_spent: Option<u64>,
    /// How much of the card the similar listings match; None until they arrive.
    pub similar_scope: Option<SimilarScope>,
    /// None until the market answers; the card is shown without waiting.
    pub analysis: Option<RivenAnalysis>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RivenViewLayout {
    /// One card: inspect, or after a pick.
    Single,
    /// The compare screen: Current beside the New roll.
    Compare,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenOverlayView {
    pub status: RivenOverlayStatus,
    #[specta(type = f64)]
    pub token: u64,
    pub layout: RivenViewLayout,
    /// The card on screen; the New roll when comparing. None while a compare's new card is unread.
    pub card: Option<RivenCardView>,
    /// Compare only: the card the player has now.
    pub current: Option<RivenCardView>,
    /// Compare only, once both cards have an estimate.
    pub verdict: Option<RivenVerdict>,
    /// Kuva the next cycle costs, from the card's roll count.
    pub next_kuva: Option<u32>,
    pub message: Option<String>,
    pub auto_close_ms: u32,
    /// Set while a roll session runs.
    pub session: Option<RivenRollTally>,
}

impl RivenOverlayView {
    fn bare(status: RivenOverlayStatus, token: u64) -> Self {
        Self {
            status,
            token,
            layout: RivenViewLayout::Single,
            card: None,
            current: None,
            verdict: None,
            next_kuva: None,
            message: None,
            auto_close_ms: AUTO_CLOSE.as_millis() as u32,
            session: None,
        }
    }

    fn with_message(status: RivenOverlayStatus, token: u64, message: impl Into<String>) -> Self {
        Self {
            message: Some(message.into()),
            ..Self::bare(status, token)
        }
    }

    fn single(token: u64, card: RivenCardView) -> Self {
        Self {
            next_kuva: next_kuva(card.card_rolls),
            card: Some(card),
            ..Self::bare(RivenOverlayStatus::Ready, token)
        }
    }
}

/// Kuva the cycle after the card's `rolls` cycles costs.
fn next_kuva(rolls: Option<u32>) -> Option<u32> {
    rolls.map(|r| (kuva_spent(r + 1) - kuva_spent(r)) as u32)
}

/// English weapon names plus the game language's, which resolve to the English name.
fn weapon_index(data: &RivenData, labels: &CardLabels) -> WeaponIndex {
    WeaponIndex::new(data.all_weapon_names().iter().map(String::as_str))
        .with_aliases(labels.weapon_aliases())
}

/// The weapon a card names ("Rubico Crita-ata" -> Rubico); only the title line counts.
fn weapon_of(data: &RivenData, labels: &CardLabels, text: &str) -> Option<String> {
    find_weapon(&weapon_index(data, labels), text)
}

fn find_weapon(index: &WeaponIndex, text: &str) -> Option<String> {
    let text = fold_accents(text);
    index
        .find_in_text(&text)
        .or_else(|| index.find_inside_lines(&text))
}

/// Renames OCR-garbled stat names to the sibling whose range fits the value; needs a known weapon.
pub fn correct_stats(
    data: &RivenData,
    weapon: Option<&str>,
    stats: &[RivenStat],
) -> Vec<RivenStat> {
    let Some(weapon) = weapon else {
        return stats.to_vec();
    };
    let inputs: Vec<RivenStatInput> = stats.iter().map(RivenStat::to_input).collect();
    let (fixed, n) = grading::correct_scanned_stats(data, weapon, &inputs);
    if n == 0 {
        return stats.to_vec();
    }
    stats
        .iter()
        .zip(fixed)
        .map(|(s, f)| RivenStat {
            name: f.name,
            positive: f.positive,
            ..s.clone()
        })
        .collect()
}

/// The card shows one decimal, two for an x-multiplier.
fn round_to_card(v: f64, multiplier: bool) -> f64 {
    let k = if multiplier { 100.0 } else { 10.0 };
    (v * k).round() / k
}

/// Grades and annotates a read card. Without a known weapon the stats are shown ungraded.
pub fn build_view(
    data: &RivenData,
    rolls: &GoodRolls,
    weapon: Option<String>,
    stats: &[RivenStat],
    unsure: bool,
    footer: Footer,
) -> RivenCardView {
    let stats = &correct_stats(data, weapon.as_deref(), stats);
    let inputs: Vec<_> = stats.iter().map(RivenStat::to_input).collect();
    let graded: Option<RivenGradeResult> = weapon
        .as_deref()
        .and_then(|w| grading::grade_riven(data, rolls, w, &inputs, None));
    RivenCardView {
        weapon: weapon.clone(),
        stats: stats
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let g = graded.as_ref().map(|g| &g.stats[i]);
                RivenOverlayStat {
                    name: s.name.clone(),
                    positive: s.positive,
                    printed_positive: s.display_positive,
                    value: s.value,
                    multiplier: s.multiplier,
                    grade: g.map(|g| g.grade.clone()),
                    min: g
                        .and_then(|g| g.min)
                        .map(|v| round_to_card(v, s.multiplier)),
                    max: g
                        .and_then(|g| g.max)
                        .map(|v| round_to_card(v, s.multiplier)),
                    quality: g.map(|g| {
                        let q = if s.positive {
                            g.roll_float
                        } else {
                            1.0 - g.roll_float
                        };
                        (q * 100.0).round() as u32
                    }),
                }
            })
            .collect(),
        overall_grade: graded.as_ref().map(|g| g.overall_grade.clone()),
        attribute_grade: graded.as_ref().map(|g| g.attribute_grade.clone()),
        best: weapon
            .as_deref()
            .and_then(|w| rolls.best_attributes(data, w, data.is_melee_weapon(w))),
        unsure,
        mr: footer.mr,
        card_rolls: footer.rolls,
        card_kuva_spent: footer.rolls.map(kuva_spent),
        similar_scope: None,
        analysis: None,
    }
}

/// warframe.market stat url names of the card's buffs and curse; stats it has no name for are left
/// out of the search.
pub fn search_stats(data: &RivenData, stats: &[RivenStat]) -> (Vec<String>, Vec<String>) {
    let (mut pos, mut neg) = (vec![], vec![]);
    for s in stats {
        let url = data.stat_name_to_tag(&s.name).and_then(tag_to_wfm_url_name);
        if let Some(u) = url {
            if s.positive { &mut pos } else { &mut neg }.push(u.to_owned());
        }
    }
    (pos, neg)
}

/// How far the similar search had to be loosened before listings turned up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SimilarScope {
    All,
    TopTwo,
    WeaponOnly,
}

/// Queries to try in order, each looser than the last: every stat, the first two buffs, the weapon.
/// A step that would repeat the one before it is left out.
pub fn search_steps(
    pos: &[String],
    neg: &[String],
) -> Vec<(SimilarScope, Vec<String>, Vec<String>)> {
    let mut steps = vec![(SimilarScope::All, pos.to_vec(), neg.to_vec())];
    if pos.len() > 2 || (pos.len() == 2 && !neg.is_empty()) {
        steps.push((SimilarScope::TopTwo, pos[..2].to_vec(), vec![]));
    }
    if !pos.is_empty() || !neg.is_empty() {
        steps.push((SimilarScope::WeaponOnly, vec![], vec![]));
    }
    steps
}

/// The hint "text too small" only when the final read lost something: a stat line the reader doubted
/// matters only while the card is still unclear (a stat without a value, or no weapon).
fn read_unsure(card: &riven_scan::CardRead, weapon_found: bool) -> bool {
    card.incomplete
        || (card.low_confidence && (!weapon_found || card.stats.iter().any(|s| s.value.is_none())))
}

/// Blocking: capture, read the card, grade. The outcome the overlay should draw first.
pub fn read_card(
    root: &Path,
    capture: &dyn CaptureSource,
    layout: RivenCardLayout,
    data: &RivenData,
    labels: &CardLabels,
    rolls: &GoodRolls,
    token: u64,
) -> (RivenOverlayView, Vec<RivenStat>) {
    match capture.capture() {
        Ok(f) => read_nth(root, &f.image, layout, 0, data, labels, rolls, token),
        Err(e) => (
            RivenOverlayView::with_message(RivenOverlayStatus::Failed, token, e),
            vec![],
        ),
    }
}

/// The card is still drawing when the open event arrives, so the first single read can fail.
const SINGLE_RETRIES: u32 = 4;
const SINGLE_RETRY_DELAY: Duration = Duration::from_millis(400);

/// Calls `read` once, then again after `delay` (up to `retries` times) while `unread` says the result
/// is not a good read and `alive` says the scan was not superseded. Returns the last result. Blocking.
fn retry_read<T>(
    retries: u32,
    delay: Duration,
    alive: impl Fn() -> bool,
    unread: impl Fn(&T) -> bool,
    mut read: impl FnMut() -> T,
) -> T {
    let mut out = read();
    for _ in 0..retries {
        if !unread(&out) || !alive() {
            break;
        }
        std::thread::sleep(delay);
        out = read();
    }
    out
}

/// [`read_card`] retried while the card is not read (failed, or no weapon). Blocking.
#[allow(clippy::too_many_arguments)]
fn read_card_retrying(
    root: &Path,
    capture: &dyn CaptureSource,
    layout: RivenCardLayout,
    data: &RivenData,
    labels: &CardLabels,
    rolls: &GoodRolls,
    token: u64,
    delay: Duration,
    alive: impl Fn() -> bool,
) -> (RivenOverlayView, Vec<RivenStat>) {
    retry_read(
        SINGLE_RETRIES,
        delay,
        alive,
        |(v, _): &(RivenOverlayView, Vec<RivenStat>)| {
            v.status != RivenOverlayStatus::Unavailable
                && (v.status != RivenOverlayStatus::Ready
                    || v.card.as_ref().is_none_or(|c| c.weapon.is_none()))
        },
        || read_card(root, capture, layout, data, labels, rolls, token),
    )
}

/// What a compare screen gave: the brightest card is the New roll, the dimmed one the Current.
pub struct CompareRead {
    pub new: (RivenOverlayView, Vec<RivenStat>),
    pub old: Option<(RivenCardView, Vec<RivenStat>)>,
}

/// Blocking: capture once and read both cards of the compare screen.
pub fn read_compare(
    root: &Path,
    capture: &dyn CaptureSource,
    data: &RivenData,
    labels: &CardLabels,
    rolls: &GoodRolls,
    token: u64,
    need_old: bool,
) -> CompareRead {
    let frame = match capture.capture() {
        Ok(f) => f.image,
        Err(e) => {
            return CompareRead {
                new: (
                    RivenOverlayView::with_message(RivenOverlayStatus::Failed, token, e),
                    vec![],
                ),
                old: None,
            };
        }
    };
    let layout = RivenCardLayout::Roll;
    let new = read_nth(root, &frame, layout, 0, data, labels, rolls, token);
    let old = (need_old && new.0.status == RivenOverlayStatus::Ready)
        .then(|| read_nth(root, &frame, layout, 1, data, labels, rolls, token))
        .and_then(|(v, stats)| v.card.map(|c| (c, stats)));
    CompareRead { new, old }
}

/// Right after a cycle the new card is still animating, so the first frames often fail to read.
const COMPARE_RETRIES: u32 = 3;
const COMPARE_RETRY_DELAY: Duration = Duration::from_millis(400);

/// [`read_compare`] again on a fresh frame (up to [`COMPARE_RETRIES`] times) while the new card is not
/// read, and until `alive` says the scan was superseded. Blocking.
#[allow(clippy::too_many_arguments)]
fn read_compare_retrying(
    root: &Path,
    capture: &dyn CaptureSource,
    data: &RivenData,
    labels: &CardLabels,
    rolls: &GoodRolls,
    token: u64,
    need_old: bool,
    delay: Duration,
    alive: impl Fn() -> bool,
) -> CompareRead {
    let mut read = read_compare(root, capture, data, labels, rolls, token, need_old);
    for _ in 0..COMPARE_RETRIES {
        let (view, _) = &read.new;
        // A card on the compare screen always has its cycle pill (it was just cycled), so a read
        // without it was taken while the pill was still fading in; the dimmed card too, if located.
        let no_pill = |c: &RivenCardView| c.card_rolls.is_none();
        let unread = view.status != RivenOverlayStatus::Unavailable
            && (view.status != RivenOverlayStatus::Ready
                || view
                    .card
                    .as_ref()
                    .is_none_or(|c| c.weapon.is_none() || no_pill(c))
                || read.old.as_ref().is_some_and(|(c, _)| no_pill(c)));
        if !unread || !alive() {
            break;
        }
        std::thread::sleep(delay);
        read = read_compare(root, capture, data, labels, rolls, token, need_old);
    }
    read
}

/// Reads the `nth` located card of a frame (0 = brightest). The second card has no layout rect to
/// fall back to: it is read where it was found or not at all.
#[allow(clippy::too_many_arguments)]
fn read_nth(
    root: &Path,
    frame: &RgbaImage,
    layout: RivenCardLayout,
    nth: usize,
    data: &RivenData,
    labels: &CardLabels,
    rolls: &GoodRolls,
    token: u64,
) -> (RivenOverlayView, Vec<RivenStat>) {
    use RivenOverlayStatus::{Failed, Unavailable};
    let located = riven_scan::locate_cards(frame);
    // The second card is the dimmed one, which the bright locator does not see.
    let target = match nth {
        0 => located.first().copied(),
        _ => riven_scan::locate_dimmed(frame, &located),
    };
    if nth > 0 && target.is_none() {
        return (RivenOverlayView::bare(Failed, token), vec![]);
    }
    let index = weapon_index(data, labels);
    let has_title = |text: &str| find_weapon(&index, text).is_some();
    // The card is not always centred (the compare and accepted screens): aim at the located card
    // first, and fall back to the layout's own rect when that read finds no weapon.
    let default = layout.rect();
    let aligned = target.map_or(default, |c| riven_scan::align_rect_to(frame, default, c));
    let found = |s: &Result<riven_scan::CardRead, ScanError>| matches!(s, Ok(c) if !c.stats.is_empty() && has_title(&c.result.text));
    let mut scanned = riven_scan::scan_card_titled(root, frame, aligned, labels, &has_title);
    // Weapon not found: look again with a taller crop around the located card, then the layout's own.
    let retries = [
        (aligned != default).then(|| riven_scan::taller(aligned, 1.35)),
        (aligned != default && nth == 0).then_some(default),
    ];
    for rect in retries.into_iter().flatten() {
        if found(&scanned) {
            break;
        }
        let again = riven_scan::scan_card_titled(root, frame, rect, labels, &has_title);
        if found(&again) || !matches!(&scanned, Ok(c) if !c.stats.is_empty()) {
            scanned = again;
        }
    }
    if !matches!(&scanned, Ok(c) if !c.stats.is_empty()) {
        let head: Vec<&str> = match &scanned {
            Ok(c) => c.result.text.lines().take(5).collect(),
            Err(_) => vec![],
        };
        tracing::info!(
            "riven scan found no stats: card {nth} of {located:?}, crop {aligned:?}, first lines {head:?}"
        );
    }
    if nth == 0 && crate::services::scan_captures::enabled() {
        let text = match &scanned {
            Ok(card) => card.result.text.clone(),
            Err(e) => format!("{e:?}"),
        };
        crate::services::scan_captures::save("riven", Some(frame), &text);
    }
    if let Ok(c) = &mut scanned
        && let Some(card) = target
    {
        // The pills sit where the crop puts them; the located card says where the footer really is.
        let strip = riven_scan::footer::read_strip(root, frame, card);
        c.footer.mr = strip.mr.or(c.footer.mr);
        c.footer.rolls = strip.rolls.or(c.footer.rolls);
        if c.footer.mr.is_none() || c.footer.rolls.is_none() {
            // The pills are cut by fractions of a whole-card crop: cut one around the located card.
            let own = riven_scan::footer::read_footer(root, &riven_scan::card_around(frame, card));
            c.footer.mr = c.footer.mr.or(own.mr);
            c.footer.rolls = c.footer.rolls.or(own.rolls);
        }
    }
    match scanned {
        Err(ScanError::Unavailable) => (RivenOverlayView::bare(Unavailable, token), vec![]),
        Err(ScanError::Failed(e)) => (RivenOverlayView::with_message(Failed, token, e), vec![]),
        Ok(card) if card.stats.is_empty() => (RivenOverlayView::bare(Failed, token), vec![]),
        Ok(card) => {
            let weapon = weapon_of(data, labels, &card.result.text);
            if weapon.is_none() {
                let head: Vec<&str> = card.result.text.lines().take(3).collect();
                tracing::info!("riven title not matched to a weapon; first lines: {head:?}");
            }
            // "Text too small" is only true of a card that is small; no located card keeps the old gate.
            let small = target.is_none_or(|c| riven_scan::card_is_small(frame, c));
            let unsure = small && read_unsure(&card, weapon.is_some());
            let view = build_view(data, rolls, weapon, &card.stats, unsure, card.footer);
            (RivenOverlayView::single(token, view), card.stats)
        }
    }
}

fn publish(app: &AppHandle, view: RivenOverlayView) {
    let view = RivenOverlayView {
        session: session().tally.map(Into::into),
        ..view
    };
    let open = view.status != RivenOverlayStatus::Closed;
    let _ = with(app, |m| match (open, m.is_shown(OverlayKind::Riven)) {
        (true, false) => m.show(OverlayKind::Riven),
        (false, true) => m.hide(OverlayKind::Riven),
        _ => {}
    });
    let _ = app.emit(EVENT, view);
}

#[cfg(windows)]
fn capture_source() -> Result<Box<dyn CaptureSource>, String> {
    Ok(Box::new(crate::services::capture::GameWindow))
}

#[cfg(not(windows))]
fn capture_source() -> Result<Box<dyn CaptureSource>, String> {
    Err("windows only".into())
}

/// The Current card of a compare: the one remembered from before the roll when it is the same
/// weapon (it was read bright and whole), else what the dimmed card read as.
fn current_card(
    data: &RivenData,
    rolls: &GoodRolls,
    new_weapon: Option<&str>,
    old: Option<(RivenCardView, Vec<RivenStat>)>,
) -> Option<(RivenCardView, Vec<RivenStat>)> {
    let kept = session().kept.clone();
    if let Some(k) = kept.filter(|k| k.weapon.as_deref() == new_weapon || new_weapon.is_none()) {
        // The single read may have missed the footer pills; the dimmed card still shows them.
        let footer = Footer {
            mr: k.footer.mr.or(old.as_ref().and_then(|(c, _)| c.mr)),
            rolls: k
                .footer
                .rolls
                .or(old.as_ref().and_then(|(c, _)| c.card_rolls)),
        };
        let card = build_view(data, rolls, k.weapon.clone(), &k.stats, k.unsure, footer);
        return Some((card, k.stats));
    }
    old.filter(|(c, _)| c.weapon.is_some())
}

/// A card read whole on a single-card screen goes to the riven memory.
fn remember_card(conn: &Connection, data: &RivenData, rolls: &GoodRolls, card: &Current) {
    let (Some(weapon), true) = (
        card.weapon.clone(),
        !card.unsure && card.stats.len() >= 2 && card.stats.iter().all(|s| s.value.is_some()),
    ) else {
        return;
    };
    let view = build_view(
        data,
        rolls,
        Some(weapon.clone()),
        &card.stats,
        false,
        card.footer,
    );
    let stored = RememberedCard {
        weapon,
        stats: correct_stats(data, view.weapon.as_deref(), &card.stats),
        mr: card.footer.mr,
        rolls: card.footer.rolls,
        grade: view.overall_grade,
        read_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64),
    };
    if let Err(e) = memory::remember(conn, &stored) {
        tracing::warn!("riven memory: saving a card failed: {e}");
    }
}

/// The dimmed old card replaced by the remembered whole read of the same weapon and cycle count.
fn recalled(
    conn: &Connection,
    data: &RivenData,
    rolls: &GoodRolls,
    old: Option<(RivenCardView, Vec<RivenStat>)>,
) -> Option<(RivenCardView, Vec<RivenStat>)> {
    let (view, _) = old.as_ref()?;
    let (weapon, cycles) = (view.weapon.clone()?, view.card_rolls?);
    let hit = memory::recall(conn, &weapon, cycles);
    let Some(hit) = hit else { return old };
    let footer = Footer {
        mr: hit.mr,
        rolls: hit.rolls,
    };
    let card = build_view(data, rolls, Some(hit.weapon), &hit.stats, false, footer);
    Some((card, hit.stats))
}

/// Runs one scan end to end and publishes each step. Must run on the async runtime.
pub async fn scan(app: &AppHandle, layout: RivenCardLayout) -> Result<(), String> {
    let token = GENERATION.begin();
    let compare = layout == RivenCardLayout::Roll;
    // The old card only needs reading when no earlier single read of it is kept, or that one lacks its footer.
    let need_old = session()
        .kept
        .as_ref()
        .is_none_or(|k| k.footer.mr.is_none() || k.footer.rolls.is_none());
    session().compare = compare;
    // The screen keeps what it showed under a small "scanning" chip (a skeleton only when nothing was shown).
    publish(
        app,
        RivenOverlayView {
            layout: if compare {
                RivenViewLayout::Compare
            } else {
                RivenViewLayout::Single
            },
            ..RivenOverlayView::bare(RivenOverlayStatus::Scanning, token)
        },
    );
    let root = ocr_onnx::resource_root(app)?;
    let (data, rolls) = crate::services::rivens::snapshot(app).await?;
    let labels = crate::services::rivens::card_labels(app).await?;
    let read = {
        let (data, rolls) = (data.clone(), rolls.clone());
        tauri::async_runtime::spawn_blocking(move || {
            let cap = capture_source()?;
            Ok::<_, String>(if compare {
                read_compare_retrying(
                    &root,
                    &*cap,
                    &data,
                    &labels,
                    &rolls,
                    token,
                    need_old,
                    COMPARE_RETRY_DELAY,
                    || GENERATION.is_current(token),
                )
            } else {
                CompareRead {
                    new: read_card_retrying(
                        &root,
                        &*cap,
                        layout,
                        &data,
                        &labels,
                        &rolls,
                        token,
                        SINGLE_RETRY_DELAY,
                        || GENERATION.is_current(token),
                    ),
                    old: None,
                }
            })
        })
        .await
        .map_err(|e| e.to_string())??
    };
    if !GENERATION.is_current(token) {
        return Ok(());
    }
    let (mut view, mut new_stats) = read.new;
    // On a compare a card with no weapon is not the new roll (the old card's last line, a glow): it
    // stays unread, so no "couldn't read" hints show while the new card is awaited.
    if compare && view.card.as_ref().is_some_and(|c| c.weapon.is_none()) {
        view.card = None;
        new_stats.clear();
        view.status = RivenOverlayStatus::Failed;
    }
    let ready = view.status == RivenOverlayStatus::Ready;
    if ready {
        let card = view.card.as_ref().expect("a ready view has its card");
        let read_as = Current {
            weapon: card.weapon.clone(),
            source: WeaponSource::Ocr,
            stats: new_stats.clone(),
            unsure: card.unsure,
            footer: Footer {
                mr: card.mr,
                rolls: card.card_rolls,
            },
        };
        if !compare {
            remember_card(&app.state::<Db>().lock(), &data, &rolls, &read_as);
        }
        let mut st = session();
        if !compare {
            st.kept = Some(read_as.clone());
        }
        st.current = Some(read_as);
    } else if !compare {
        session().kept = None;
    }
    let mut cur_stats = vec![];
    if compare {
        let new_weapon = view.card.as_ref().and_then(|c| c.weapon.clone());
        let old = recalled(&app.state::<Db>().lock(), &data, &rolls, read.old);
        let current = current_card(&data, &rolls, new_weapon.as_deref(), old);
        if let Some((card, stats)) = current {
            view.layout = RivenViewLayout::Compare;
            view.current = Some(card);
            cur_stats = stats;
            // A New roll that did not read is the "partial" state: only Current is drawn.
            if !ready {
                view.status = RivenOverlayStatus::Ready;
                view.message = None;
            }
        }
    }
    show_with_analysis(app, &data, token, view, &new_stats, &cur_stats).await;
    Ok(())
}

/// Stats a search can name, signed: what a listing is matched against.
pub fn card_stats(pos: &[String], neg: &[String]) -> Vec<CardStat> {
    let tag = |v: &[String], positive| {
        v.iter()
            .map(move |u| (u.clone(), positive))
            .collect::<Vec<_>>()
    };
    [tag(pos, true), tag(neg, false)].concat()
}

/// Similar listings for a card, loosening the search until there are a few, then scored against it.
async fn analyze_card(
    app: &AppHandle,
    data: &RivenData,
    weapon: &str,
    stats: &[RivenStat],
) -> Result<(SimilarScope, RivenAnalysis), WfmError> {
    let (pos, neg) = search_stats(data, &correct_stats(data, Some(weapon), stats));
    let session = app.state::<Session>();
    let cache = app.state::<SearchCache>();
    let family = data.family_slug(weapon);
    let mut pool: Vec<crate::services::wfm::contracts::WfmContract> = vec![];
    let mut scope = SimilarScope::All;
    for (step, positive, negative) in search_steps(&pos, &neg) {
        let opts = RivenSearchOptions {
            limit: Some(POOL_LIMIT),
            positive_stats: Some(positive),
            negative_stats: Some(negative),
            full_coverage: None,
        };
        let found = search_similar(session.client(), &cache, &family, &opts).await?;
        scope = step;
        for l in found {
            if !pool.iter().any(|p| p.id == l.id) {
                pool.push(l);
            }
        }
        if pool.len() >= ENOUGH_POOL {
            break;
        }
    }
    Ok((scope, analyze(&card_stats(&pos, &neg), &pool)))
}

/// The verdict once both cards have an estimate.
pub fn compare_verdict(cur: &RivenCardView, new: &RivenCardView) -> Option<RivenVerdict> {
    let est = |c: &RivenCardView| c.analysis.as_ref()?.estimate.clone();
    Some(verdict(
        &est(cur)?,
        &est(new)?,
        compare_grades(cur.overall_grade.as_deref(), new.overall_grade.as_deref()),
    ))
}

/// Publishes the card(s), arms the auto-close, then fills in each card's market analysis (and the
/// verdict) as it arrives.
async fn show_with_analysis(
    app: &AppHandle,
    data: &RivenData,
    token: u64,
    mut view: RivenOverlayView,
    new_stats: &[RivenStat],
    cur_stats: &[RivenStat],
) {
    publish(app, view.clone());
    arm(app, token);
    for is_new in [true, false] {
        let (card, stats) = if is_new {
            (view.card.as_ref(), new_stats)
        } else {
            (view.current.as_ref(), cur_stats)
        };
        let Some(weapon) = card.and_then(|c| c.weapon.clone()) else {
            continue;
        };
        let result = analyze_card(app, data, &weapon, stats).await;
        if !GENERATION.is_current(token) {
            return;
        }
        let (scope, analysis) = match result {
            Ok((scope, analysis)) => (Some(scope), analysis),
            Err(e) => {
                tracing::warn!("riven overlay: similar search failed: {e:?}");
                (
                    None,
                    RivenAnalysis {
                        estimate: None,
                        listings: vec![],
                    },
                )
            }
        };
        let slot = if is_new {
            &mut view.card
        } else {
            &mut view.current
        };
        if let Some(c) = slot {
            c.similar_scope = scope;
            c.analysis = Some(analysis);
        }
        if let (Some(cur), Some(new)) = (&view.current, &view.card) {
            view.verdict = compare_verdict(cur, new);
        }
        publish(app, view.clone());
    }
}

fn close(app: &AppHandle) {
    GENERATION.invalidate();
    {
        let mut st = session();
        st.current = None;
        st.kept = None;
    }
    publish(
        app,
        RivenOverlayView::bare(RivenOverlayStatus::Closed, GENERATION.current()),
    );
}

/// Closes this scan's card after the delay unless a newer scan or a close got there first.
fn arm(app: &AppHandle, token: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTO_CLOSE).await;
        if GENERATION.is_current(token) {
            close(&app);
        }
    });
}

/// The compare screen is rescanned as a compare (only the new card is read again); any other as before.
fn rescan_layout(compare: bool) -> RivenCardLayout {
    if compare {
        RivenCardLayout::Roll
    } else {
        RivenCardLayout::default()
    }
}

/// Re-scan hotkey: the first press opens the card, later presses re-read it in the same window.
pub fn on_hotkey(app: &AppHandle) {
    scan_after(app, Duration::ZERO, rescan_layout(session().compare));
}

// WFHelper's delays: the card draws ~0.2 s after the screen opens, a roll reveal takes ~2.85 s.
const OPEN_SCAN_DELAY: Duration = Duration::from_millis(200);
const ROLL_SCAN_DELAY: Duration = Duration::from_millis(2850);
const CHOICE_SCAN_DELAY: Duration = Duration::from_millis(1200);

/// The cycle screen as EE.log reports it (WFHelper's `rivenOverlayIpc`): open reads the card,
/// a confirmed roll counts and reads the compare screen, a pick (the new card or the old one: the log
/// does not say which) reads the single card on screen, close hides.
pub fn on_event(app: &AppHandle, event: &RivenEvent) {
    match event {
        RivenEvent::SessionOpen => {
            tracing::info!("riven overlay: cycle screen opened");
            scan_after(app, OPEN_SCAN_DELAY, RivenCardLayout::Single);
        }
        RivenEvent::RollPending { kuva_per_roll, .. } => {
            let mut s = session();
            s.tally
                .get_or_insert_with(|| RollTally::new(*kuva_per_roll))
                .set_cost(*kuva_per_roll);
        }
        RivenEvent::RollConfirmed => {
            if let Some(t) = session().tally.as_mut() {
                t.roll_done();
            }
            scan_after(app, ROLL_SCAN_DELAY, RivenCardLayout::Roll);
        }
        RivenEvent::ChoiceConfirmed => scan_after(app, CHOICE_SCAN_DELAY, RivenCardLayout::Single),
        RivenEvent::SessionClose => {
            let mut s = session();
            s.tally = None;
            s.compare = false;
            drop(s);
            close(app);
        }
        RivenEvent::ChatView | RivenEvent::DioramaSetup | RivenEvent::WeaponPath(_) => {}
    }
}

/// Scans after `delay` unless a close or a newer scan came in meanwhile.
fn scan_after(app: &AppHandle, delay: Duration, layout: RivenCardLayout) {
    let app = app.clone();
    let before = GENERATION.current();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        if !GENERATION.is_current(before) {
            return;
        }
        if let Err(e) = scan(&app, layout).await {
            tracing::warn!("riven overlay scan failed: {e}");
            publish(
                &app,
                RivenOverlayView::with_message(RivenOverlayStatus::Failed, GENERATION.current(), e),
            );
        }
    });
}

/// Reads the riven on screen and shows it.
#[tauri::command]
#[specta::specta]
pub async fn riven_overlay_scan(
    app: AppHandle,
    layout: Option<RivenCardLayout>,
) -> Result<(), String> {
    scan(&app, layout.unwrap_or_default()).await
}

/// Decides what a fits-in label read does to the current weapon; `Some` is the weapon to switch to.
/// A label that names the current weapon only upgrades its source to `Label`.
fn apply_label(
    data: &RivenData,
    index: &WeaponIndex,
    cur: &mut Current,
    lines: &[String],
) -> Option<String> {
    let label: WeaponLabelMatch = index.find_by_label_line(lines)?;
    let current = cur.weapon.clone().unwrap_or_default();
    if label.name == current {
        cur.source = WeaponSource::Label;
        return None;
    }
    let same_family = !current.is_empty()
        && current != "Riven"
        && data.family_slug(&label.name) == data.family_slug(&current);
    if !should_apply_label_weapon(&label, &current, cur.source, same_family) {
        return None;
    }
    cur.weapon = Some(label.name.clone());
    cur.source = WeaponSource::Label;
    Some(label.name)
}

/// Feeds the OCR lines of the fits-in panel caption: the weapon it names replaces the card's by
/// source and family, and the card is re-graded. Returns the weapon now shown, if the card changed.
#[tauri::command]
#[specta::specta]
pub async fn riven_overlay_apply_label(
    app: AppHandle,
    lines: Vec<String>,
) -> Result<Option<String>, String> {
    let (data, rolls) = crate::services::rivens::snapshot(&app).await?;
    let index = weapon_index(&data, &*crate::services::rivens::card_labels(&app).await?);
    let (weapon, stats, unsure, footer) = {
        let mut st = session();
        let Some(cur) = st.current.as_mut() else {
            return Ok(None);
        };
        let Some(weapon) = apply_label(&data, &index, cur, &lines) else {
            return Ok(None);
        };
        (weapon, cur.stats.clone(), cur.unsure, cur.footer)
    };
    let token = GENERATION.begin();
    let card = build_view(&data, &rolls, Some(weapon.clone()), &stats, unsure, footer);
    show_with_analysis(
        &app,
        &data,
        token,
        RivenOverlayView::single(token, card),
        &stats,
        &[],
    )
    .await;
    Ok(Some(weapon))
}

/// Starts a roll session; the tally rides on every overlay view until it ends.
#[tauri::command]
#[specta::specta]
pub async fn riven_session_start(kuva_per_roll: u32) -> Result<RivenRollTally, String> {
    let tally = RollTally::new(kuva_per_roll);
    session().tally = Some(tally);
    Ok(tally.into())
}

/// Counts one finished roll (whether or not its scan read anything).
#[tauri::command]
#[specta::specta]
pub async fn riven_session_roll_done() -> Result<RivenRollTally, String> {
    let tally = session()
        .tally
        .as_mut()
        .ok_or("no riven session running")?
        .roll_done();
    Ok(tally.into())
}

#[tauri::command]
#[specta::specta]
pub async fn riven_session_end() -> Result<(), String> {
    session().tally = None;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use image::RgbaImage;

    use super::*;
    use crate::services::bundled_data::BundledData;
    use crate::services::capture::{CaptureKind, Frame};

    fn data() -> Option<&'static RivenData> {
        static DATA: OnceLock<Option<RivenData>> = OnceLock::new();
        DATA.get_or_init(|| {
            crate::services::bundled_data::test_root()
                .map(|root| RivenData::load(&BundledData::new(root)))
        })
        .as_ref()
    }

    /// Runs `retry_read` over a scripted reader: `true` entries are good reads. Returns (result, calls).
    fn run_retry(script: &[bool], alive: impl Fn() -> bool) -> (bool, u32) {
        let calls = std::cell::Cell::new(0u32);
        let out = retry_read(
            SINGLE_RETRIES,
            Duration::ZERO,
            alive,
            |ok: &bool| !*ok,
            || {
                let i = calls.get() as usize;
                calls.set(calls.get() + 1);
                script.get(i).copied().unwrap_or(false)
            },
        );
        (out, calls.get())
    }

    #[test]
    fn retry_read_stops_at_the_first_good_read() {
        assert_eq!(run_retry(&[false, false, true, true], || true), (true, 3));
    }

    #[test]
    fn retry_read_gives_up_after_the_last_try() {
        assert_eq!(run_retry(&[], || true), (false, 1 + SINGLE_RETRIES));
    }

    #[test]
    fn retry_read_stops_when_the_scan_is_superseded() {
        let alive = std::cell::Cell::new(true);
        let calls = std::cell::Cell::new(0);
        let out = retry_read(
            SINGLE_RETRIES,
            Duration::ZERO,
            || alive.get(),
            |ok: &bool| !*ok,
            || {
                calls.set(calls.get() + 1);
                alive.set(false); // a newer scan began during the first read
                false
            },
        );
        assert!(!out);
        assert_eq!(calls.get(), 1);
    }

    fn stat(name: &str, positive: bool, value: f64) -> RivenStat {
        RivenStat {
            name: name.into(),
            positive,
            display_positive: None,
            value: Some(value),
            multiplier: false,
        }
    }

    fn rubico_card() -> Vec<RivenStat> {
        vec![
            stat("Critical Damage", true, 90.0),
            stat("Multishot", true, 60.0),
            stat("Damage", true, 100.0),
            stat("Zoom", false, 30.0),
        ]
    }

    struct Black(bool);
    impl CaptureSource for Black {
        fn capture(&self) -> Result<Frame, String> {
            if self.0 {
                return Err("no game window".into());
            }
            Ok(Frame {
                image: RgbaImage::new(1920, 1080),
                kind: CaptureKind::Fixture,
            })
        }
    }

    fn empty_rolls() -> GoodRolls {
        GoodRolls::new(vec![], None)
    }

    #[test]
    fn known_weapon_grades_every_stat() {
        let Some(data) = data() else { return };
        let v = build_view(
            data,
            &empty_rolls(),
            Some("Rubico".into()),
            &rubico_card(),
            false,
            Footer::default(),
        );
        assert_eq!(v.stats.len(), 4);
        assert!(v.stats.iter().all(|s| s.grade.is_some()));
        assert!(v.overall_grade.is_some());
    }

    fn current_of(weapon: &str, stats: &[RivenStat], rolls: u32) -> Current {
        Current {
            weapon: Some(weapon.into()),
            source: WeaponSource::Ocr,
            stats: stats.to_vec(),
            unsure: false,
            footer: Footer {
                mr: Some(15),
                rolls: Some(rolls),
            },
        }
    }

    fn staticor(status: f64, rolls: u32) -> Current {
        current_of(
            "Staticor",
            &[
                stat("Status Chance", true, status),
                stat("Damage to Corpus", true, 1.66),
            ],
            rolls,
        )
    }

    /// What the dimmed card reads as: weapon and cycles right, stats unreliable.
    fn dimmed(data: &RivenData, rolls: u32) -> Option<(RivenCardView, Vec<RivenStat>)> {
        let junk = [stat("Zoom", false, 1.0)];
        let view = build_view(
            data,
            &empty_rolls(),
            Some("Staticor".into()),
            &junk,
            false,
            Footer {
                mr: Some(15),
                rolls: Some(rolls),
            },
        );
        Some((view, junk.to_vec()))
    }

    #[test]
    fn the_compare_takes_its_current_from_memory_and_confirming_promotes_the_new_card() {
        let Some(data) = data() else { return };
        let db = crate::db::mem_db();
        let conn = db.lock();
        let names = |o: Option<(RivenCardView, Vec<RivenStat>)>| {
            o.unwrap()
                .0
                .stats
                .into_iter()
                .map(|s| s.name)
                .collect::<Vec<_>>()
        };
        // Nothing remembered: the dimmed read stands.
        let got = recalled(&conn, data, &empty_rolls(), dimmed(data, 7));
        assert_eq!(names(got), ["Zoom"]);
        // The card read whole on the cycle screen is the next compare's Current.
        remember_card(&conn, data, &empty_rolls(), &staticor(94.2, 7));
        let got = recalled(&conn, data, &empty_rolls(), dimmed(data, 7));
        assert_eq!(names(got.clone()), ["Status Chance", "Damage to Corpus"]);
        assert_eq!(got.unwrap().1[0].value, Some(94.2));
        // A card of another cycle count is not it.
        assert_eq!(
            names(recalled(&conn, data, &empty_rolls(), dimmed(data, 8))),
            ["Zoom"]
        );
        // Accepting: the screen afterwards shows the new card, which becomes the remembered one.
        let mut accepted = staticor(146.5, 7);
        accepted.footer.rolls = Some(7);
        remember_card(&conn, data, &empty_rolls(), &accepted);
        let got = recalled(&conn, data, &empty_rolls(), dimmed(data, 7));
        assert_eq!(got.unwrap().1[0].value, Some(146.5));
    }

    #[test]
    fn the_rescan_key_stays_on_the_compare_screen() {
        assert_eq!(rescan_layout(true), RivenCardLayout::Roll);
        assert_eq!(rescan_layout(false), RivenCardLayout::Single);
    }

    #[test]
    fn a_recoil_curse_keeps_the_sign_the_card_prints() {
        let Some(data) = data() else { return };
        let mut recoil = stat("Weapon Recoil", false, 51.2);
        recoil.display_positive = Some(true);
        let v = build_view(
            data,
            &empty_rolls(),
            Some("Staticor".into()),
            &[recoil],
            false,
            Footer::default(),
        );
        assert_eq!(v.stats[0].printed_positive, Some(true));
        assert!(!v.stats[0].positive);
    }

    #[test]
    fn unknown_weapon_shows_the_stats_ungraded() {
        let Some(data) = data() else { return };
        let v = build_view(
            data,
            &empty_rolls(),
            None,
            &rubico_card(),
            true,
            Footer::default(),
        );
        assert!(v.unsure);
        assert!(v.stats.iter().all(|s| s.grade.is_none()));
        assert!(v.overall_grade.is_none() && v.best.is_none());
    }

    #[test]
    fn the_small_text_hint_needs_an_actually_unclear_read() {
        let read = |incomplete, low_confidence, stats| riven_scan::CardRead {
            result: Default::default(),
            stats,
            incomplete,
            attempts: 2,
            low_confidence,
            footer: Footer::default(),
        };
        let mut blank = stat("Critical Damage", true, 90.0);
        blank.value = None;
        // Four clean stats and a weapon: a doubted line alone says nothing.
        assert!(!read_unsure(&read(false, true, rubico_card()), true));
        assert!(read_unsure(&read(false, true, rubico_card()), false));
        assert!(read_unsure(&read(false, true, vec![blank]), true));
        assert!(read_unsure(&read(true, false, rubico_card()), true));
    }

    #[test]
    fn similar_search_loosens_stepwise() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let scopes = |p: &[&str], n: &[&str]| {
            search_steps(&v(p), &v(n))
                .into_iter()
                .map(|(s, ..)| s)
                .collect::<Vec<_>>()
        };
        use SimilarScope::*;
        assert_eq!(scopes(&["a", "b", "c"], &["z"]), [All, TopTwo, WeaponOnly]);
        assert_eq!(scopes(&["a", "b"], &["z"]), [All, TopTwo, WeaponOnly]);
        assert_eq!(scopes(&["a", "b"], &[]), [All, WeaponOnly]);
        assert_eq!(scopes(&[], &[]), [All]);
        let steps = search_steps(&v(&["a", "b", "c"]), &v(&["z"]));
        assert_eq!(
            (steps[1].1.clone(), steps[1].2.clone()),
            (v(&["a", "b"]), v(&[]))
        );
        assert!(steps[2].1.is_empty() && steps[2].2.is_empty());
    }

    #[test]
    fn search_uses_wfm_names_and_splits_the_curse() {
        let Some(data) = data() else { return };
        let (pos, neg) = search_stats(data, &rubico_card());
        assert_eq!(
            pos,
            ["critical_damage", "multishot", "base_damage_/_melee_damage"]
        );
        assert_eq!(neg, ["zoom"]);
    }

    #[test]
    fn weapon_comes_from_the_title_line() {
        let Some(data) = data() else { return };
        assert_eq!(
            weapon_of(
                data,
                &CardLabels::default(),
                "Rubico Crita-ata
Critical Damage"
            )
            .as_deref(),
            Some("Rubico")
        );
        assert_eq!(
            weapon_of(
                data,
                &CardLabels::default(),
                "Critical Damage
Multishot"
            ),
            None
        );
    }

    #[test]
    fn a_header_ahead_of_the_title_on_a_french_card_still_finds_the_weapon() {
        let Some(data) = data() else { return };
        for text in [
            "Mod Riven de Mêlée Rubico Crita-ata
+64,6% de Vitesse d'Attaque",
            "Mod Riven de Fusil
Rubico Crita-ata
+64,6%",
        ] {
            let got = weapon_of(data, &CardLabels::default(), text);
            assert_eq!(got.as_deref(), Some("Rubico"), "{text}");
        }
    }

    #[cfg(windows)]
    struct Still(RgbaImage);
    #[cfg(windows)]
    impl CaptureSource for Still {
        fn capture(&self) -> Result<Frame, String> {
            Ok(Frame {
                image: self.0.clone(),
                kind: CaptureKind::Fixture,
            })
        }
    }

    // The runtime is the Windows DLL until a Linux .so is added.
    #[cfg(windows)]
    #[test]
    fn a_french_card_cut_through_the_title_band_is_retried_and_graded() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let card = ::image::open(root.join("tests/fixtures/riven/initial-card-orthos-fr.png"))
            .expect("fixture")
            .to_rgba8();
        // Offsets where the fixed stat band alone clipped the title and left the weapon unknown.
        for (fw, fh, dy) in [
            (2560u32, 1440u32, 0i64),
            (1920, 1080, -60),
            (2560, 1440, 30),
        ] {
            let mut frame = RgbaImage::from_pixel(fw, fh, image::Rgba([0, 0, 0, 255]));
            let (x, y) = (
                (fw as f32 * 0.22 + (fw as f32 * 0.56 - 370.0) / 2.0) as i64,
                (fh as f32 * 0.43) as i64 + dy,
            );
            image::imageops::replace(&mut frame, &card, x, y);
            let (view, stats) = read_card(
                &root.join("resources"),
                &Still(frame),
                RivenCardLayout::Single,
                data,
                &labels,
                &empty_rolls(),
                1,
            );
            let at = format!("{fw}x{fh} dy={dy}: {view:?}");
            let view = view.card.expect(&at);
            assert_eq!(view.weapon.as_deref(), Some("Orthos"), "{at}");
            assert!(!view.unsure, "{at}");
            assert_eq!((view.mr, view.card_rolls), (Some(10), Some(39)), "{at}");
            assert_eq!(view.card_kuva_spent, Some(121_800), "{at}");
            assert_eq!(stats.len(), 4, "{at}");
            assert_eq!(view.stats.len(), 4, "{at}");
            assert!(
                view.stats
                    .iter()
                    .all(|s| s.value.is_some() && s.grade.is_some()),
                "{at}"
            );
        }
    }

    // The runtime is the Windows DLL until a Linux .so is added.
    #[cfg(windows)]
    #[test]
    fn the_compare_and_accepted_screens_grade_the_new_card() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        for (file, layout) in [
            ("compare-orthos-fr.png", RivenCardLayout::Roll),
            ("accepted-orthos-fr.png", RivenCardLayout::Single),
        ] {
            let frame = ::image::open(root.join("tests/fixtures/riven/screens").join(file))
                .expect("fixture")
                .to_rgba8();
            let (view, stats) = read_card(
                &root.join("resources"),
                &Still(frame),
                layout,
                data,
                &labels,
                &empty_rolls(),
                1,
            );
            let at = format!("{file}: {view:?}");
            assert_eq!(view.status, RivenOverlayStatus::Ready, "{at}");
            let view = view.card.expect(&at);
            assert_eq!(view.weapon.as_deref(), Some("Orthos"), "{at}");
            assert_eq!((view.mr, view.card_rolls), (Some(10), Some(40)), "{at}");
            let got: Vec<(&str, Option<f64>)> =
                stats.iter().map(|s| (s.name.as_str(), s.value)).collect();
            assert_eq!(
                got,
                [
                    ("Toxin", Some(145.0)),
                    ("Critical Damage", Some(122.2)),
                    ("Critical Chance for Slide Attack", Some(70.5)),
                ],
                "{at}"
            );
            assert!(view.stats.iter().all(|s| s.grade.is_some()), "{at}");
        }
    }

    /// Frames in order, the last one repeating; counts the captures.
    #[cfg(windows)]
    struct Frames(Mutex<Vec<RgbaImage>>, std::sync::atomic::AtomicUsize);
    #[cfg(windows)]
    impl CaptureSource for Frames {
        fn capture(&self) -> Result<Frame, String> {
            self.1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let mut f = self.0.lock().unwrap();
            let image = if f.len() > 1 {
                f.remove(0)
            } else {
                f[0].clone()
            };
            Ok(Frame {
                image,
                kind: CaptureKind::Fixture,
            })
        }
    }

    // The runtime is the Windows DLL until a Linux .so is added.
    #[cfg(windows)]
    #[test]
    fn a_compare_read_retries_until_the_new_card_has_drawn() {
        use std::sync::atomic::Ordering::SeqCst;
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let card =
            ::image::open(root.join("tests/fixtures/riven/screens/compare-akbronco-cycle-fr.png"))
                .expect("fixture")
                .to_rgba8();
        let blank = RgbaImage::new(card.width(), card.height());
        let run = |frames: Vec<RgbaImage>, alive: bool| {
            let cap = Frames(Mutex::new(frames), 0.into());
            let r = read_compare_retrying(
                &root.join("resources"),
                &cap,
                data,
                &labels,
                &empty_rolls(),
                1,
                true,
                Duration::ZERO,
                || alive,
            );
            (r, cap.1.load(SeqCst))
        };
        // Two frames before the card draws, then it reads: Akbronco with both stats and the MR.
        let (r, shots) = run(vec![blank.clone(), blank.clone(), card.clone()], true);
        let at = format!("{:?}", r.new.0);
        let new = r.new.0.card.expect(&at);
        assert_eq!(new.weapon.as_deref(), Some("Akbronco"), "{at}");
        assert_eq!((new.mr, new.stats.len()), (Some(10), 2), "{at}");
        // This fixture's cycle pill never reads, so the retries also run on the drawn card (2 + 2).
        assert_eq!(shots, 4);
        // A card read whole first time (Tekko, with its pills) is read once.
        let tekko = ::image::open(root.join("tests/fixtures/riven/screens/compare-tekko-fr.png"))
            .expect("fixture")
            .to_rgba8();
        assert_eq!(run(vec![tekko], true).1, 1);
        // Never drawn: gives up after the retries; a superseded scan stops at once.
        assert_eq!(
            run(vec![blank.clone()], true).1,
            1 + COMPARE_RETRIES as usize
        );
        assert_eq!(run(vec![blank], false).1, 1);
    }

    // The runtime is the Windows DLL until a Linux .so is added.
    #[cfg(windows)]
    #[test]
    fn a_title_wrapped_onto_two_lines_still_names_the_weapon() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let frame =
            ::image::open(root.join("tests/fixtures/riven/screens/roll-akbronco-wrapped-fr.png"))
                .expect("fixture")
                .to_rgba8();
        let (view, stats) = read_card(
            &root.join("resources"),
            &Still(frame),
            RivenCardLayout::Single,
            data,
            &labels,
            &empty_rolls(),
            1,
        );
        let at = format!("{view:?}");
        assert_eq!(view.status, RivenOverlayStatus::Ready, "{at}");
        let view = view.card.expect(&at);
        assert_eq!(view.weapon.as_deref(), Some("Akbronco"), "{at}");
        assert_eq!((view.mr, view.card_rolls), (Some(10), None), "{at}");
        let got: Vec<Option<f64>> = stats.iter().map(|s| s.value).collect();
        assert_eq!(
            got,
            [Some(166.7), Some(143.4), Some(1.49), Some(0.59)],
            "{at}"
        );
    }

    // The runtime is the Windows DLL until a Linux .so is added.
    #[cfg(windows)]
    #[test]
    fn the_compare_screen_of_a_smaller_card_reads_title_stats_and_footer() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let shot = ::image::open(root.join("tests/fixtures/riven/screens/compare-staticor-fr.png"))
            .expect("fixture")
            .to_rgba8();
        // The capture is the game window, whatever size the player runs it at.
        for width in [1280u32, 1525, 1920, 2560] {
            let height = shot.height() * width / shot.width();
            let frame = image::imageops::resize(
                &shot,
                width,
                height,
                image::imageops::FilterType::Lanczos3,
            );
            let (view, stats) = read_card(
                &root.join("resources"),
                &Still(frame),
                RivenCardLayout::Roll,
                data,
                &labels,
                &empty_rolls(),
                1,
            );
            let at = format!("{width}px: {view:?}");
            let view = view.card.expect(&at);
            assert_eq!(view.weapon.as_deref(), Some("Staticor"), "{at}");
            assert_eq!((view.mr, view.card_rolls), (Some(15), Some(4)), "{at}");
            assert!(!view.unsure, "{at}");
            let got: Vec<Option<f64>> = stats.iter().map(|s| s.value).collect();
            assert_eq!(got, [Some(134.0), Some(144.3), Some(31.7)], "{at}");
        }
    }

    // The runtime is the Windows DLL until a Linux .so is added.
    #[cfg(windows)]
    #[cfg(windows)]
    #[test]
    fn the_compare_screen_reads_both_cards_old_dimmed_and_new() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let read = |file: &str| {
            let frame = ::image::open(root.join("tests/fixtures/riven/screens").join(file))
                .expect("fixture")
                .to_rgba8();
            read_compare(
                &root.join("resources"),
                &Still(frame),
                data,
                &labels,
                &empty_rolls(),
                1,
                true,
            )
        };
        let card = |v: &RivenCardView| {
            (
                v.weapon.clone().unwrap_or_default(),
                v.mr,
                v.card_rolls,
                v.stats.iter().map(|s| s.value).collect::<Vec<_>>(),
            )
        };
        let r = read("compare-orthos-fr.png");
        let new = r.new.0.card.expect("new card");
        assert_eq!(
            card(&new),
            (
                "Orthos".into(),
                Some(10),
                Some(40),
                vec![Some(145.0), Some(122.2), Some(70.5)]
            )
        );
        let (old, _) = r.old.expect("old card");
        assert_eq!(old.weapon.as_deref(), Some("Orthos"));
        assert_eq!((old.mr, old.card_rolls), (Some(10), Some(40)));
        assert_eq!(old.stats.len(), 4, "{old:?}");
        assert_eq!(old.stats[0].value, Some(64.6));

        let r = read("compare-staticor-fr.png");
        assert_eq!(card(&r.new.0.card.expect("new card")).0, "Staticor");
        let (old, _) = r.old.expect("old card");
        assert_eq!(old.weapon.as_deref(), Some("Staticor"));
        assert_eq!((old.mr, old.card_rolls), (Some(15), Some(4)));

        // Three stats (two wrapped, a recoil curse that prints a plus): the new card, not the old one's.
        let r = read("compare-staticor-hexatron-fr.png");
        let new = r.new.0.card.expect("new card");
        assert_eq!(card(&new).0, "Staticor");
        assert_eq!((new.mr, new.card_rolls), (Some(15), Some(7)));
        let got: Vec<_> = new
            .stats
            .iter()
            .map(|s| (s.name.as_str(), s.value))
            .collect();
        assert_eq!(
            got,
            [
                ("Status Chance", Some(146.5)),
                ("Damage to Corpus", Some(1.66)),
                ("Weapon Recoil", Some(51.2)),
            ]
        );
        assert_eq!(new.stats[2].printed_positive, Some(true));
        assert!(!new.stats[2].positive);
        let (old, _) = r.old.expect("old card");
        assert_eq!((old.mr, old.card_rolls), (Some(15), Some(7)));
    }

    /// A single-digit cycle pill ("1") on both cards, the old one dimmed (a real 1920x1080 capture).
    #[cfg(windows)]
    #[test]
    fn the_compare_screen_reads_a_one_digit_cycle_count_on_both_cards() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let frame = ::image::open(root.join("tests/fixtures/riven/screens/compare-akbolto-fr.png"))
            .expect("fixture")
            .to_rgba8();
        let r = read_compare(
            &root.join("resources"),
            &Still(frame),
            data,
            &labels,
            &empty_rolls(),
            1,
            true,
        );
        let new = r.new.0.card.expect("new card");
        let (old, _) = r.old.expect("old card");
        for (name, card) in [("new", &new), ("old", &old)] {
            assert_eq!(card.weapon.as_deref(), Some("Akbolto"), "{name}");
            assert_eq!(
                (card.mr, card.card_rolls),
                (Some(15), Some(1)),
                "{name}: {card:?}"
            );
        }
        let got: Vec<_> = new.stats.iter().map(|s| s.value).collect();
        assert_eq!(got, [Some(107.1), Some(95.4), Some(105.4), Some(77.9)]);
        let got: Vec<_> = old.stats.iter().map(|s| s.value).collect();
        assert_eq!(got, [Some(96.7), Some(61.8), Some(85.0)]);
    }

    /// Same one-digit cycle pill on a card with a different rank polarity and a 4-line new card (Tekko).
    #[cfg(windows)]
    #[test]
    fn the_compare_screen_reads_a_one_digit_cycle_count_on_a_tekko() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let frame = ::image::open(root.join("tests/fixtures/riven/screens/compare-tekko-fr.png"))
            .expect("fixture")
            .to_rgba8();
        let r = read_compare(
            &root.join("resources"),
            &Still(frame),
            data,
            &labels,
            &empty_rolls(),
            1,
            true,
        );
        let new = r.new.0.card.expect("new card");
        let (old, _) = r.old.expect("old card");
        for (name, card) in [("new", &new), ("old", &old)] {
            assert_eq!(card.weapon.as_deref(), Some("Tekko"), "{name}");
            assert_eq!(
                (card.mr, card.card_rolls),
                (Some(12), Some(1)),
                "{name}: {card:?}"
            );
        }
        let got: Vec<_> = new.stats.iter().map(|s| s.value).collect();
        assert_eq!(got, [Some(105.0), Some(192.8), Some(1.52), Some(0.58)]);
        let got: Vec<_> = old.stats.iter().map(|s| s.value).collect();
        assert_eq!(got, [Some(79.4), Some(80.0), Some(112.0)]);
    }

    /// Right after a cycle the cycle pills fade in after the rest of the cards: a read without them
    /// (both cards) is taken again until they show.
    #[cfg(windows)]
    #[test]
    fn a_compare_read_waits_for_the_cycle_pills() {
        let Some(data) = data() else { return };
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let labels = CardLabels::load(&BundledData::new(root.join("resources/game-data")), "fr");
        let full = ::image::open(root.join("tests/fixtures/riven/screens/compare-tekko-fr.png"))
            .expect("fixture")
            .to_rgba8();
        let mut early = full.clone();
        for (x0, y0, x1, y1) in [(1025, 822, 1095, 862), (650, 795, 705, 820)] {
            for y in y0..y1 {
                for x in x0..x1 {
                    early.put_pixel(x, y, ::image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        let cap = Frames(Mutex::new(vec![early, full]), 0.into());
        let r = read_compare_retrying(
            &root.join("resources"),
            &cap,
            data,
            &labels,
            &empty_rolls(),
            1,
            true,
            Duration::ZERO,
            || true,
        );
        let new = r.new.0.card.expect("new card");
        let (old, _) = r.old.expect("old card");
        assert_eq!(
            (new.card_rolls, old.card_rolls),
            (Some(1), Some(1)),
            "{} captures",
            cap.1.load(std::sync::atomic::Ordering::SeqCst)
        );
        assert_eq!(cap.1.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// The compare screen's card says "Orthos" (disposition 1.0), its caption "S'ÉQUIPE SUR Orthos
    /// Prime" (0.7). The read values fit the card's weapon, and the grader logs a misfit and regrades
    /// the Prime name as Orthos, so the card weapon is graded and the caption is not used.
    #[test]
    fn the_card_weapon_not_the_equip_caption_is_the_one_the_values_fit() {
        let Some(data) = data() else { return };
        let stats = [
            stat("Toxin", true, 145.0),
            stat("Critical Damage", true, 122.2),
            stat("Critical Chance for Slide Attack", false, 70.5),
        ];
        let grade = |w: &str| {
            let v = build_view(
                data,
                &empty_rolls(),
                Some(w.into()),
                &stats,
                false,
                Footer::default(),
            );
            (
                v.overall_grade,
                v.stats.into_iter().map(|s| s.grade).collect::<Vec<_>>(),
            )
        };
        assert_eq!(data.weapon_disposition("Orthos"), Some(1.0));
        assert_eq!(data.weapon_disposition("Orthos Prime"), Some(0.7));
        assert_eq!(grade("Orthos"), grade("Orthos Prime"));
        // The values fit only at the normalized 1.2 disposition, not at Orthos' bundled 1.0.
        assert_eq!(grade("Orthos").0.as_deref(), Some("B"));
    }

    #[test]
    fn missing_runtime_reports_unavailable_not_failure() {
        let Some(data) = data() else { return };
        let none = Path::new("no-such-dir");
        let (v, stats) = read_card(
            none,
            &Black(false),
            RivenCardLayout::Single,
            data,
            &CardLabels::default(),
            &empty_rolls(),
            7,
        );
        assert_eq!((v.status, v.token), (RivenOverlayStatus::Unavailable, 7));
        assert!(stats.is_empty());
    }

    #[test]
    fn capture_error_is_a_failure_with_its_message() {
        let Some(data) = data() else { return };
        let (v, _) = read_card(
            Path::new("."),
            &Black(true),
            RivenCardLayout::Chat,
            data,
            &CardLabels::default(),
            &empty_rolls(),
            2,
        );
        assert_eq!(v.status, RivenOverlayStatus::Failed);
        assert_eq!(v.message.as_deref(), Some("no game window"));
    }

    #[test]
    fn a_newer_scan_or_close_makes_older_results_stale() {
        let g = ScanGeneration::default();
        let first = g.begin();
        let second = g.begin();
        assert!(!g.is_current(first) && g.is_current(second));
        g.invalidate();
        assert!(!g.is_current(second));
    }

    #[test]
    fn label_replaces_the_weapon_by_source_and_family() {
        let Some(data) = data() else { return };
        let index = WeaponIndex::new(data.all_weapon_names().iter().map(String::as_str));
        let cur = |w: Option<&str>, source| Current {
            weapon: w.map(Into::into),
            source,
            stats: rubico_card(),
            unsure: false,
            footer: Footer::default(),
        };
        let lines = vec!["FITS IN".to_owned(), "Rubico".to_owned()];
        // An exact label beats a different weapon from the dialog.
        let mut c = cur(Some("Soma"), WeaponSource::Dialog);
        assert_eq!(
            apply_label(data, &index, &mut c, &lines).as_deref(),
            Some("Rubico")
        );
        assert_eq!(
            (c.weapon.as_deref(), c.source),
            (Some("Rubico"), WeaponSource::Label)
        );
        // The same weapon only upgrades the source.
        let mut c = cur(Some("Rubico"), WeaponSource::Ocr);
        assert_eq!(apply_label(data, &index, &mut c, &lines), None);
        assert_eq!(c.source, WeaponSource::Label);
        // A fuzzy label does not override a dialog weapon of another family.
        let mut c = cur(Some("Soma"), WeaponSource::Dialog);
        assert_eq!(
            apply_label(data, &index, &mut c, &["Rubic0".to_owned()]),
            None
        );
        assert_eq!(c.weapon.as_deref(), Some("Soma"));
        // No weapon yet: any label applies.
        let mut c = cur(None, WeaponSource::None);
        assert!(apply_label(data, &index, &mut c, &lines).is_some());
    }

    #[test]
    fn correcting_leaves_a_clean_card_alone() {
        let Some(data) = data() else { return };
        let card = rubico_card();
        assert_eq!(correct_stats(data, None, &card), card);
        assert_eq!(correct_stats(data, Some("Rubico"), &card), card);
    }

    #[test]
    fn roll_tally_counts_rolls_and_kuva() {
        let mut t = RollTally::new(900);
        t.roll_done();
        t.set_cost(1000);
        let t = t.roll_done();
        assert_eq!((t.roll_count, t.total_kuva_spent), (2, 1900));
        assert_eq!(RivenRollTally::from(t).total_kuva_spent, 1900);
    }
}
