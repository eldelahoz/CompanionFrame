//! Sign-in session: v1 header-mode sign-in (docs/wfm-auth.md section 4), token in the OS keyring.
//!
//! The password is never stored or logged and its buffers are zeroized. If the keyring refuses the
//! write the login stays in memory only (`persistable: false`), like WFHelper's safeStorage case.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use zeroize::Zeroizing;

use super::client::{BASE_V1, BASE_V2, TokenSink, WfmClient};
use super::errors::WfmError;
use crate::services::atomic::write_atomic;

const KEYRING_SERVICE: &str = "com.companionframe.app";
const KEYRING_USER: &str = "wfm-session";
const DEVICE_ID_FILE: &str = "wfm-device-id";

/// Raw secret storage; the session keeps the JSON in it.
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, value: &str) -> Result<(), String>;
    fn delete(&self);
}

pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
    }
}

impl SecretStore for KeyringStore {
    fn get(&self) -> Option<String> {
        Self::entry().ok()?.get_password().ok()
    }
    fn set(&self, value: &str) -> Result<(), String> {
        Self::entry()
            .and_then(|e| e.set_password(value))
            .map_err(|e| e.to_string())
    }
    fn delete(&self) {
        if let Ok(e) = Self::entry() {
            let _ = e.delete_credential();
        }
    }
}

/// How the login was obtained; the webview one also leaves cookies to clear on sign-out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Password,
    Webview,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Saved {
    token: String,
    user_name: String,
    platform: String,
    #[serde(default)]
    source: Source,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct SessionStatus {
    pub logged_in: bool,
    pub user_name: Option<String>,
    pub platform: Option<String>,
    pub source: Option<Source>,
    /// The server rejected the token (401): sign in again.
    pub expired: bool,
    /// False when the keyring refused the write, so the login ends with the app.
    pub persistable: bool,
}

struct Shared {
    store: Arc<dyn SecretStore>,
    user: Mutex<Option<(String, String, Source)>>,
    expired: AtomicBool,
    persistable: AtomicBool,
    /// Hears every token change (rotation, sign-in, sign-out, expiry), e.g. the WebSocket.
    tap: Mutex<Option<TokenSink>>,
}

impl Shared {
    fn save(&self, saved: &Saved) {
        let ok = serde_json::to_string(saved)
            .map_err(|e| e.to_string())
            .and_then(|json| self.store.set(&json));
        if let Err(e) = &ok {
            tracing::warn!(error = %e, "wfm session not persisted: keyring unavailable");
        }
        self.persistable.store(ok.is_ok(), Ordering::SeqCst);
    }

    fn notify(&self, token: Option<&str>) {
        if let Some(tap) = &*self.tap.lock().unwrap() {
            tap(token);
        }
    }

    fn load(&self) -> Option<Saved> {
        serde_json::from_str(&self.store.get()?).ok()
    }
}

pub struct Session {
    client: WfmClient,
    shared: Arc<Shared>,
    device_id: String,
}

#[derive(Serialize)]
struct SignInBody<'a> {
    email: &'a str,
    password: &'a str,
    device_id: &'a str,
    auth_type: &'a str,
}

/// Random per-install id, persisted next to the settings so the account sees a stable device.
pub fn device_id(dir: &Path) -> String {
    let path = dir.join(DEVICE_ID_FILE);
    if let Some(id) = std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
    {
        return id;
    }
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = write_atomic(&path, id.as_bytes()) {
        tracing::warn!(error = %e, "could not persist wfm device id");
    }
    id
}

impl Session {
    pub fn new(store: Arc<dyn SecretStore>, device_id: String) -> Result<Self, WfmError> {
        Self::with_urls(BASE_V1, BASE_V2, store, device_id)
    }

