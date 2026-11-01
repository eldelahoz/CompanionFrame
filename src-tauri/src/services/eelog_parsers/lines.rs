//! Single-line classifiers: each answers "is this EE.log line X" or pulls one field out of it.

use std::sync::LazyLock;

use regex::Regex;

macro_rules! rx {
    ($name:ident, $p:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($p).unwrap());
    };
}

// First line of every EE.log; the client language the game was started with.
rx!(
    COMMAND_LINE_LANGUAGE,
    r"Process Command-line:.*?(?:^|\s)-language:([A-Za-z]{2,3})\b"
);
rx!(LOGIN_COMPLETE, r"\bMainMenu::LoginDone result=true\b");
rx!(INVENTORY_SYNC, r"\bSyncInventoryFromDB\b");
rx!(EE_CONFIG_SAVED, r"Saved package: /Configs/EE\.cfg\s*$");
// Mission-info dumps use both key-value and JSON shapes; VoidT6 means omnia.
rx!(
    ACTIVE_MISSION_TAG,
    r#"(?:^|["\s])activeMissionTag["\s]*[=:]\s*"?([A-Za-z0-9_]+)"#
);
// Extraction or abort ends the fissure; no orbiter line re-tags afterwards.
rx!(
    MISSION_END,
    r"Sys \[Info\]: EOM missionLocationUnlocked=|TopMenu\.lua: Abort:"
);
// TradingPost.lua appends punctuation after the counterparty; exclude it from the name.
rx!(
    TRADE_PARTNER,
    r"(?i)TradingPost\.lua.*?[Tt]rade.*?[Ww]ith[: ]+([A-Za-z0-9_\-.]+)\.?\s*$"
);
rx!(
    REWARD_TRIGGER,
    r"(?i)\bPause countdown done\b|\bGot rewards\b"
);
// Fires while the reward choice cards render (one line per un-cached icon).
rx!(
    REWARD_UI_READY,
    r"(?i)ProjectionRewardChoice\.lua:\s*Missing icon data!"
);
// Fires the moment the in-game reward screen closes.
rx!(
    REWARD_SCREEN_CLOSE,
    r"(?i)ProjectionRewardChoice\.lua:\s*Relic reward screen shut down"
);
// Keep the earlier PopulateInventoryGrid fallback in case DE renames LoadingCompleteEnd.
rx!(
    RELIC_PICKER_OPEN,
    r"(?i)\bThemedProjectionManager\.lua:\s*LoadingCompleteEnd\b|\bProjection[A-Za-z_]*\.lua:\s*LoadingCompleteEnd\b|\bThemedProjectionManager\.lua:\s*PopulateInventoryGrid\b|\bProjectionManager\.lua:\s*PopulateInventoryGrid\b|\bProjection[A-Za-z_]*\.lua:\s*PopulateInventoryGrid\b"
);
// InitMapping fires entering and leaving full-screen UI, so the gap distinguishes exit.
rx!(
    RELIC_PICKER_CLOSE,
    r"(?i)\bInitMapping for all devices with bindings\b"
);

// Private chat tabs prefix the username with F; message text is not logged.
const CHAT_TAB_MARKER: &str = "ChatRedux::AddTab: Adding tab with channel name: ";

pub fn is_login_complete_line(line: &str) -> bool {
    LOGIN_COMPLETE.is_match(line)
}

/// The game is fetching its inventory from the server (start of a login or reload).
pub fn is_inventory_sync_line(line: &str) -> bool {
    INVENTORY_SYNC.is_match(line)
}

/// The game writes settings (interface scale included) to EE.cfg lazily; this
/// line is the only signal that the file just changed.
pub fn is_ee_config_saved_line(line: &str) -> bool {
    EE_CONFIG_SAVED.is_match(line)
}

pub fn is_mission_end_line(line: &str) -> bool {
    MISSION_END.is_match(line)
}

