//! Overlay window state machine, free of Win32/Tauri so `cargo test` can drive it with a fake [`Host`].
//!
//! The four windows are created once and only shown/hidden afterwards. Input is click-through
//! unless the window is interactive or the cursor is over one of its reported regions; the game
//! keeps keyboard focus throughout because the windows are `WS_EX_NOACTIVATE`.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayKind {
    Reward,
    Riven,
    Arbi,
    Trade,
}

impl OverlayKind {
    pub const ALL: [OverlayKind; 4] = [Self::Reward, Self::Riven, Self::Arbi, Self::Trade];

    pub fn name(self) -> &'static str {
        match self {
            Self::Reward => "reward",
            Self::Riven => "riven",
            Self::Arbi => "arbi",
            Self::Trade => "trade",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// Physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
pub struct OverlayRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl OverlayRect {
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foreground {
    Game,
    /// One of our overlay windows: the game lost focus to it and should get it back.
    Overlay,
    /// Our main window: leave everything as it is.
    Own,
    Other,
}

#[derive(Debug, Clone, Copy)]
pub struct Game {
    pub monitor: OverlayRect,
    pub foreground: Foreground,
}

pub trait Host: Send {
    fn show(&mut self, kind: OverlayKind);
    fn hide(&mut self, kind: OverlayKind);
    fn set_click_through(&mut self, kind: OverlayKind, on: bool);
    fn place(&mut self, kind: OverlayKind, rect: OverlayRect);
    fn is_topmost(&self, kind: OverlayKind) -> bool;
    fn raise(&mut self, kind: OverlayKind);
    fn set_content_visible(&mut self, kind: OverlayKind, visible: bool);
    /// `None` when the game is not running (or cannot be told).
    fn game(&mut self) -> Option<Game>;
    fn primary_monitor(&self) -> Option<OverlayRect>;
    fn cursor(&self) -> Option<(i32, i32)>;
    fn restore_game_focus(&mut self);
    /// Move mode: an activatable window takes the foreground when clicked instead of the game
    /// (WFHelper's focusable overlays); every window is reset when moving ends.
    fn set_activatable(&mut self, _kind: OverlayKind, _on: bool) {}
    /// Move mode: the game captures the mouse while focused, so one overlay takes the foreground.
    /// Ended by `restore_game_focus`.
    fn take_focus_for_move(&mut self, _kind: OverlayKind) {}
    /// Blank the page instead of unmapping the window.
    fn keep_mapped(&self) -> bool;
    /// The user's per-overlay switch; read on every show so a settings change needs no restart.
    fn enabled(&self, _kind: OverlayKind) -> bool {
        true
    }
}

#[derive(Default)]
struct Slot {
    mapped: bool,
    shown: bool,
    suspended: bool,
    interactive: bool,
    regions: Vec<OverlayRect>,
    click_through: Option<bool>,
    placed: Option<OverlayRect>,
}

impl Slot {
    fn visible(&self) -> bool {
        self.shown && !self.suspended
    }

    fn tracks_cursor(&self) -> bool {
        self.visible() && !self.interactive && !self.regions.is_empty()
    }
}

pub struct Manager<H: Host> {
    host: H,
    slots: [Slot; 4],
    /// While moving: which kinds were already shown before, so ending hides only the rest.
    moving: Option<[bool; 4]>,
    /// Last foreground class seen by `tick`, so move-mode logging only reports changes.
    last_fg: Option<Foreground>,
}

impl<H: Host> Manager<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            slots: Default::default(),
            moving: None,
            last_fg: None,
        }
    }

    #[cfg(test)]
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    pub fn is_shown(&self, kind: OverlayKind) -> bool {
        self.slots[kind.index()].shown
    }

    pub fn is_moving(&self) -> bool {
        self.moving.is_some()
    }

    /// Move mode: every enabled overlay is shown so its panel can be dragged (each page reports
    /// its panel as a clickable region, so the stacked full-monitor windows do not cover each other).
    pub fn start_move(&mut self) {
        if self.moving.is_some() {
            return;
        }
        self.moving = Some(std::array::from_fn(|i| self.slots[i].shown));
        for kind in OverlayKind::ALL {
            self.show(kind);
        }
        if let Some(kind) = OverlayKind::ALL
            .into_iter()
            .find(|k| self.slots[k.index()].shown)
        {
            self.host.take_focus_for_move(kind);
        }
    }

    /// Locks the overlays again: the ones shown only for moving go away, the others drop the
    /// move regions (a page re-reports its own, e.g. the trade popup's box).
    pub fn end_move(&mut self) {
        let Some(before) = self.moving.take() else {
            return;
        };
        for kind in OverlayKind::ALL {
            self.host.set_activatable(kind, false);
        }
        self.host.restore_game_focus();
        for kind in OverlayKind::ALL {
            if before[kind.index()] {
                self.slots[kind.index()].interactive = false;
                self.set_regions(kind, Vec::new());
            } else {
                self.hide(kind);
            }
        }
    }

    pub fn show(&mut self, kind: OverlayKind) {
        if !self.host.enabled(kind) {
            return;
        }
        let s = &mut self.slots[kind.index()];
        s.shown = true;
        s.suspended = false;
        self.present(kind);
    }

