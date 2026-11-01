//! Pure EE.log domain parsers. Each is a state machine fed one line at a time;
//! file IO and tailing live elsewhere.

pub mod arbi;
pub mod lines;
pub mod profit_taker;
pub mod riven;
pub mod sanitize;
pub mod timing;
pub mod trade;

/// The game-clock stamp that opens every EE.log line, 0.0 when absent.
pub fn line_ts(line: &str) -> f64 {
    use regex::Regex;
    use std::sync::LazyLock;
    static TS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^\d]*(\d+\.\d+)").unwrap());
    TS.captures(line)
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(0.0)
}
