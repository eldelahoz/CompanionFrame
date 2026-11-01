//! Time-aware line helpers. Every method takes `now` (ms) from the caller so the
//! logic stays pure and testable.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

static UPTIME_PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)\.(\d{3}) ").unwrap());

/// Estimate file-line delay from the minimum wall-clock minus game-uptime offset.
#[derive(Default)]
pub struct EeUptimeTracker {
    offset_ms: Option<f64>,
    last_uptime_ms: f64,
    last_staleness_ms: f64,
}

impl EeUptimeTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Return line staleness; unstamped rows inherit the current batch delay.
    pub fn observe(&mut self, line: &str, now_ms: f64) -> f64 {
        let Some(m) = UPTIME_PREFIX.captures(line) else {
            return self.last_staleness_ms;
        };
        let uptime_ms =
            m[1].parse::<f64>().unwrap_or(0.0) * 1000.0 + m[2].parse::<f64>().unwrap_or(0.0);
        if uptime_ms < self.last_uptime_ms - 5_000.0 {
            self.offset_ms = None; // game restarted
        }
        self.last_uptime_ms = uptime_ms;

        let offset = now_ms - uptime_ms;
        let base = match self.offset_ms {
            Some(o) if o <= offset => o,
            _ => {
                self.offset_ms = Some(offset);
                offset
            }
        };
        self.last_staleness_ms = (offset - base).max(0.0);
        self.last_staleness_ms
    }
}

const PRUNE_ABOVE: usize = 64;

/// A key seen again within `window_ms` of its last sighting is an echo; every sighting counts.
pub struct EchoFilter {
    window_ms: f64,
    last_seen: HashMap<String, f64>,
}

impl EchoFilter {
    pub fn new(window_ms: f64) -> Self {
        Self {
            window_ms,
            last_seen: HashMap::new(),
        }
    }

    pub fn is_echo(&mut self, key: &str, now: f64) -> bool {
        let previous = self.last_seen.insert(key.to_string(), now);
        if self.last_seen.len() > PRUNE_ABOVE {
            let window = self.window_ms;
            self.last_seen.retain(|_, at| now - *at < window);
        }
        previous.is_some_and(|p| now - p < self.window_ms)
    }

    pub fn clear(&mut self) {
        self.last_seen.clear();
    }
}

// Lowercase to allow a single case-insensitive check without regex cost.
const FILTER_SUBSTRINGS_LOWER: [&str; 23] = [
    "loadingcompleteend",
    "populateinventorygrid",
    "initmapping",
    "dialog::sendresult",
    "pause countdown done",
    "got rewards",
    "projectionrewardchoice.lua",
    "omegarerollselection.swf",
    "diorama setup",
    "npcmanager::clearagents",
    "recycled effects",
    "dialog::createokcancel",
    // The trade result in any language: only the dialog kind is stable.
    "dialog::createok(",
    "themeddetailedpurchasedialog",
    "tradingpost.lua",
    "you are offering",
    "the trade was successful",
    "chatredux::addtab",
    "mainmenu::logindone",
    "eom missionlocationunlocked=",
    "topmenu.lua: abort:",
    "syncautopopulatedconsumables for mission",
    "onstatestarted, mission type=",
];

// Match the monitor's picker cooldown because the game repeats these lines.
const RELIC_PICKER_SUPPRESS_MS: f64 = 7500.0;
// One AddTab forward per window is enough; also guards re-delivery regressions.
const CHAT_TAB_SUPPRESS_MS: f64 = 2000.0;
const CHAT_RIVEN_WEAPON_WINDOW_MS: f64 = 2000.0;
const REROLL_WEAPON_WINDOW_MS: f64 = 15_000.0;

/// Prefilter for real-time sources: drops lines no downstream parser wants.
#[derive(Default)]
pub struct DebugLineGate {
    relic_suppress_until: f64,
    chat_tab_suppress_until: f64,
    riven_weapon_load_until: f64,
}