pub fn parse_active_mission_tag(line: &str) -> Option<String> {
    if !line.contains("activeMissionTag") {
        return None;
    }
    ACTIVE_MISSION_TAG.captures(line).map(|c| c[1].to_string())
}

pub fn parse_whisper_username(line: &str) -> Option<String> {
    let rest = &line[line.find(CHAT_TAB_MARKER)? + CHAT_TAB_MARKER.len()..];
    let name = rest[..rest.find(" to index")?].strip_prefix('F')?; // only private/whisper tabs
    // Warframe appends a platform glyph (U+E000). DBWIN delivers it latin1 as
    // 3 chars, the file poll utf8 as 1 - strip all so both sources agree.
    let name = name.trim_end_matches(|c: char| !c.is_ascii()).trim();
    (!name.is_empty()).then(|| name.to_string())
}

pub fn parse_trade_partner(line: &str) -> Option<String> {
    let c = TRADE_PARTNER.captures(line)?;
    let name = c[1].trim_end_matches('.').trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The game language from the `Process Command-line:` line, lowercase ("fr").
pub fn command_line_language(line: &str) -> Option<String> {
    Some(COMMAND_LINE_LANGUAGE.captures(line)?[1].to_ascii_lowercase())
}

pub fn is_reward_trigger_line(line: &str) -> bool {
    REWARD_TRIGGER.is_match(line)
}

pub fn is_reward_ui_ready_line(line: &str) -> bool {
    REWARD_UI_READY.is_match(line)
}

pub fn is_reward_screen_close_line(line: &str) -> bool {
    REWARD_SCREEN_CLOSE.is_match(line)
}

pub fn is_relic_picker_open_line(line: &str) -> bool {
    RELIC_PICKER_OPEN.is_match(line)
}

pub fn is_relic_picker_close_line(line: &str) -> bool {
    RELIC_PICKER_CLOSE.is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_game_language_from_the_command_line() {
        let line = "0.630 Sys [Diag]: Process Command-line: -windowMode:2 -cluster:public -language:fr -clienttype:Steam";
        assert_eq!(command_line_language(line).as_deref(), Some("fr"));
        assert_eq!(
            command_line_language("Sys [Diag]: Process Command-line: -language:TC").as_deref(),
            Some("tc")
        );
        assert_eq!(
            command_line_language("Sys [Diag]: Process Command-line: -cluster:public"),
            None
        );
        assert_eq!(
            command_line_language("Script [Info]: chat -language:fr"),
            None
        );
    }

    #[test]
    fn parses_the_active_mission_tag_in_both_shapes() {
        assert_eq!(
            parse_active_mission_tag("    activeMissionTag=VoidT6").as_deref(),
            Some("VoidT6")
        );
        assert_eq!(
            parse_active_mission_tag("activeMissionTag=VoidT1").as_deref(),
            Some("VoidT1")
        );
        assert_eq!(
            parse_active_mission_tag(r#"    "activeMissionTag" : "VoidT5","#).as_deref(),
            Some("VoidT5")
        );
        assert_eq!(
            parse_active_mission_tag(r#"{"activeMissionTag":"VoidT2"}"#).as_deref(),
            Some("VoidT2")
        );
    }

    #[test]
    fn ignores_unrelated_and_empty_mission_tag_lines() {
        assert_eq!(
            parse_active_mission_tag("    activeMissionId=SolNode717_6a59dee6"),
            None
        );
        assert_eq!(parse_active_mission_tag("activeMissionTag="), None);
        assert_eq!(
            parse_active_mission_tag("123.4 Sys [Info]: nothing here"),
            None
        );
    }

    #[test]
    fn matches_extraction_and_abort_lines_only() {
        assert!(is_mission_end_line(
            "81.780 Sys [Info]: EOM missionLocationUnlocked=1"
        ));
        assert!(is_mission_end_line(
            "94.132 Script [Info]: TopMenu.lua: Abort: aborting mission"
        ));
        assert!(!is_mission_end_line(
            "50.1 Sys [Info]: MissionLocation loaded"
        ));
        assert!(!is_mission_end_line("    activeMissionTag=VoidT6"));
    }

    fn tab(channel: &str) -> String {
        format!(
            "123.456 Sys [Info]: ChatRedux::AddTab: Adding tab with channel name: {channel} to index 4"
        )
    }

    #[test]
    fn extracts_whisper_usernames() {
        assert_eq!(
            parse_whisper_username(&tab("FSomePlayer")).as_deref(),
            Some("SomePlayer")
        );
        assert_eq!(
            parse_whisper_username(&tab("FConsoleDude★")).as_deref(),
            Some("ConsoleDude")
        );
        // utf8 ee 80 80 (U+E000) decoded as latin1 = 3 junk chars; the file poll gives 1 char
        assert_eq!(
            parse_whisper_username(&tab("Ftenno0451î\u{80}\u{80}")).as_deref(),
            Some("tenno0451")
        );
        assert_eq!(
            parse_whisper_username(&tab("Ftenno0451\u{E000}")).as_deref(),
            Some("tenno0451")
        );
    }

    #[test]
    fn ignores_non_whisper_tabs_and_unrelated_lines() {
        assert_eq!(parse_whisper_username(&tab("Region")), None);
        assert_eq!(parse_whisper_username(&tab("CClanChat")), None);
        assert_eq!(
            parse_whisper_username("123 Sys [Info]: Pause countdown done"),
            None
        );
        assert_eq!(
            parse_whisper_username("ChatRedux::AddTab: Adding tab with channel name: F"),
            None
        );
    }

    #[test]
    fn detects_the_inventory_sync_marker() {
        assert!(is_inventory_sync_line(
            "24.505 Sys [Info]: SyncInventoryFromDB"
        ));
        assert!(!is_inventory_sync_line(
            "25.082 Sys [Info]: OnInventoryResults, body size=1"
        ));
    }

    #[test]
    fn detects_a_completed_login_only() {
        assert!(is_login_complete_line(
            "16.880 Script [Info]: ThemedMainMenu.lua: MainMenu::LoginDone result=true"
        ));
        assert!(!is_login_complete_line("14.092 Sys [Info]: Logging in as "));
        assert!(!is_login_complete_line("MainMenu::LoginDone result=false"));
    }

    #[test]
    fn reads_the_trade_partner_without_trailing_punctuation() {
        let line = "12.0 Script [Info]: TradingPost.lua: Started trade with Some_Player.";
        assert_eq!(parse_trade_partner(line).as_deref(), Some("Some_Player"));
        assert_eq!(parse_trade_partner("12.0 Sys [Info]: nothing"), None);
    }

    #[test]
    fn classifies_reward_relic_and_config_lines() {
        assert!(is_reward_trigger_line(
            "1.0 Sys [Info]: PAUSE COUNTDOWN DONE"
        ));
        assert!(is_reward_trigger_line("1.0 Sys [Info]: Got rewards"));
        assert!(is_reward_ui_ready_line(
            "1.0 ProjectionRewardChoice.lua: Missing icon data!"
        ));
        assert!(is_reward_screen_close_line(
            "1.0 ProjectionRewardChoice.lua: Relic reward screen shut down"
        ));
        assert!(is_relic_picker_open_line(
            "1.0 ThemedProjectionManager.lua: LoadingCompleteEnd"
        ));
        assert!(is_relic_picker_open_line(
            "1.0 ProjectionManager.lua: PopulateInventoryGrid"
        ));
        assert!(is_relic_picker_close_line(
            "1.0 InitMapping for all devices with bindings"
        ));
        assert!(is_ee_config_saved_line(
            "1.0 Sys [Info]: Saved package: /Configs/EE.cfg  "
        ));
        assert!(!is_ee_config_saved_line(
            "1.0 Saved package: /Configs/EE.cfg.bak"
        ));
    }
}
