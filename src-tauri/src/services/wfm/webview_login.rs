//! Sign in through the official warframe.market page in a visible webview (docs/wfm-auth.md section 7).
//! The user types the credentials on the site itself; we only read the `JWT` cookie it sets, check it
//! against `GET /v2/me` and adopt it. The password never reaches this app.

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use super::errors::WfmError;
use super::session::{Session, SessionStatus};

const LABEL: &str = "wfm-login";
// shortcut: sign-in URL not verified live (Cloudflare challenges automated checks), check it on the first real sign-in.
const PAGE: &str = "https://warframe.market/auth/signin";
const DIR: &str = "wfm-login";
const TIMEOUT: Duration = Duration::from_secs(5 * 60);
const POLL: Duration = Duration::from_millis(1500);

/// The `JWT` cookie value; the site sets it for anonymous visitors too, so it is only a candidate.
fn jwt_cookie(cookies: &[(String, String)]) -> Option<&str> {
    cookies
        .iter()
        .find(|(n, v)| n == "JWT" && !v.is_empty())
        .map(|(_, v)| v.as_str())
}

/// Claims of a JWT (no signature check: the server judges the token, this only filters junk).
fn jwt_claims(token: &str) -> Option<Value> {
    let mut parts = token.split('.');
    let (_, payload, _, None) = (parts.next()?, parts.next()?, parts.next()?, parts.next()) else {
        return None;
    };
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()
}

fn data_dir(app: &AppHandle) -> Result<std::path::PathBuf, WfmError> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(WfmError::invalid)?
        .join(DIR))
}

/// Sign-out: close a leftover window and drop the cookies the site left in our own data directory.
pub fn clear(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
    if let Ok(dir) = data_dir(app) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_sign_in_webview(
    app: AppHandle,
    session: State<'_, Session>,
) -> Result<SessionStatus, WfmError> {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.set_focus();
        return Err(WfmError::invalid("The sign-in window is already open."));
    }
    let page: url::Url = PAGE.parse().map_err(WfmError::invalid)?;
    let win = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(page.clone()))
        .title("Sign in to warframe.market")
        .inner_size(1000.0, 760.0)
        .data_directory(data_dir(&app)?)
        .build()
        .map_err(WfmError::invalid)?;

    let start = Instant::now();
    let mut tried = String::new();
    let result = loop {
        tokio::time::sleep(POLL).await;
        if app.get_webview_window(LABEL).is_none() {
            break Err(WfmError::invalid("Sign-in cancelled."));
        }
        if start.elapsed() > TIMEOUT {
            break Err(WfmError::invalid("Sign-in timed out."));
        }
        let Ok(cookies) = win.cookies_for_url(page.clone()) else {
            continue;
        };
        let cookies: Vec<_> = cookies
            .iter()
            .map(|c| (c.name().to_owned(), c.value().to_owned()))
            .collect();
        let Some(token) = jwt_cookie(&cookies).filter(|t| *t != tried && jwt_claims(t).is_some())
        else {
            continue;
        };
        tried = token.to_owned();
        // 401 = still the anonymous cookie; anything else transient: keep polling.
        if let Ok(status) = session.adopt_web_token(token).await {
            break Ok(status);
        }
    };
    let _ = win.destroy();
    result
}

/// Closes the sign-in window; the pending `wfm_sign_in_webview` then resolves as cancelled.
#[tauri::command]
#[specta::specta]
pub fn wfm_sign_in_webview_cancel(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.destroy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(n: &str, v: &str) -> (String, String) {
        (n.into(), v.into())
    }

    fn jwt(claims: &str) -> String {
        format!("e30.{}.sig", URL_SAFE_NO_PAD.encode(claims))
    }

    #[test]
    fn picks_the_jwt_cookie_only() {
        let jar = [c("cf_clearance", "x"), c("jwt", "lower"), c("JWT", "tok")];
        assert_eq!(jwt_cookie(&jar), Some("tok"));
        assert_eq!(jwt_cookie(&[c("JWT", "")]), None);
        assert_eq!(jwt_cookie(&[c("cf_clearance", "x")]), None);
    }

    #[test]
    fn parses_claims_and_rejects_junk() {
        let t = jwt(r#"{"csrf_token":"abc","sub":"u1"}"#);
        assert_eq!(jwt_claims(&t).unwrap()["sub"], "u1");
        for bad in ["", "abc", "a.b", "a.!!.c", "a.b.c.d", &jwt("not json")] {
            assert!(jwt_claims(bad).is_none(), "{bad}");
        }
    }
}