    fn with_urls(
        v1: &str,
        v2: &str,
        store: Arc<dyn SecretStore>,
        device_id: String,
    ) -> Result<Self, WfmError> {
        let shared = Arc::new(Shared {
            store,
            user: Mutex::new(None),
            expired: AtomicBool::new(false),
            persistable: AtomicBool::new(true),
            tap: Mutex::new(None),
        });
        let sink = shared.clone();
        let client = WfmClient::new(
            v1,
            v2,
            Box::new(move |t| match t {
                // Rotated: keep the user, swap the token.
                Some(t) => {
                    if let Some(s) = sink.load() {
                        sink.save(&Saved {
                            token: t.to_owned(),
                            ..s
                        });
                    }
                    sink.notify(Some(t));
                }
                // Expired: back to signed out, ask for a new login.
                None => {
                    sink.store.delete();
                    *sink.user.lock().unwrap() = None;
                    sink.expired.store(true, Ordering::SeqCst);
                    sink.notify(None);
                }
            }),
        )?;
        if let Some(s) = shared.load() {
            client.set_token(Some(s.token));
            *shared.user.lock().unwrap() = Some((s.user_name, s.platform, s.source));
        }
        Ok(Self {
            client,
            shared,
            device_id,
        })
    }

    pub fn client(&self) -> &WfmClient {
        &self.client
    }

    pub fn on_token(&self, tap: TokenSink) {
        *self.shared.tap.lock().unwrap() = Some(tap);
    }

    pub fn status(&self) -> SessionStatus {
        let user = self.shared.user.lock().unwrap().clone();
        let logged_in = self.client.token().is_some();
        SessionStatus {
            logged_in,
            user_name: user.as_ref().filter(|_| logged_in).map(|u| u.0.clone()),
            platform: user.clone().filter(|_| logged_in).map(|u| u.1),
            source: user.filter(|_| logged_in).map(|u| u.2),
            expired: self.shared.expired.load(Ordering::SeqCst),
            persistable: self.shared.persistable.load(Ordering::SeqCst),
        }
    }

    /// One request, no CSRF/cookie retry and no resend: a password is never worth a second post.
    pub async fn sign_in(
        &self,
        email: &str,
        password: Zeroizing<String>,
    ) -> Result<SessionStatus, WfmError> {
        let email = email.trim();
        if email.is_empty() || password.is_empty() {
            return Err(WfmError::invalid("Email and password are required."));
        }
        let body = Zeroizing::new(
            serde_json::to_string(&SignInBody {
                email,
                password: &password,
                device_id: &self.device_id,
                auth_type: "header",
            })
            .map_err(WfmError::invalid)?,
        );
        drop(password);
        let (header_token, payload) = self.client.sign_in(body).await?;
        let field = |k: &str| {
            payload
                .pointer(&format!("/payload/{k}"))
                .or_else(|| payload.get(k))
        };
        let token = header_token
            .or_else(|| field("token").and_then(Value::as_str).map(str::to_owned))
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                WfmError::invalid("Sign-in succeeded but no session token was returned.")
            })?;
        let user = field("user");
        let text = |k: &str| user.and_then(|u| u.get(k)).and_then(Value::as_str);
        let saved = Saved {
            token: token.clone(),
            user_name: text("ingame_name")
                .or_else(|| text("name"))
                .unwrap_or_else(|| email.split('@').next().unwrap_or(email))
                .to_owned(),
            platform: text("platform").unwrap_or("pc").to_owned(),
            source: Source::Password,
        };
        Ok(self.adopt(saved))
    }

    /// Adopts a JWT the webview login produced, after `GET /v2/me` proved it is a real login.
    pub async fn adopt_web_token(&self, token: &str) -> Result<SessionStatus, WfmError> {
        let me = self.client.probe_me(token).await?;
        let text = |k: &str| me.pointer(&format!("/data/{k}")).and_then(Value::as_str);
        let user_name = text("ingameName")
            .or_else(|| text("slug"))
            .ok_or_else(|| WfmError::invalid("The profile answer had no user name."))?;
        Ok(self.adopt(Saved {
            token: token.to_owned(),
            user_name: user_name.to_owned(),
            platform: text("platform").unwrap_or("pc").to_owned(),
            source: Source::Webview,
        }))
    }

    fn adopt(&self, saved: Saved) -> SessionStatus {
        self.client.set_token(Some(saved.token.clone()));
        *self.shared.user.lock().unwrap() = Some((
            saved.user_name.clone(),
            saved.platform.clone(),
            saved.source,
        ));
        self.shared.expired.store(false, Ordering::SeqCst);
        self.shared.save(&saved);
        self.shared.notify(Some(&saved.token));
        tracing::info!(user = %saved.user_name, source = ?saved.source, "wfm signed in");
        self.status()
    }

    pub fn sign_out(&self) -> SessionStatus {
        self.client.set_token(None);
        *self.shared.user.lock().unwrap() = None;
        self.shared.expired.store(false, Ordering::SeqCst);
        self.shared.persistable.store(true, Ordering::SeqCst);
        self.shared.store.delete();
        self.shared.notify(None);
        self.status()
    }
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_sign_in(
    email: String,
    password: String,
    session: State<'_, Session>,
) -> Result<SessionStatus, WfmError> {
    session.sign_in(&email, Zeroizing::new(password)).await
}

