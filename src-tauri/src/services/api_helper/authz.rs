//! Using the user's own game auth string: fetch their inventory from DE's endpoint and tell
//! "game closed" from "login screen" from "token missed". The authz comes in as `&str` and is
//! only ever formatted into the request URL; every error here is stripped of that URL.

use std::io::Read;
use std::path::Path;

use reqwest::Client;

use super::HelperRunReason;
use crate::services::atomic::write_atomic;
use crate::services::game_memory::GameAuthzStatus;
use crate::services::inventory::payload::unwrap_inventory_text;

/// DE's own hosts, tried in order.
pub const HOSTS: [&str; 2] = ["https://api.warframe.com", "https://mobile.warframe.com"];

/// Real inventories are ~1 MiB and only grow; keep this well above them.
const MAX_INVENTORY_BYTES: usize = 16 * 1024 * 1024;

/// EE.log logs this once the session is authenticated (AlecaFrame uses the same marker), and the
/// file is recreated on game start.
pub const EE_LOG_LOGIN_MARKER: &str = "Logging in as ";

/// GET `<base>/api/inventory.php<authz>` (`authz` is `?accountId=..&nonce=..`) and store the body
/// at `dest` once it looks like an inventory, so a junk reply never replaces a good file.
pub async fn fetch_inventory(
    client: &Client,
    bases: &[&str],
    authz: &str,
    dest: &Path,
) -> Result<(), String> {
    let mut last = "no inventory hosts".to_owned();
    for base in bases {
        match fetch_one(client, &format!("{base}/api/inventory.php{authz}")).await {
            Ok(body) => {
                if let Some(dir) = dest.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                write_atomic(dest, &body).map_err(|e| e.to_string())?;
                tracing::info!(host = base, bytes = body.len(), "inventory fetched");
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(host = base, error = %e, "inventory request failed");
                last = e;
            }
        }
    }
    Err(last)
}

async fn fetch_one(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let mut res = client
        .get(url)
        .header("Accept", "application/json,*/*")
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if res.status() != reqwest::StatusCode::OK {
        return Err(format!("HTTP {}", res.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.without_url().to_string())? {
        if body.len() + chunk.len() > MAX_INVENTORY_BYTES {
            return Err(format!("response exceeds {MAX_INVENTORY_BYTES} bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    let text = std::str::from_utf8(&body).map_err(|_| "response is not UTF-8")?;
    match unwrap_inventory_text(text) {
        Ok(Some(_)) => Ok(body),
        _ => Err(format!(
            "unexpected inventory response ({} bytes)",
            body.len()
        )),
    }
}

/// Chunked scan so a large EE.log never sits in memory at once.
pub fn file_contains_marker(path: &Path, marker: &str, chunk_size: usize) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = vec![0u8; chunk_size];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = match f.read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(n) => n,
        };
        carry.extend_from_slice(&buf[..n]);
        if carry.windows(marker.len()).any(|w| w == marker.as_bytes()) {
            return true;
        }
        let keep = carry.len().saturating_sub(marker.len());
        carry.drain(..keep);
    }
}

/// Exit 3 / "crumbs not found" conflates "at the login screen" with "logged in but the token scan
/// failed"; EE.log tells the two apart.
pub fn classify_not_logged_in(ee_log: Option<&Path>) -> HelperRunReason {
    match ee_log {
        Some(p) if file_contains_marker(p, EE_LOG_LOGIN_MARKER, 4 * 1024 * 1024) => {
            HelperRunReason::TokenNotFound
        }
        _ => HelperRunReason::NotLoggedIn,
    }
}

/// The reason for a native scan that yielded no usable auth.
pub fn native_reason(status: &GameAuthzStatus, ee_log: Option<&Path>) -> HelperRunReason {
    match status {
        GameAuthzStatus::ProcessNotFound => HelperRunReason::GameNotRunning,
        GameAuthzStatus::AccessDenied => HelperRunReason::AccessDenied,
        GameAuthzStatus::Unavailable | GameAuthzStatus::Ok => HelperRunReason::Error,
        GameAuthzStatus::CrumbsNotFound | GameAuthzStatus::CrumbsAmbiguous => {
            classify_not_logged_in(ee_log)
        }
    }
}
