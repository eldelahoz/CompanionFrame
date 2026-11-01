//! Wall-clock milliseconds since the Unix epoch.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// For values that cross to the TS side as JS numbers. Keeps the sub-millisecond fraction so it
/// compares exactly with file mtimes read the same way.
pub fn now_ms_f64() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}
