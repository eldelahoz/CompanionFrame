//! Fallback for a Cloudflare-challenged network (docs/wfm-auth.md §4): a hidden webview earns
//! warframe.market's clearance cookies and the caller replays them on its own requests.
//! Used by the static-asset image service (`wfm_images`); header-mode sign-in has not needed it. Ports WFHelper's `_solveChallengeInWindow`.

use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const PAGE: &str = "https://warframe.market/";
const TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(500);

/// WFHelper finishes on the JWT cookie (set once the real page loaded); cf_* are kept when present.
#[cfg(test)]
fn is_ready(cookies: &[(String, String)]) -> bool {
    cookies.iter().any(|(n, _)| n == "JWT")
}

fn is_clearance(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("cf_clearance") || n.starts_with("__cf")
}

/// Only the cf_* cookies travel, like WFHelper's `_clearanceCookie`.
fn cookie_header(cookies: &[(String, String)]) -> String {
    cookies
        .iter()
        .filter(|(n, _)| is_clearance(n))
        .map(|(n, v)| format!("{n}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The webview is pinned to this User-Agent so replayed requests can send the same one.
pub const CLEARANCE_UA: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) CompanionFrame/",
    env!("CARGO_PKG_VERSION")
);

/// Static assets only need Cloudflare's own clearance, not the JWT of a signed-in page.
pub async fn asset_clearance_header(app: &AppHandle) -> Result<String, String> {
    clearance(app, |c| {
        c.iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("cf_clearance"))
    })
    .await
}

async fn clearance(
    app: &AppHandle,
    ready: fn(&[(String, String)]) -> bool,
) -> Result<String, String> {
    let start = Instant::now();
    let dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("wfm-clearance");
    let win = WebviewWindowBuilder::new(
        app,
        "wfm-clearance",
        WebviewUrl::External(PAGE.parse().map_err(|e: url::ParseError| e.to_string())?),
    )
    .visible(false)
    .user_agent(CLEARANCE_UA)
    .data_directory(dir)
    .build()
    .map_err(|e| e.to_string())?;

    let page: url::Url = PAGE.parse().unwrap();
    let mut cookies = Vec::new();
    while start.elapsed() < TIMEOUT {
        tokio::time::sleep(POLL).await;
        cookies = win
            .cookies_for_url(page.clone())
            .map_err(|e| e.to_string())?
            .iter()
            .map(|c| (c.name().to_string(), c.value().to_string()))
            .collect();
        if ready(&cookies) {
            break;
        }
    }
    let _ = win.destroy();
    Ok(cookie_header(&cookies))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: &str, v: &str) -> (String, String) {
        (n.into(), v.into())
    }

    #[test]
    fn header_keeps_only_cloudflare_cookies() {
        let jar = [c("JWT", "x"), c("cf_clearance", "abc"), c("__cf_bm", "d")];
        assert_eq!(cookie_header(&jar), "cf_clearance=abc; __cf_bm=d");
        assert_eq!(cookie_header(&[c("JWT", "x")]), "");
    }

    #[test]
    fn ready_on_jwt() {
        assert!(is_ready(&[c("JWT", "x")]));
        assert!(!is_ready(&[c("cf_clearance", "abc")]));
    }
}
