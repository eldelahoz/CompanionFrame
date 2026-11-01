//! Scan retries and round timing, on a paused clock with scripted attempts.
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use image::RgbaImage;
use tokio::time::Instant;

use super::RewardReader;
use super::flow::*;
use super::names::PoolItem;
use super::scan_tests::{PLAIN, frame_with_cards};
use crate::services::capture::{CaptureKind, CaptureSource, Frame};

type Scripted = Result<Option<Attempt<u32>>, String>;

fn attempt(cards: usize, read: usize) -> Scripted {
    Ok(Some(Attempt {
        cards,
        items: (0..cards).map(|i| (i < read).then_some(i as u32)).collect(),
    }))
}

async fn run(script: Vec<Scripted>) -> (Result<Outcome<u32>, String>, usize, Duration) {
    let queue = Mutex::new(VecDeque::from(script));
    let calls = Mutex::new(0usize);
    let start = Instant::now();
    let out = run_attempts(|| {
        *calls.lock().unwrap() += 1;
        let next = queue.lock().unwrap().pop_front().unwrap_or(Ok(None));
        async move { next }
    })
    .await;
    let n = *calls.lock().unwrap();
    (out, n, start.elapsed())
}

#[tokio::test(start_paused = true)]
async fn a_full_read_returns_at_once() {
    let (out, calls, took) = run(vec![attempt(4, 4)]).await;
    assert!(matches!(out, Ok(Outcome::Read(v)) if v.len() == 4));
    assert_eq!((calls, took), (1, Duration::ZERO));
}

#[tokio::test(start_paused = true)]
async fn a_fade_that_left_a_slot_unread_gets_one_more_try() {
    let (out, calls, took) = run(vec![attempt(4, 3), attempt(4, 4)]).await;
    assert!(matches!(out, Ok(Outcome::Read(_))));
    assert_eq!((calls, took), (2, Duration::from_millis(450)));
}

#[tokio::test(start_paused = true)]
async fn two_short_reads_ship_nothing_instead_of_a_wrong_set() {
    let (out, calls, _) = run(vec![attempt(4, 3), attempt(4, 2), attempt(4, 4)]).await;
    assert!(matches!(out, Ok(Outcome::Partial { read: 2, cards: 4 })));
    assert_eq!(calls, 2);
}

