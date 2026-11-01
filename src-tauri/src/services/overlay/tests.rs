use super::manager::*;

fn r(x: i32, y: i32, w: i32, h: i32) -> OverlayRect {
    OverlayRect { x, y, w, h }
}

const K: OverlayKind = OverlayKind::Reward;

#[derive(Default)]
struct Fake {
    log: Vec<String>,
    keep: bool,
    topmost: bool,
    game: Option<Game>,
    cursor: Option<(i32, i32)>,
    off: bool,
}

impl Fake {
    fn count(&self, prefix: &str) -> usize {
        self.log.iter().filter(|l| l.starts_with(prefix)).count()
    }
    fn last_through(&self) -> Option<&String> {
        self.log.iter().rev().find(|l| l.starts_with("through"))
    }
}

impl Host for Fake {
    fn show(&mut self, k: OverlayKind) {
        self.log.push(format!("show {}", k.name()));
    }
    fn hide(&mut self, k: OverlayKind) {
        self.log.push(format!("hide {}", k.name()));
    }
    fn set_click_through(&mut self, k: OverlayKind, on: bool) {
        self.log.push(format!("through {} {on}", k.name()));
    }
    fn place(&mut self, k: OverlayKind, p: OverlayRect) {
        self.log.push(format!("place {} {},{}", k.name(), p.x, p.y));
    }
    fn is_topmost(&self, _: OverlayKind) -> bool {
        self.topmost
    }
    fn raise(&mut self, k: OverlayKind) {
        self.log.push(format!("raise {}", k.name()));
        self.topmost = true;
    }
    fn set_content_visible(&mut self, k: OverlayKind, v: bool) {
        self.log.push(format!("content {} {v}", k.name()));
    }
    fn game(&mut self) -> Option<Game> {
        self.game
    }
    fn primary_monitor(&self) -> Option<OverlayRect> {
        Some(r(0, 0, 1920, 1080))
    }
    fn cursor(&self) -> Option<(i32, i32)> {
        self.cursor
    }
    fn restore_game_focus(&mut self) {
        self.log.push("restore focus".into());
    }
    fn set_activatable(&mut self, k: OverlayKind, on: bool) {
        self.log.push(format!("activatable {} {on}", k.name()));
    }
    fn take_focus_for_move(&mut self, k: OverlayKind) {
        self.log.push(format!("grab {}", k.name()));
    }
    fn keep_mapped(&self) -> bool {
        self.keep
    }
    fn enabled(&self, _: OverlayKind) -> bool {
        !self.off
    }
}

fn game(x: i32, fg: Foreground) -> Option<Game> {
    Some(Game {
        monitor: r(x, 0, 2560, 1440),
        foreground: fg,
    })
}

fn mgr(host: Fake) -> Manager<Fake> {
    Manager::new(host)
}

fn host(m: &mut Manager<Fake>) -> &mut Fake {
    // The fake is owned by the manager; tests reach it through this accessor.
    m.host_mut()
}

#[test]
fn starts_hidden_and_never_touches_windows() {
    let m = mgr(Fake::default());
    assert!(OverlayKind::ALL.iter().all(|k| !m.is_shown(*k)));
}

#[test]
fn show_hide_reshow_reuses_the_window_and_raises() {
    let mut m = mgr(Fake::default());
    m.show(K);
    m.hide(K);
    m.show(K);
    let h = host(&mut m);
    assert_eq!(h.count("show reward"), 2);
    assert_eq!(h.count("hide reward"), 1);
    assert_eq!(h.count("raise reward"), 2);
    assert_eq!(h.count("show riven"), 0);
}

#[test]
fn hidden_window_is_click_through() {
    let mut m = mgr(Fake::default());
    m.show(K);
    m.set_interactive(K, true);
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward false");
    m.hide(K);
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward true");
}

#[test]
fn places_on_the_game_monitor_and_follows_it() {
    let mut m = mgr(Fake {
        game: game(1920, Foreground::Game),
        ..Default::default()
    });
    m.show(K);
    assert_eq!(host(&mut m).count("place reward 1920,0"), 1);
    m.tick();
    assert_eq!(
        host(&mut m).count("place"),
        1,
        "unchanged monitor is not re-placed"
    );
    host(&mut m).game = game(0, Foreground::Game);
    m.tick();
    assert_eq!(host(&mut m).count("place reward 0,0"), 1);
}