    pub fn hide(&mut self, kind: OverlayKind) {
        let keep = self.host.keep_mapped();
        let s = &mut self.slots[kind.index()];
        s.shown = false;
        s.suspended = false;
        s.interactive = false;
        // Regions stay: the page only re-sends them when its boxes change, and they are ignored while hidden.
        if keep && s.mapped {
            self.host.set_content_visible(kind, false);
        } else {
            s.mapped = false;
            self.host.hide(kind);
        }
        self.apply_input(kind, true);
    }

    pub fn set_interactive(&mut self, kind: OverlayKind, on: bool) {
        self.slots[kind.index()].interactive = on;
        self.apply_input(kind, false);
    }

    /// Regions in physical screen pixels that take clicks while the rest stays click-through.
    pub fn set_regions(&mut self, kind: OverlayKind, regions: Vec<OverlayRect>) {
        let s = &mut self.slots[kind.index()];
        if self.moving.is_some() && s.regions != regions {
            tracing::info!(
                "overlay {} regions: {} (first {:?})",
                kind.name(),
                regions.len(),
                regions.first()
            );
        }
        s.regions = regions;
        self.apply_input(kind, false);
    }

    /// True while some visible window has regions, i.e. the cursor must be sampled quickly.
    pub fn needs_fast_poll(&self) -> bool {
        self.slots.iter().any(Slot::tracks_cursor)
    }

    /// Cheap cursor sample: flips click-through only when the cursor enters/leaves a region.
    pub fn poll_cursor(&mut self) {
        for kind in OverlayKind::ALL {
            if self.slots[kind.index()].tracks_cursor() {
                self.apply_input(kind, false);
            }
        }
    }

    /// Slow pass: follows the game's monitor and focus, and re-asserts topmost.
    pub fn tick(&mut self) {
        for kind in OverlayKind::ALL {
            if self.slots[kind.index()].shown && !self.host.enabled(kind) {
                self.hide(kind);
            }
        }
        if let Some(game) = self.host.game() {
            if self.moving.is_some() && self.last_fg != Some(game.foreground) {
                tracing::info!("overlay foreground: {:?}", game.foreground);
            }
            self.last_fg = Some(game.foreground);
            for kind in OverlayKind::ALL {
                if self.slots[kind.index()].shown {
                    self.place(kind, game.monitor);
                }
            }
            // Moving: a press on a panel may hand the foreground to the overlay (or a webview helper
            // window); that must neither hide the panel being dragged nor steal focus back mid-drag.
            let foreground = match game.foreground {
                Foreground::Own => Foreground::Own,
                _ if self.moving.is_some() => Foreground::Game,
                f => f,
            };
            match foreground {
                Foreground::Game => {
                    for kind in OverlayKind::ALL {
                        let s = &mut self.slots[kind.index()];
                        if s.shown && s.suspended {
                            s.suspended = false;
                            self.present(kind);
                        }
                    }
                }
                Foreground::Overlay => {
                    if self.slots.iter().any(Slot::visible) {
                        self.host.restore_game_focus();
                    }
                }
                Foreground::Own => {}
                Foreground::Other => {
                    for kind in OverlayKind::ALL {
                        let s = &mut self.slots[kind.index()];
                        if s.visible() {
                            s.suspended = true;
                            self.suspend(kind);
                        }
                    }
                }
            }
        }
        for kind in OverlayKind::ALL {
            if self.slots[kind.index()].visible() && !self.host.is_topmost(kind) {
                self.host.raise(kind);
            }
        }
    }

    /// The monitor overlays are drawn on: the game's, else the primary one.
    pub fn target_monitor(&mut self) -> Option<OverlayRect> {
        self.host
            .game()
            .map(|g| g.monitor)
            .or_else(|| self.host.primary_monitor())
    }

    fn present(&mut self, kind: OverlayKind) {
        if let Some(r) = self.target_monitor() {
            self.place(kind, r);
        }
        let keep = self.host.keep_mapped();
        if keep {
            self.host.set_content_visible(kind, true);
        }
        if !(keep && self.slots[kind.index()].mapped) {
            self.host.show(kind);
        }
        self.slots[kind.index()].mapped = true;
        if self.moving.is_some() {
            self.host.set_activatable(kind, true);
        }
        self.host.raise(kind);
        self.apply_input(kind, true);
    }

    fn suspend(&mut self, kind: OverlayKind) {
        if self.host.keep_mapped() {
            self.host.set_content_visible(kind, false);
        } else {
            self.slots[kind.index()].mapped = false;
            self.host.hide(kind);
        }
        self.apply_input(kind, true);
    }

    fn place(&mut self, kind: OverlayKind, rect: OverlayRect) {
        let s = &mut self.slots[kind.index()];
        if s.placed != Some(rect) {
            s.placed = Some(rect);
            self.host.place(kind, rect);
        }
    }

    /// Recomputes the input mode; `force` re-sends it even if unchanged (a style set before the
    /// window is mapped can be lost).
    fn apply_input(&mut self, kind: OverlayKind, force: bool) {
        let cursor = self.host.cursor();
        let s = &mut self.slots[kind.index()];
        let over = cursor.is_some_and(|(x, y)| s.regions.iter().any(|r| r.contains(x, y)));
        let through = !(s.visible() && (s.interactive || over));
        if force || s.click_through != Some(through) {
            if self.moving.is_some() && s.click_through != Some(through) {
                tracing::info!(
                    "overlay {} click-through {}",
                    kind.name(),
                    if through { "on" } else { "off" }
                );
            }
            s.click_through = Some(through);
            self.host.set_click_through(kind, through);
        }
    }
}
