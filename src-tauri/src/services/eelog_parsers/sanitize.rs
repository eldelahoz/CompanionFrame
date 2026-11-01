//! Cleanup for names copied out of EE.log lines.

use std::sync::LazyLock;

use regex::Regex;

static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());
static FRAMEWORK: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        Regex::new(r"^\d+\.\d+\s").unwrap(),
        Regex::new(r"\[(Info|Error|Warning)\]").unwrap(),
        Regex::new(r"\b[A-Za-z_]+\.lua\b").unwrap(),
    ]
});
static DIALOG_TAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i),?\s*\b(?:title|leftItem|rightItem)=.*$").unwrap());

fn is_pua(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{FFFD}')
}

/// Strip live UTF-8 platform glyphs, legacy Latin-1 mojibake and trailing non-ASCII.
pub fn strip_platform_glyphs(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_pua(c) {
            i += 1;
        } else if matches!(c, '\u{EE}' | '\u{EF}')
            && chars
                .get(i + 1..i + 3)
                .is_some_and(|t| t.iter().all(|x| ('\u{80}'..='\u{BF}').contains(x)))
        {
            i += 3;
        } else {
            out.push(c);
            i += 1;
        }
    }
    let trimmed = out.trim_end_matches(|c: char| !c.is_ascii());
    SPACES.replace_all(trimmed, " ").trim().to_string()
}

/// Log framework line, not an item ("11828.904 Script [Info]: Dialog.lua: ...").
pub fn is_log_framework_line(value: &str) -> bool {
    FRAMEWORK.iter().any(|r| r.is_match(value))
}

/// Dialog::CreateOkCancel arg tail glued to the last item ("..., title= leftItem=/...").
pub fn strip_dialog_arg_tail(value: &str) -> String {
    DIALOG_TAIL.replace(value, "").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_glyphs_mojibake_and_trailing_non_ascii() {
        assert_eq!(strip_platform_glyphs("Name\u{E000}"), "Name");
        assert_eq!(strip_platform_glyphs("Name\u{EE}\u{80}\u{93}"), "Name");
        assert_eq!(strip_platform_glyphs("  A   B  é"), "A B");
    }

    #[test]
    fn detects_framework_lines_and_dialog_tails() {
        assert!(is_log_framework_line(
            "11828.904 Script [Info]: Dialog.lua: x"
        ));
        assert!(!is_log_framework_line("Soma Prime"));
        assert_eq!(strip_dialog_arg_tail("Soma, title= leftItem=/x"), "Soma");
    }
}