#[test]
fn falls_back_to_the_primary_monitor_without_the_game() {
    let mut m = mgr(Fake::default());
    m.show(K);
    assert_eq!(host(&mut m).count("place reward 0,0"), 1);
}

#[test]
fn tick_reasserts_topmost_only_when_lost() {
    let mut m = mgr(Fake::default());
    m.show(K);
    let before = host(&mut m).count("raise");
    m.tick();
    assert_eq!(host(&mut m).count("raise"), before);
    host(&mut m).topmost = false;
    m.tick();
    assert_eq!(host(&mut m).count("raise"), before + 1);
}

#[test]
fn hidden_windows_are_not_raised() {
    let mut m = mgr(Fake::default());
    m.tick();
    assert_eq!(host(&mut m).count("raise"), 0);
}

#[test]
fn hides_when_another_app_is_in_front_and_restores_on_return() {
    let mut m = mgr(Fake {
        game: game(0, Foreground::Game),
        ..Default::default()
    });
    m.show(K);
    host(&mut m).game = game(0, Foreground::Other);
    m.tick();
    assert_eq!(host(&mut m).count("hide reward"), 1);
    assert!(m.is_shown(K), "still wanted, only suspended");
    m.tick();
    assert_eq!(host(&mut m).count("hide reward"), 1, "no repeated hides");
    host(&mut m).game = game(0, Foreground::Game);
    m.tick();
    assert_eq!(host(&mut m).count("show reward"), 2);
}

#[test]
fn own_window_in_front_changes_nothing() {
    let mut m = mgr(Fake {
        game: game(0, Foreground::Game),
        ..Default::default()
    });
    m.show(K);
    host(&mut m).game = game(0, Foreground::Own);
    m.tick();
    assert_eq!(host(&mut m).count("hide"), 0);
    assert_eq!(host(&mut m).count("restore"), 0);
}

#[test]
fn overlay_holding_focus_hands_it_back_to_the_game() {
    let mut m = mgr(Fake {
        game: game(0, Foreground::Game),
        ..Default::default()
    });
    m.tick();
    host(&mut m).game = game(0, Foreground::Overlay);
    m.tick();
    assert_eq!(
        host(&mut m).count("restore"),
        0,
        "nothing visible, nothing to give back"
    );
    m.show(K);
    m.tick();
    assert_eq!(host(&mut m).count("restore"), 1);
}

#[test]
fn explicit_hide_wins_over_a_pending_restore() {
    let mut m = mgr(Fake {
        game: game(0, Foreground::Other),
        ..Default::default()
    });
    m.show(K);
    m.tick();
    m.hide(K);
    host(&mut m).game = game(0, Foreground::Game);
    m.tick();
    assert_eq!(host(&mut m).count("show reward"), 1);
}

#[test]
fn interactive_toggles_click_through_without_resending() {
    let mut m = mgr(Fake::default());
    m.show(K);
    m.set_interactive(K, true);
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward false");
    let n = host(&mut m).count("through");
    m.set_interactive(K, true);
    assert_eq!(host(&mut m).count("through"), n);
    m.set_interactive(K, false);
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward true");
}

#[test]
fn regions_take_clicks_only_under_the_cursor() {
    let mut m = mgr(Fake::default());
    m.show(K);
    m.set_regions(K, vec![r(100, 100, 50, 20)]);
    assert!(m.needs_fast_poll());
    host(&mut m).cursor = Some((120, 110));
    m.poll_cursor();
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward false");
    let n = host(&mut m).count("through");
    m.poll_cursor();
    assert_eq!(host(&mut m).count("through"), n, "no change, no call");
    host(&mut m).cursor = Some((150, 110));
    m.poll_cursor();
    assert_eq!(
        host(&mut m).last_through().unwrap(),
        "through reward true",
        "right edge is exclusive"
    );
}

#[test]
fn regions_survive_hide_and_only_take_clicks_while_visible() {
    let mut m = mgr(Fake::default());
    m.show(K);
    m.set_regions(K, vec![r(0, 0, 10, 10)]);
    m.hide(K);
    assert!(!m.needs_fast_poll());
    host(&mut m).cursor = Some((5, 5));
    m.poll_cursor();
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward true");
    m.show(K);
    assert!(m.needs_fast_poll());
    assert_eq!(host(&mut m).last_through().unwrap(), "through reward false");
}