impl DebugLineGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when the line should be forwarded to the line parsers.
    pub fn wants(&mut self, msg: &str, now: f64) -> bool {
        let lower = msg.to_lowercase();
        let is_weapon_load = lower.contains("/lotus/weapons/")
            && (lower.contains("resourceloader")
                || lower.contains("resloader")
                || lower.contains("resource load completed"));
        if lower.contains("omegarerollselection.swf") {
            self.riven_weapon_load_until = now + REROLL_WEAPON_WINDOW_MS;
        } else if lower.contains("themeddetailedpurchasedialog") && lower.contains("hudvis") {
            self.riven_weapon_load_until = now + CHAT_RIVEN_WEAPON_WINDOW_MS;
        }
        if is_weapon_load && now >= self.riven_weapon_load_until {
            return false;
        }
        if !is_weapon_load && !FILTER_SUBSTRINGS_LOWER.iter().any(|s| lower.contains(s)) {
            return false;
        }

        if lower.contains("loadingcompleteend") || lower.contains("populateinventorygrid") {
            if now < self.relic_suppress_until {
                return false;
            }
            self.relic_suppress_until = now + RELIC_PICKER_SUPPRESS_MS;
        }
        if lower.contains("chatredux::addtab") {
            if now < self.chat_tab_suppress_until {
                return false;
            }
            self.chat_tab_suppress_until = now + CHAT_TAB_SUPPRESS_MS;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(uptime: &str) -> String {
        format!("{uptime} Sys [Info]: something")
    }

    #[test]
    fn uptime_reports_near_zero_staleness_while_lines_arrive_promptly() {
        let mut t = EeUptimeTracker::new();
        assert_eq!(t.observe(&line("100.000"), 1_000_100_000.0), 0.0);
        assert_eq!(t.observe(&line("101.000"), 1_000_101_050.0), 50.0);
    }

    #[test]
    fn uptime_measures_a_delayed_flush_batch_against_the_tightest_offset() {
        let mut t = EeUptimeTracker::new();
        t.observe(&line("100.000"), 1_000_100_000.0);
        // Generated at uptime 110s but flushed 10s late.
        assert_eq!(t.observe(&line("110.000"), 1_000_120_000.0), 10_000.0);
    }

    #[test]
    fn uptime_unstamped_lines_inherit_the_batch_staleness() {
        let mut t = EeUptimeTracker::new();
        t.observe(&line("100.000"), 1_000_100_000.0);
        t.observe(&line("110.000"), 1_000_120_000.0);
        assert_eq!(
            t.observe(r#"    "activeMissionTag" : "VoidT2","#, 1_000_120_001.0),
            10_000.0
        );
    }

    #[test]
    fn uptime_resets_the_offset_when_the_game_restarts() {
        let mut t = EeUptimeTracker::new();
        t.observe(&line("5000.000"), 1_000_000_000.0);
        assert_eq!(t.observe(&line("10.000"), 1_000_300_000.0), 0.0);
        assert_eq!(t.observe(&line("11.000"), 1_000_301_000.0), 0.0);
    }

    #[test]
    fn echo_filter_flags_a_repeat_inside_the_window_and_counts_every_sighting() {
        let mut f = EchoFilter::new(30_000.0);
        assert!(!f.is_echo("a", 0.0));
        assert!(f.is_echo("a", 20_000.0));
        // The sighting at 20s restarted the window.
        assert!(f.is_echo("a", 45_000.0));
        assert!(!f.is_echo("b", 45_000.0));
        assert!(!f.is_echo("a", 80_000.0));
    }

    const RELIC_LINE: &str = "Script [Info]: ThemedProjectionManager.lua: LoadingCompleteEnd";
    const RELIC_FALLBACK: &str =
        "Script [Info]: ThemedProjectionManager.lua: PopulateInventoryGrid";
    const CHAT_LINE: &str =
        "Sys [Info]: ChatRedux::AddTab: Adding tab with channel name: FUser to index 3";
    const REWARD_LINE: &str = "Sys [Info]: Pause countdown done";

    #[test]
    fn gate_drops_unmatched_lines_and_matches_case_insensitively() {
        let mut g = DebugLineGate::new();
        assert!(!g.wants("Sys [Info]: some unrelated engine noise", 0.0));
        assert!(g.wants(&REWARD_LINE.to_uppercase(), 0.0));
    }

    #[test]
    fn gate_suppresses_relic_line_repeats_within_the_cooldown_window() {
        let mut g = DebugLineGate::new();
        assert!(g.wants(RELIC_LINE, 1000.0));
        assert!(!g.wants(RELIC_LINE, 2000.0));
        // Fallback trigger shares the same suppression window
        assert!(!g.wants(RELIC_FALLBACK, 8000.0));
        assert!(g.wants(RELIC_LINE, 1000.0 + 7500.0));
    }

    #[test]
    fn gate_suppresses_chat_tab_repeats_within_its_own_shorter_window() {
        let mut g = DebugLineGate::new();
        assert!(g.wants(CHAT_LINE, 1000.0));
        assert!(!g.wants(CHAT_LINE, 2500.0));
        assert!(g.wants(CHAT_LINE, 1000.0 + 2000.0));
    }

    #[test]
    fn gate_does_not_suppress_non_relic_trigger_lines() {
        let mut g = DebugLineGate::new();
        assert!(g.wants(REWARD_LINE, 1000.0));
        assert!(g.wants(REWARD_LINE, 1001.0));
    }

    #[test]
    fn gate_forwards_reward_ui_login_and_mission_lines() {
        let mut g = DebugLineGate::new();
        for l in [
            "Script [Info]: ProjectionRewardChoice.lua: Missing icon data!",
            "Script [Info]: ThemedMainMenu.lua: MainMenu::LoginDone result=true",
            "Sys [Info]: EOM missionLocationUnlocked=1",
            "Script [Info]: TopMenu.lua: Abort: mission failed",
            "Sys [Info]: SyncAutoPopulatedConsumables for mission MT_SURVIVAL with location SolNode25",
            "Game [Info]: OnStateStarted, mission type=MT_CAPTURE",
        ] {
            assert!(g.wants(l, 0.0), "{l}");
        }
    }

    #[test]
    fn gate_forwards_only_weapon_resource_loads_inside_a_riven_view_window() {
        let mut g = DebugLineGate::new();
        let weapon_load = "Sys [Info]: ResourceLoader 0x1234 (/Lotus/Weapons/Tenno/Shotgun/PrimeBoar) Found 1,081 items to load";
        assert!(!g.wants(weapon_load, 900.0));
        assert!(g.wants("ThemedDetailedPurchaseDialog.lua: DBG: HudVis 1", 1000.0));
        assert!(g.wants(weapon_load, 1100.0));
        assert!(!g.wants(
            "Sys [Info]: ResourceLoader 0x1234 (/Lotus/Levels/Episodes/SmallBlackRoom.level) Found 22 items to load",
            1200.0
        ));
        assert!(!g.wants(weapon_load, 3000.0));
    }
}