#[tauri::command]
#[specta::specta]
pub fn wfm_sign_out(app: tauri::AppHandle, session: State<'_, Session>) -> SessionStatus {
    super::webview_login::clear(&app);
    session.sign_out()
}

#[tauri::command]
#[specta::specta]
pub fn wfm_session_status(session: State<'_, Session>) -> SessionStatus {
    session.status()
}

#[cfg(test)]
mod tests {
    use super::super::errors::Class;
    use super::super::mock::{Mock, reply, spawn};
    use super::*;
    use reqwest::Method;

    use super::super::client::Api;

    #[derive(Default)]
    struct MemStore(Mutex<Option<String>>);
    impl SecretStore for MemStore {
        fn get(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn set(&self, v: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(v.into());
            Ok(())
        }
        fn delete(&self) {
            *self.0.lock().unwrap() = None;
        }
    }

    struct BrokenStore;
    impl SecretStore for BrokenStore {
        fn get(&self) -> Option<String> {
            None
        }
        fn set(&self, _: &str) -> Result<(), String> {
            Err("no keyring".into())
        }
        fn delete(&self) {}
    }

    fn session(m: &Mock, store: Arc<dyn SecretStore>) -> Session {
        Session::with_urls(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            store,
            "dev-1".into(),
        )
        .unwrap()
    }

    fn pw(s: &str) -> Zeroizing<String> {
        Zeroizing::new(s.into())
    }

