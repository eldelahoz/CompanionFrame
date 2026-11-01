//! Riven rolling-screen and chat-link state machine. Time comes in as `now_ms`
//! and the callbacks of the original become returned [`RivenEvent`]s, so it
//! needs no timers: the idle backstop is checked on every call and via `tick`.

use std::sync::LazyLock;

use regex::Regex;

macro_rules! rx {
    ($name:ident, $p:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($p).unwrap());
    };
}

rx!(
    SESSION_OPEN,
    r"Sys \[Info\]: Created /Lotus/Interface/OmegaRerollSelection\.swf"
);
rx!(
    SESSION_CLOSE,
    r"NpcManager::ClearAgents\(\) ReadyToCreateAgents = false"
);
// Matches any HudVis line - the number tracks increments/decrements.
rx!(
    HUD_VIS,
    r"ThemedDetailedPurchaseDialog\.lua: DBG: HudVis (\d+)"
);
// Two-step riven detection: PopulateInfo with a Randomized mod path confirms it's a riven.
rx!(
    POPULATE_RIVEN,
    r"ThemedDetailedPurchaseDialog\.lua: PopulateInfo->/Lotus/StoreItems/Upgrades/Mods/Randomized/"
);
rx!(
    CYCLE_CONFIRM_EN,
    r"Dialog::CreateOkCancel\(description=Are you sure you want to cycle (.+?) for ([\d,. ]+)\?"
);
rx!(
    CHOICE_CONFIRM_EN,
    r"Dialog::CreateOkCancel\(description=Cycle Riven into current selection\?"
);
rx!(GENERIC_DIALOG, r"Dialog::CreateOkCancel\(");
// Any language prints the cycle cost as the number right before the question mark.
rx!(
    DIALOG_COST,
    r"description=[^?]*?(\d[\d,. \x{a0}\x{202f}]*)\s*\?"
);
rx!(GENERIC_DIALOG_NON_INTERACTIVE, r"leftItem=nil");
rx!(SEND_RESULT, r"Dialog\.lua:\s*Dialog::SendResult\((\d+)\)");
rx!(
    DIORAMA_SETUP,
    r"(?i)OmegaRerollSelection\.lua.*Diorama setup"
);
// The roll-screen diorama loads the riven's weapon model right after the screen
// opens - the resource path names the exact weapon variant.
rx!(
    DIORAMA_WEAPON_LOAD,
    r"(?:ResourceLoader|Resloader|Resource load completed) 0x[0-9A-Fa-f]+ \((/Lotus/Weapons/[^)]+)\)"
);
// Extra close signal emitted by the recycled effects line.
rx!(RECYCLED_EFFECTS, r"ytes of recycled effects");