#[tokio::test(start_paused = true)]
async fn unread_cards_are_retried_until_the_window_closes() {
    let script = (0..20).map(|_| attempt(2, 0)).collect();
    let (out, calls, took) = run(script).await;
    assert!(matches!(out, Ok(Outcome::Nothing)));
    assert!(
        calls <= 10 && took <= Duration::from_secs(5),
        "{calls} {took:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn three_frames_without_a_layout_are_not_the_reward_screen() {
    let (out, calls, _) = run(vec![Ok(None), Ok(None), Ok(None), attempt(2, 2)]).await;
    assert!(matches!(out, Ok(Outcome::NotRewardScreen)));
    assert_eq!(calls, 3);
}

#[tokio::test(start_paused = true)]
async fn a_capture_that_always_fails_is_an_error() {
    let (out, _, _) = run(vec![
        Err("black".into()),
        Err("black".into()),
        Err("black".into()),
    ])
    .await;
    assert_eq!(out.err().as_deref(), Some("black"));
}

fn at(base: Instant, ms: u64) -> Instant {
    base + Duration::from_millis(ms)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn eelog_round(t: Instant) -> Session {
    let mut s = Session::default();
    s.begin(Trigger::EeLog, Duration::ZERO, t);
    s
}

#[tokio::test(start_paused = true)]
async fn a_second_trigger_while_scanning_is_ignored() {
    let (mut s, t) = (Session::default(), Instant::now());
    assert_eq!(s.begin(Trigger::Hotkey, Duration::ZERO, t), Some(1));
    assert_eq!(s.begin(Trigger::EeLog, Duration::ZERO, t), None);
    s.finish();
    assert_eq!(s.begin(Trigger::Hotkey, Duration::ZERO, t), Some(2));
}

#[tokio::test(start_paused = true)]
async fn the_log_cannot_reopen_a_just_closed_overlay_but_the_hotkey_can() {
    let (mut s, t) = (Session::default(), Instant::now());
    s.closed(t);
    assert_eq!(s.begin(Trigger::EeLog, Duration::ZERO, at(t, 2_999)), None);
    assert_eq!(
        s.begin(Trigger::Hotkey, Duration::ZERO, at(t, 100)),
        Some(1)
    );
    s.finish();
    assert_eq!(
        s.begin(Trigger::EeLog, Duration::ZERO, at(t, 3_000)),
        Some(2)
    );
}

#[tokio::test(start_paused = true)]
async fn the_log_scan_waits_out_the_fixed_delay_from_the_trigger() {
    let (mut s, t) = (Session::default(), Instant::now());
    s.begin(Trigger::EeLog, ms(200), t);
    assert_eq!(s.scan_delay(t), ms(250));
    assert_eq!(s.scan_delay(at(t, 900)), Duration::ZERO);
}

#[tokio::test(start_paused = true)]
async fn success_stays_until_the_vote_ends_but_never_under_five_seconds() {
    let t = Instant::now();
    let s = eelog_round(t);
    assert_eq!(s.success_hide(at(t, 1_000), false), ms(13_500));
    assert_eq!(s.success_hide(at(t, 1_000), true), ms(9_000));
    assert_eq!(s.success_hide(at(t, 13_000), false), ms(5_000));
    let mut h = Session::default();
    h.begin(Trigger::Hotkey, Duration::ZERO, t);
    assert_eq!(h.success_hide(t, false), ms(8_500));
}

#[tokio::test(start_paused = true)]
async fn the_game_closing_its_screen_shortens_the_card() {
    let t = Instant::now();
    // Mid-scan: only noted; the success path then uses the minimum.
    let mut s = eelog_round(t);
    assert_eq!(s.on_screen_closed(Duration::ZERO, at(t, 2_000)), None);
    assert_eq!(s.success_hide(at(t, 2_500), false), ms(5_000));
    s.finish();
    assert_eq!(
        s.on_screen_closed(Duration::ZERO, at(t, 3_000)),
        None,
        "same round"
    );

    let idle = |staleness, now| {
        let mut s = eelog_round(t);
        s.finish();
        s.on_screen_closed(staleness, at(t, now))
    };
    assert_eq!(idle(Duration::ZERO, 1_000), Some(ms(4_000)));
    assert_eq!(idle(Duration::ZERO, 8_000), Some(ms(1_500)));
    assert_eq!(idle(ms(6_000), 8_000), None, "stale line");
}

struct Fixed(&'static str);
impl RewardReader for Fixed {
    fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
        vec![self.0.into()]
    }
}

struct Synthetic(RgbaImage);
impl CaptureSource for Synthetic {
    fn capture(&self) -> Result<Frame, String> {
        Ok(Frame {
            image: self.0.clone(),
            kind: CaptureKind::Fixture,
        })
    }
}

#[test]
fn a_frame_with_three_cards_reads_three_english_names() {
    let src = Synthetic(frame_with_cards(1920, 1080, 3, &PLAIN));
    let pool = vec![
        PoolItem::new("Khora Prime Systems Blueprint", None),
        PoolItem::new(
            "Khora Prime Systeme",
            Some("Khora Prime Systems Blueprint".into()),
        ),
    ];
    let (a, dbg) = scan_once(
        &src,
        &[&Fixed("Khora Prime Systems Blueprint")],
        &pool,
        0.99,
    )
    .unwrap()
    .unwrap();
    assert_eq!(a.cards, 3);
    assert!(
        a.items
            .iter()
            .all(|i| i.as_deref() == Some("Khora Prime Systems Blueprint"))
    );
    assert_eq!(dbg.slots.len(), 3);
}

#[test]
fn a_frame_without_cards_is_no_layout_and_a_failing_capture_an_error() {
    let blank = Synthetic(RgbaImage::new(1920, 1080));
    assert!(
        scan_once(&blank, &[&Fixed("x")], &[], 0.99)
            .unwrap()
            .is_none()
    );
    struct Broken;
    impl CaptureSource for Broken {
        fn capture(&self) -> Result<Frame, String> {
            Err("no window".into())
        }
    }
    assert!(scan_once(&Broken, &[&Fixed("x")], &[], 0.99).is_err());
}

#[test]
fn an_unreadable_slot_scan_falls_back_to_whole_names_in_the_band_text() {
    let src = Synthetic(frame_with_cards(1920, 1080, 2, &PLAIN));
    let pool = vec![
        PoolItem::new("Forma Blueprint", None),
        PoolItem::new("Braton Prime Stock", None),
    ];
    // Slot reads see the whole text too, but a pool-wide read finds two names, none gated per slot.
    let (a, dbg) = scan_once(
        &src,
        &[&Fixed("noise Forma Blueprint xx Braton Prime Stock")],
        &pool,
        0.99,
    )
    .unwrap()
    .unwrap();
    assert_eq!(a.cards, a.items.len());
    assert!(a.items.iter().all(Option::is_some));
    assert!(!dbg.slots.is_empty());
}

#[test]
fn the_fallback_is_skipped_for_readers_that_opt_out() {
    struct NoFallback;
    impl RewardReader for NoFallback {
        fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
            vec!["Forma Blueprint".into()]
        }
        fn text_fallback(&self) -> bool {
            false
        }
    }
    let blank = Synthetic(RgbaImage::new(1920, 1080));
    let pool = vec![PoolItem::new("Forma Blueprint", None)];
    assert!(
        scan_once(&blank, &[&NoFallback], &pool, 0.99)
            .unwrap()
            .is_none()
    );
    let (a, _) = scan_once(&blank, &[&Fixed("Forma Blueprint")], &pool, 0.99)
        .unwrap()
        .unwrap();
    assert_eq!(a.items, vec![Some("Forma Blueprint".to_string())]);
}

#[test]
fn a_later_reader_fills_only_the_slots_the_first_left_unread() {
    struct Calls(std::sync::atomic::AtomicUsize, &'static str);
    impl RewardReader for Calls {
        fn ocr_lines(&self, _: &RgbaImage) -> Vec<String> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            vec![self.1.into()]
        }
    }
    let src = Synthetic(frame_with_cards(1920, 1080, 2, &PLAIN));
    let pool = vec![PoolItem::new("Forma Blueprint", None)];
    let first = Calls(Default::default(), "Forma Blueprint");
    let second = Calls(Default::default(), "zzz");
    let (a, _) = scan_once(&src, &[&first, &second], &pool, 0.99)
        .unwrap()
        .unwrap();
    assert!(a.items.iter().all(Option::is_some));
    assert_eq!(
        second.0.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "the fallback reader never ran"
    );

    let unread = Calls(Default::default(), "zzz");
    let (a, dbg) = scan_once(&src, &[&unread, &first], &pool, 0.99)
        .unwrap()
        .unwrap();
    assert!(a.items.iter().all(Option::is_some), "{a:?}", a = a.items);
    assert!(dbg.slots[0].iter().any(|t| t == "zzz"));
}

#[tokio::test(start_paused = true)]
async fn a_hotkey_after_the_reward_screen_closed_knows_it_is_gone() {
    let t = Instant::now();
    assert!(
        !Session::default().reward_screen_gone(t),
        "no EE.log round yet"
    );
    let mut s = eelog_round(t);
    s.finish();
    assert!(!s.reward_screen_gone(at(t, 2_000)), "still voting");
    assert!(s.reward_screen_gone(at(t, 20_000)), "vote window over");
    s.on_screen_closed(Duration::ZERO, at(t, 3_000));
    s.begin(Trigger::Hotkey, Duration::ZERO, at(t, 4_000));
    assert!(s.reward_screen_gone(at(t, 4_000)), "close line seen");
}