    fn ok_signin(name: &str) -> super::super::mock::Reply {
        reply(
            200,
            &format!(r#"{{"payload":{{"user":{{"ingame_name":"{name}","platform":"pc"}}}}}}"#),
        )
        .header("Authorization", "JWT test-token")
    }

    #[tokio::test]
    async fn persists_restores_and_signs_out() {
        let m = spawn(vec![ok_signin("Trade Partner")]);
        let store = Arc::new(MemStore::default());
        let s = session(&m, store.clone());
        let st = s
            .sign_in("tester@example.test", pw("correct-horse"))
            .await
            .unwrap();
        assert!(st.logged_in && st.persistable && !st.expired);
        assert_eq!(st.user_name.as_deref(), Some("Trade Partner"));
        assert_eq!(
            serde_json::from_str::<Value>(&store.get().unwrap()).unwrap(),
            serde_json::json!({"token":"test-token","user_name":"Trade Partner","platform":"pc","source":"password"})
        );

        let restored = session(&m, store.clone());
        assert_eq!(restored.status(), st);
        assert_eq!(restored.client().token().as_deref(), Some("test-token"));

        let out = restored.sign_out();
        assert!(!out.logged_in && out.user_name.is_none());
        assert_eq!(store.get(), None);
        assert_eq!(restored.client().token(), None);
    }

    #[tokio::test]
    async fn sign_in_posts_header_mode_body_with_device_id() {
        let m = spawn(vec![ok_signin("A")]);
        let s = session(&m, Arc::new(MemStore::default()));
        s.sign_in(" a@b.test ", pw("pw")).await.unwrap();
        let r = &m.requests()[0];
        assert!(r.starts_with("post /v1/auth/signin "));
        assert!(r.ends_with(
            r#"{"email":"a@b.test","password":"pw","device_id":"dev-1","auth_type":"header"}"#
        ));
    }

    #[tokio::test]
    async fn adopts_a_webview_token_after_the_me_probe() {
        let me = r#"{"data":{"ingameName":"Web User","platform":"pc"}}"#;
        let m = spawn(vec![reply(200, me)]);
        let store = Arc::new(MemStore::default());
        let s = session(&m, store.clone());
        let st = s.adopt_web_token("cookie-jwt").await.unwrap();
        assert_eq!(
            (st.logged_in, st.user_name.as_deref(), st.source),
            (true, Some("Web User"), Some(Source::Webview))
        );
        assert!(m.requests()[0].contains("authorization: bearer cookie-jwt\r\n"));
        assert_eq!(session(&m, store).status(), st, "survives a restart");
    }

    #[tokio::test]
    async fn anonymous_web_token_is_not_adopted() {
        let m = spawn(vec![reply(401, "")]);
        let s = session(&m, Arc::new(MemStore::default()));
        assert!(s.adopt_web_token("anon").await.is_err());
        let st = s.status();
        assert!(
            !st.logged_in && !st.expired,
            "a probe must not touch the session"
        );
    }

    #[test]
    fn legacy_saved_session_defaults_to_password_source() {
        let m = spawn(vec![]);
        let store = Arc::new(MemStore::default());
        store
            .set(r#"{"token":"t","user_name":"U","platform":"pc"}"#)
            .unwrap();
        assert_eq!(session(&m, store).status().source, Some(Source::Password));
    }

    #[test]
    fn corrupt_stored_session_starts_signed_out() {
        let m = spawn(vec![]);
        for junk in ["{bad-json", r#"{"token":"only"}"#] {
            let store = Arc::new(MemStore::default());
            store.set(junk).unwrap();
            let s = session(&m, store);
            assert!(!s.status().logged_in, "{junk}");
            assert_eq!(s.client().token(), None);
        }
    }

    #[tokio::test]
    async fn keyring_failure_keeps_login_in_memory_only() {
        let m = spawn(vec![ok_signin("Trade Partner")]);
        let s = session(&m, Arc::new(BrokenStore));
        let st = s.sign_in("t@e.test", pw("pw")).await.unwrap();
        assert!(st.logged_in && !st.persistable);
        let restarted = session(&m, Arc::new(BrokenStore));
        assert!(!restarted.status().logged_in);
        assert!(s.sign_out().persistable, "sign-out resets the flag");
    }

    #[tokio::test]
    async fn token_falls_back_to_body_and_name_to_email() {
        let m = spawn(vec![reply(200, r#"{"payload":{"token":"from-body"}}"#)]);
        let s = session(&m, Arc::new(MemStore::default()));
        let st = s.sign_in("alice@example.test", pw("pw")).await.unwrap();
        assert_eq!(s.client().token().as_deref(), Some("from-body"));
        assert_eq!(st.user_name.as_deref(), Some("alice"));
        assert_eq!(st.platform.as_deref(), Some("pc"));
    }

    #[tokio::test]
    async fn missing_token_is_an_error_not_a_login() {
        let m = spawn(vec![reply(200, "{}")]);
        let s = session(&m, Arc::new(MemStore::default()));
        assert!(s.sign_in("a@b.test", pw("pw")).await.is_err());
        assert!(!s.status().logged_in);
    }

    #[tokio::test]
    async fn empty_credentials_send_nothing() {
        let m = spawn(vec![]);
        let s = session(&m, Arc::new(MemStore::default()));
        assert!(s.sign_in("", pw("pw")).await.is_err());
        assert!(s.sign_in("a@b.test", pw("")).await.is_err());
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn failed_sign_in_is_single_shot_and_surfaces_status() {
        for (status, want) in [(429, "rate"), (500, "api"), (401, "unauth")] {
            let m = spawn(vec![reply(status, ""), ok_signin("A")]);
            let s = session(&m, Arc::new(MemStore::default()));
            let err = s.sign_in("a@b.test", pw("pw")).await.unwrap_err();
            let got = match err {
                WfmError::RateLimited { .. } => "rate",
                WfmError::Api { .. } => "api",
                WfmError::Unauthorized { .. } => "unauth",
                _ => "other",
            };
            assert_eq!(got, want);
            assert_eq!(
                m.requests().len(),
                1,
                "{status}: no csrf fallback or resend"
            );
            assert!(!s.status().logged_in);
        }
    }

    #[tokio::test]
    async fn rotated_token_is_persisted_and_used() {
        let m = spawn(vec![
            ok_signin("A"),
            reply(200, "{}").header("Authorization", "Bearer rotated"),
            reply(200, "{}"),
        ]);
        let store = Arc::new(MemStore::default());
        let s = session(&m, store.clone());
        s.sign_in("a@b.test", pw("pw")).await.unwrap();
        s.client()
            .request(Api::V2, Method::GET, "/me", None)
            .await
            .unwrap();
        s.client()
            .request(Api::V2, Method::GET, "/me", None)
            .await
            .unwrap();
        let saved: Saved = serde_json::from_str(&store.get().unwrap()).unwrap();
        assert_eq!(
            (saved.token.as_str(), saved.user_name.as_str()),
            ("rotated", "A")
        );
        assert!(m.requests()[2].contains("authorization: bearer rotated\r\n"));
    }

    #[tokio::test]
    async fn token_tap_hears_sign_in_rotation_expiry_and_sign_out() {
        let m = spawn(vec![
            ok_signin("A"),
            reply(200, "{}").header("Authorization", "Bearer rotated"),
            reply(401, ""),
        ]);
        let s = session(&m, Arc::new(MemStore::default()));
        let heard: Arc<Mutex<Vec<Option<String>>>> = Default::default();
        let h = heard.clone();
        s.on_token(Box::new(move |t| {
            h.lock().unwrap().push(t.map(str::to_owned))
        }));
        s.sign_in("a@b.test", pw("pw")).await.unwrap();
        let get = || s.client().request(Api::V2, Method::GET, "/me", None);
        get().await.unwrap();
        get().await.unwrap_err();
        s.sign_out();
        let some = |t: &str| Some(t.to_owned());
        assert_eq!(
            *heard.lock().unwrap(),
            [some("test-token"), some("rotated"), None, None]
        );
    }
    #[tokio::test]
    async fn expired_session_signs_out_without_looping() {
        let m = spawn(vec![ok_signin("A"), reply(401, ""), ok_signin("A")]);
        let store = Arc::new(MemStore::default());
        let s = session(&m, store.clone());
        s.sign_in("a@b.test", pw("pw")).await.unwrap();
        let err = s
            .client()
            .request(Api::V2, Method::GET, "/me", None)
            .await
            .unwrap_err();
        assert_eq!(err.class(), Class::Transient);
        let st = s.status();
        assert!(!st.logged_in && st.expired);
        assert_eq!(store.get(), None);
        assert_eq!(m.requests().len(), 2);
        // A fresh sign-in clears the flag.
        assert!(!s.sign_in("a@b.test", pw("pw")).await.unwrap().expired);
    }

    #[test]
    fn device_id_is_random_and_stable() {
        let dir = std::env::temp_dir().join(format!("cf-wfm-dev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = device_id(&dir);
        assert!(a.len() >= 6, "wfm v2 wants 6-256 chars");
        assert_eq!(device_id(&dir), a);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