#[test]
fn keep_mapped_blanks_instead_of_unmapping() {
    let mut m = mgr(Fake {
        keep: true,
        ..Default::default()
    });
    m.show(K);
    m.hide(K);
    m.show(K);
    let h = host(&mut m);
    assert_eq!(h.count("show reward"), 1, "mapped once");
    assert_eq!(h.count("hide"), 0);
    assert_eq!(h.count("content reward false"), 1);
    assert_eq!(h.count("content reward true"), 2);
}

#[test]
fn keep_mapped_hide_before_first_show_does_not_map() {
    let mut m = mgr(Fake {
        keep: true,
        ..Default::default()
    });
    m.hide(K);
    assert_eq!(host(&mut m).count("content"), 0);
}

#[test]
fn a_disabled_overlay_never_shows_and_a_shown_one_is_hidden_on_the_next_tick() {
    let mut m = mgr(Fake::default());
    host(&mut m).off = true;
    m.show(K);
    assert!(!m.is_shown(K));
    assert_eq!(host(&mut m).count("show"), 0);

    host(&mut m).off = false;
    m.show(K);
    host(&mut m).off = true;
    m.tick();
    assert!(!m.is_shown(K));
    assert_eq!(host(&mut m).count("hide reward"), 1);
}

#[test]
fn move_mode_shows_every_overlay_and_hides_only_the_ones_it_opened() {
    let mut m = mgr(Fake::default());
    m.show(OverlayKind::Arbi);
    m.start_move();
    assert!(m.is_moving());
    for kind in OverlayKind::ALL {
        assert!(m.is_shown(kind), "{}", kind.name());
    }
    m.set_regions(OverlayKind::Arbi, vec![r(0, 0, 10, 10)]);
    m.end_move();
    assert!(!m.is_moving());
    assert!(m.is_shown(OverlayKind::Arbi), "was up before moving");
    assert!(!m.needs_fast_poll(), "move regions are dropped");
    for kind in [OverlayKind::Reward, OverlayKind::Riven, OverlayKind::Trade] {
        assert!(!m.is_shown(kind), "{}", kind.name());
    }
}

#[test]
fn move_mode_skips_disabled_overlays() {
    let mut m = mgr(Fake {
        off: true,
        ..Fake::default()
    });
    m.start_move();
    assert!(OverlayKind::ALL.iter().all(|&k| !m.is_shown(k)));
    m.end_move();
}

#[test]
fn moving_keeps_overlays_visible_when_the_overlay_takes_the_foreground() {
    let mut m = mgr(Fake {
        game: game(0, Foreground::Game),
        ..Default::default()
    });
    m.start_move();
    for fg in [Foreground::Overlay, Foreground::Other] {
        host(&mut m).game = game(0, fg);
        m.tick();
    }
    let h = host(&mut m);
    assert_eq!(h.count("hide"), 0, "a panel being dragged must not vanish");
    assert_eq!(h.count("restore"), 0, "no focus fight mid-drag");
    m.end_move();
    assert_eq!(host(&mut m).count("restore"), 1);
}

#[test]
fn move_mode_grabs_focus_once_and_gives_it_back() {
    let mut m = mgr(Fake::default());
    m.start_move();
    m.start_move();
    assert_eq!(host(&mut m).count("grab reward"), 1);
    assert_eq!(host(&mut m).count("restore"), 0);
    m.end_move();
    assert_eq!(host(&mut m).count("restore"), 1);

    let mut off = mgr(Fake {
        off: true,
        ..Fake::default()
    });
    off.start_move();
    assert_eq!(
        host(&mut off).count("grab"),
        0,
        "nothing shown, nothing to focus"
    );
}

#[test]
fn every_shown_window_is_activatable_while_moving_and_restored_after() {
    let mut m = mgr(Fake::default());
    m.start_move();
    for k in OverlayKind::ALL {
        assert_eq!(
            host(&mut m).count(&format!("activatable {} true", k.name())),
            1
        );
    }
    m.end_move();
    for k in OverlayKind::ALL {
        assert_eq!(
            host(&mut m).count(&format!("activatable {} false", k.name())),
            1
        );
    }
}