// Backstop for a missed close marker, not a screen-idle detector: only riven events reset it.
const SESSION_IDLE_TIMEOUT_MS: f64 = 600_000.0;
// Every match resets the idle timeout, so force-close at 30 minutes even while matches continue.
const SESSION_MAX_MS: f64 = 30.0 * 60_000.0;
const SEND_RESULT_COOLDOWN_MS: f64 = 400.0;
const GENERIC_DIALOG_COOLDOWN_MS: f64 = 600.0;
const CHOICE_DIALOG_COOLDOWN_MS: f64 = 2000.0;
// An active session absorbs the open burst, so this only guards a close racing it.
const SESSION_OPEN_COOLDOWN_MS: f64 = 2_000.0;
// Teardown markers trail up to ~2s behind the close they belong to.
const CLOSE_MARKER_SPACING_MS: f64 = 2_000.0;
const DIORAMA_DEDUP_MS: f64 = 2_000.0;
/// Accept the diorama weapon-load line this long after session open.
const WEAPON_PATH_WINDOW_MS: f64 = 15_000.0;
const FORCE_END_COOLDOWN_MS: f64 = 5_000.0;
const CHAT_RIVEN_POPULATE_WINDOW_MS: f64 = 2_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineSource {
    Dbwin,
    File,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RivenEvent {
    SessionOpen,
    SessionClose,
    ChatView,
    RollPending { weapon: String, kuva_per_roll: u32 },
    RollConfirmed,
    DioramaSetup,
    ChoiceConfirmed,
    WeaponPath(String),
}

#[derive(Debug, Default, PartialEq)]
pub struct RivenOutput {
    pub events: Vec<RivenEvent>,
    /// Whether the riven flow consumed a SendResult from this line.
    pub send_result_consumed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialog {
    RollConfirm,
    Choice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextDialog {
    Cycle,
    Choice,
}

pub struct RivenStateMachine {
    pending_dialog: Option<Dialog>,
    session_active: bool,
    session_started_at: f64,
    idle_deadline: Option<f64>,
    next_dialog: NextDialog,
    chat_view_active: bool,
    chat_hud_vis_level: u32,
    chat_weapon_path: Option<String>,
    last_send_result_at: f64,
    last_choice_dialog_at: f64,
    last_session_open_at: f64,
    last_close_marker_at: f64,
    last_close_dispatch_at: f64,
    last_diorama_at: f64,
    /// True once this session's diorama weapon load was reported (once per session).
    weapon_path_sent: bool,
    // Gate close patterns on diorama setup so loading-transition lines cannot close the screen.
    diorama_ready: bool,
    force_ended_at: f64,
    last_hud_vis: u32,
    last_hud_vis_increase_at: f64,
}

impl Default for RivenStateMachine {
    fn default() -> Self {
        Self {
            pending_dialog: None,
            session_active: false,
            session_started_at: 0.0,
            idle_deadline: None,
            next_dialog: NextDialog::Cycle,
            chat_view_active: false,
            chat_hud_vis_level: 0,
            chat_weapon_path: None,
            last_send_result_at: 0.0,
            last_choice_dialog_at: 0.0,
            last_session_open_at: 0.0,
            last_close_marker_at: 0.0,
            last_close_dispatch_at: 0.0,
            last_diorama_at: 0.0,
            weapon_path_sent: false,
            diorama_ready: false,
            force_ended_at: 0.0,
            last_hud_vis: 0,
            last_hud_vis_increase_at: 0.0,
        }
    }
}

impl RivenStateMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_session_active(&self) -> bool {
        self.session_active
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn end_session_state(&mut self) {
        self.session_active = false;
        self.session_started_at = 0.0;
        self.diorama_ready = false;
        self.pending_dialog = None;
        self.next_dialog = NextDialog::Cycle;
        self.weapon_path_sent = false;
        // A closed session must not leak its cooldowns into the next open.
        self.last_session_open_at = 0.0;
        self.last_diorama_at = 0.0;
        self.idle_deadline = None;
    }

    fn reset_idle_timer(&mut self, now: f64) {
        self.idle_deadline = Some(now + SESSION_IDLE_TIMEOUT_MS);
    }

    /// Fires the idle backstop when its deadline has passed: closes the overlay too, not just state.
    pub fn tick(&mut self, now_ms: f64) -> Vec<RivenEvent> {
        if self.idle_deadline.is_some_and(|d| now_ms >= d) {
            self.end_session_state();
            return vec![RivenEvent::SessionClose];
        }
        Vec::new()
    }

    /// Force-closes an overlong session and reports whether this call closed it.
    fn force_end_if_expired(&mut self, now: f64) -> bool {
        if !self.session_active || self.session_started_at == 0.0 {
            return false;
        }
        if now - self.session_started_at < SESSION_MAX_MS {
            return false;
        }
        self.end_session_state();
        self.force_ended_at = now;
        true
    }

    /// Re-arm over a rolling screen whose open marker is gone; a live session keeps its dialog state.
    pub fn resume_session(&mut self, now_ms: f64) {
        let was_active = self.session_active;
        self.session_active = true;
        self.session_started_at = now_ms;
        self.diorama_ready = true;
        self.force_ended_at = 0.0;
        if !was_active {
            self.pending_dialog = None;
            self.next_dialog = NextDialog::Cycle;
            self.chat_view_active = false;
        }
        self.reset_idle_timer(now_ms);
    }

    /// The overlay was dismissed externally.
    pub fn force_end_session(&mut self, now_ms: f64) {
        if !self.session_active && self.pending_dialog.is_none() && !self.chat_view_active {
            return;
        }
        self.session_active = false;
        self.session_started_at = 0.0;
        self.diorama_ready = false;
        self.chat_view_active = false;
        self.chat_hud_vis_level = 0;
        self.chat_weapon_path = None;
        self.pending_dialog = None;
        self.next_dialog = NextDialog::Cycle;
        self.force_ended_at = now_ms;
        self.last_hud_vis = 0;
        self.last_hud_vis_increase_at = 0.0;
        self.idle_deadline = None;
    }

    pub fn process(
        &mut self,
        line: &str,
        source: LineSource,
        realtime_active: bool,
        now_ms: f64,
    ) -> RivenOutput {
        let now = now_ms;
        let mut out = RivenOutput {
            events: self.tick(now),
            send_result_consumed: false,
        };
        let skip_from_file = realtime_active && source == LineSource::File;

        // Check before extending the session so continuous matches cannot bypass the cap.
        if self.force_end_if_expired(now) {
            out.events.push(RivenEvent::SessionClose);
        }

        if !skip_from_file
            && SESSION_OPEN.is_match(line)
            && !self.session_active
            && now - self.last_session_open_at >= SESSION_OPEN_COOLDOWN_MS
        {
            self.last_session_open_at = now;
            self.session_active = true;
            self.session_started_at = now;
            self.chat_view_active = false;
            self.chat_hud_vis_level = 0;
            self.chat_weapon_path = None;
            self.next_dialog = NextDialog::Cycle;
            self.pending_dialog = None;
            self.weapon_path_sent = false;
            self.reset_idle_timer(now);
            out.events.push(RivenEvent::SessionOpen);
        }

        // The short open window excludes relay bystanders while allowing the file poll's delayed
        // exact-variant path after DBWIN has already marked the diorama ready.
        let weapon_match = DIORAMA_WEAPON_LOAD.captures(line).map(|c| c[1].to_string());
        if self.session_active
            && !self.weapon_path_sent
            && self.session_started_at > 0.0
            && now - self.session_started_at < WEAPON_PATH_WINDOW_MS
            && let Some(path) = &weapon_match
        {
            self.weapon_path_sent = true;
            out.events.push(RivenEvent::WeaponPath(path.clone()));
        }

        // Accept readiness from either source so DBWIN/file ordering cannot block roll OCR.
        if self.session_active
            && DIORAMA_SETUP.is_match(line)
            && now - self.last_diorama_at >= DIORAMA_DEDUP_MS
        {
            self.last_diorama_at = now;
            self.diorama_ready = true;
            self.reset_idle_timer(now);
            out.events.push(RivenEvent::DioramaSetup);
        }

        // Require diorama readiness because these close lines also fire during screen loading.
        if !skip_from_file
            && self.session_active
            && self.diorama_ready
            && (SESSION_CLOSE.is_match(line) || RECYCLED_EFFECTS.is_match(line))
        {
            // Spacing gate is real-time only - file polls deliver batched, stale timestamps.
            // The chain cap stops sub-2s marker streams from starving the close forever.
            let trailing = realtime_active
                && now - self.last_close_marker_at < CLOSE_MARKER_SPACING_MS
                && now - self.last_close_dispatch_at < 2.0 * CLOSE_MARKER_SPACING_MS;
            self.last_close_marker_at = now;
            if !trailing {
                self.last_close_dispatch_at = now;
                self.end_session_state();
                out.events.push(RivenEvent::SessionClose);
            }
        }

        if !skip_from_file
            && !self.session_active
            && let Some(c) = HUD_VIS.captures(line)
        {
            let new_vis: u32 = c[1].parse().unwrap_or(0);
            if self.chat_view_active && new_vis < self.chat_hud_vis_level {
                self.chat_view_active = false;
                self.chat_hud_vis_level = 0;
                self.chat_weapon_path = None;
                self.last_hud_vis_increase_at = 0.0;
                out.events.push(RivenEvent::SessionClose);
            } else if new_vis > self.last_hud_vis {
                if !self.chat_view_active {
                    self.chat_weapon_path = None;
                }
                self.last_hud_vis_increase_at = now;
            } else if !self.chat_view_active && new_vis < self.last_hud_vis {
                self.chat_weapon_path = None;
                self.last_hud_vis_increase_at = 0.0;
            }
            self.last_hud_vis = new_vis;
        }

        let in_populate_window = self.last_hud_vis_increase_at > 0.0
            && now - self.last_hud_vis_increase_at < CHAT_RIVEN_POPULATE_WINDOW_MS;
        let chat_idle = !skip_from_file && !self.session_active && !self.chat_view_active;

        if chat_idle
            && in_populate_window
            && let Some(path) = &weapon_match
        {
            self.chat_weapon_path = Some(path.clone());
        }

        // PopulateInfo with Randomized mod path within 2s of HudVis increase = riven
        if chat_idle
            && self.last_hud_vis_increase_at > 0.0
            && POPULATE_RIVEN.is_match(line)
            && in_populate_window
        {
            let weapon_path = self.chat_weapon_path.take();
            self.chat_view_active = true;
            self.chat_hud_vis_level = self.last_hud_vis;
            out.events.push(RivenEvent::ChatView);
            if let Some(path) = weapon_path {
                out.events.push(RivenEvent::WeaponPath(path));
            }
        }

        let recently_force_ended =
            |s: &Self| !s.session_active && now - s.force_ended_at < FORCE_END_COOLDOWN_MS;
        let mut dialog_handled = skip_from_file;

        if !skip_from_file
            && let Some(c) = CYCLE_CONFIRM_EN.captures(line)
            && !recently_force_ended(self)
        {
            dialog_handled = true;
            self.session_active = true;
            self.reset_idle_timer(now);
            self.pending_dialog = Some(Dialog::RollConfirm);
            let cost = c[2].replace([',', '.', ' '], "").parse().unwrap_or(0);
            out.events.push(RivenEvent::RollPending {
                weapon: c[1].trim().to_string(),
                kuva_per_roll: cost,
            });
        }

        if !dialog_handled
            && !skip_from_file
            && CHOICE_CONFIRM_EN.is_match(line)
            && !recently_force_ended(self)
        {
            dialog_handled = true;
            self.session_active = true;
            self.reset_idle_timer(now);
            self.pending_dialog = Some(Dialog::Choice);
            if now - self.last_choice_dialog_at >= CHOICE_DIALOG_COOLDOWN_MS {
                self.last_choice_dialog_at = now;
            }
        }

        // Layer 2: generic fallback
        if !dialog_handled
            && self.session_active
            && self.pending_dialog.is_none()
            && now - self.last_send_result_at >= GENERIC_DIALOG_COOLDOWN_MS
            && GENERIC_DIALOG.is_match(line)
            && !GENERIC_DIALOG_NON_INTERACTIVE.is_match(line)
        {
            self.reset_idle_timer(now);
            if self.next_dialog == NextDialog::Cycle {
                self.pending_dialog = Some(Dialog::RollConfirm);
                let cost = DIALOG_COST.captures(line).map_or(0, |c| {
                    c[1].chars()
                        .filter(char::is_ascii_digit)
                        .collect::<String>()
                        .parse()
                        .unwrap_or(0)
                });
                out.events.push(RivenEvent::RollPending {
                    weapon: String::new(),
                    kuva_per_roll: cost,
                });
            } else {
                self.pending_dialog = Some(Dialog::Choice);
            }
        }

        if let Some(c) = SEND_RESULT.captures(line) {
            // Even when skipping riven from file poll, mark as consumed so it doesn't
            // leak to the relic picker close handler.
            if self.session_active && skip_from_file {
                out.send_result_consumed = true;
            }
            if !skip_from_file && (self.pending_dialog.is_some() || self.session_active) {
                out.send_result_consumed = true;
                if self.session_active {
                    self.reset_idle_timer(now);
                }
                if let Some(dialog) = self.pending_dialog.take()
                    && &c[1] == "4"
                    && now - self.last_send_result_at >= SEND_RESULT_COOLDOWN_MS
                {
                    self.last_send_result_at = now;
                    match dialog {
                        Dialog::RollConfirm => {
                            self.next_dialog = NextDialog::Choice;
                            out.events.push(RivenEvent::RollConfirmed);
                        }
                        Dialog::Choice => {
                            self.next_dialog = NextDialog::Cycle;
                            out.events.push(RivenEvent::ChoiceConfirmed);
                        }
                    }
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests;
