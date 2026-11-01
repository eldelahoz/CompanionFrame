//! Decides what the keyboard hook does with each key event. Pure so it can be tested
//! without a hook; the platform layer only feeds it events and obeys the answer.

use tokio::sync::oneshot;

use super::accelerator::{Accel, Mods, is_modifier_vk};

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Pass,
    /// Hide the key from the foreground app without any event.
    Swallow,
    Fire {
        id: String,
        swallow: bool,
    },
}

const VK_ESCAPE: u32 = 0x1B;

#[derive(Default)]
pub struct Matcher {
    watch: Vec<(String, Accel)>,
    /// Auto-repeat sends key-downs while held; only the first one fires.
    held: Option<u32>,
    capture: Option<oneshot::Sender<Option<Accel>>>,
}

impl Matcher {
    pub fn set_watch(&mut self, watch: Vec<(String, Accel)>) {
        self.watch = watch;
    }

    /// The next non-modifier key-down (any app) is sent back instead of acting; bare Esc sends `None`.
    pub fn arm_capture(&mut self, tx: oneshot::Sender<Option<Accel>>) {
        self.capture = Some(tx);
    }

    pub fn cancel_capture(&mut self) {
        self.capture = None;
    }

    /// `in_game` / `mods` are lazy: they cost a syscall and most keys never need them.
    /// `any_window` (dev override) fires without requiring Warframe, but never swallows.
    pub fn on_key(
        &mut self,
        vk: u32,
        down: bool,
        mods: impl FnOnce() -> Mods,
        in_game: impl FnOnce() -> bool,
        any_window: bool,
    ) -> Action {
        if !down {
            if self.held == Some(vk) {
                self.held = None;
            }
            return Action::Pass;
        }
        if !is_modifier_vk(vk)
            && let Some(tx) = self.capture.take()
        {
            let m = mods();
            let _ =
                tx.send((vk != VK_ESCAPE || m != Mods::default()).then_some(Accel { vk, mods: m }));
            self.held = Some(vk);
            return Action::Swallow;
        }
        if !self.watch.iter().any(|(_, a)| a.vk == vk) {
            return Action::Pass;
        }
        let m = mods();
        let Some((id, _)) = self.watch.iter().find(|(_, a)| a.vk == vk && a.mods == m) else {
            return Action::Pass;
        };
        let swallow = in_game();
        if !swallow && !any_window {
            return Action::Pass;
        }
        if self.held == Some(vk) {
            return if swallow {
                Action::Swallow
            } else {
                Action::Pass
            };
        }
        self.held = Some(vk);
        Action::Fire {
            id: id.clone(),
            swallow,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::hotkeys::accelerator::parse;

    const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
    };
    const CTRL: Mods = Mods { ctrl: true, ..NONE };

    fn matcher() -> Matcher {
        let mut m = Matcher::default();
        m.set_watch(vec![
            ("overlay".into(), parse("F8").unwrap()),
            ("rescan".into(), parse("Ctrl+F6").unwrap()),
        ]);
        m
    }

    fn key(m: &mut Matcher, vk: u32, down: bool, mods: Mods, in_game: bool) -> Action {
        m.on_key(vk, down, || mods, || in_game, false)
    }

    fn fire(id: &str, swallow: bool) -> Action {
        Action::Fire {
            id: id.into(),
            swallow,
        }
    }

    #[test]
    fn fires_and_swallows_only_with_the_game_in_front() {
        let mut m = matcher();
        assert_eq!(key(&mut m, 0x77, true, NONE, true), fire("overlay", true));
        assert_eq!(key(&mut m, 0x77, true, NONE, false), Action::Pass);
    }

    #[test]
    fn other_apps_never_fire_or_lose_the_key() {
        let mut m = matcher();
        assert_eq!(key(&mut m, 0x77, true, NONE, false), Action::Pass);
        // and the failed attempt did not arm the repeat guard
        assert_eq!(key(&mut m, 0x77, true, NONE, true), fire("overlay", true));
    }

    #[test]
    fn modifiers_must_match_exactly() {
        let mut m = matcher();
        assert_eq!(
            key(&mut m, 0x75, true, NONE, true),
            Action::Pass,
            "F6 without Ctrl"
        );
        assert_eq!(key(&mut m, 0x77, true, CTRL, true), Action::Pass, "Ctrl+F8");
        assert_eq!(key(&mut m, 0x75, true, CTRL, true), fire("rescan", true));
    }

    #[test]
    fn auto_repeat_fires_once_but_stays_swallowed() {
        let mut m = matcher();
        assert_eq!(key(&mut m, 0x77, true, NONE, true), fire("overlay", true));
        assert_eq!(key(&mut m, 0x77, true, NONE, true), Action::Swallow);
        assert_eq!(key(&mut m, 0x77, true, NONE, true), Action::Swallow);
        assert_eq!(key(&mut m, 0x77, false, NONE, true), Action::Pass);
        assert_eq!(key(&mut m, 0x77, true, NONE, true), fire("overlay", true));
    }

    #[test]
    fn unwatched_keys_skip_the_syscalls() {
        let mut m = matcher();
        let a = m.on_key(0x41, true, || panic!("mods"), || panic!("in_game"), false);
        assert_eq!(a, Action::Pass);
    }

    #[test]
    fn any_window_override_fires_without_swallowing() {
        let mut m = matcher();
        assert_eq!(
            m.on_key(0x77, true, || NONE, || false, true),
            fire("overlay", false)
        );
        assert_eq!(m.on_key(0x77, true, || NONE, || false, true), Action::Pass);
    }

    #[test]
    fn watch_can_be_replaced_live() {
        let mut m = matcher();
        m.set_watch(vec![("overlay".into(), parse("F9").unwrap())]);
        assert_eq!(key(&mut m, 0x77, true, NONE, true), Action::Pass);
        assert_eq!(key(&mut m, 0x78, true, NONE, true), fire("overlay", true));
    }

    #[test]
    fn capture_takes_the_next_real_key_with_its_modifiers() {
        let mut m = matcher();
        let (tx, mut rx) = oneshot::channel();
        m.arm_capture(tx);
        assert_eq!(
            key(&mut m, 0x11, true, CTRL, false),
            Action::Pass,
            "a modifier alone is ignored"
        );
        assert_eq!(key(&mut m, 0x58, true, CTRL, false), Action::Swallow);
        assert_eq!(rx.try_recv().unwrap(), Some(parse("Ctrl+X").unwrap()));
        assert_eq!(
            key(&mut m, 0x41, true, NONE, false),
            Action::Pass,
            "one-shot"
        );
    }

    #[test]
    fn capture_escape_cancels_and_cancel_disarms() {
        let mut m = matcher();
        let (tx, mut rx) = oneshot::channel();
        m.arm_capture(tx);
        key(&mut m, 0x1B, true, NONE, false);
        assert_eq!(rx.try_recv().unwrap(), None);

        let (tx, _rx) = oneshot::channel();
        m.arm_capture(tx);
        m.cancel_capture();
        assert_eq!(key(&mut m, 0x58, true, CTRL, false), Action::Pass);
    }
}
