//! warframe.market HTTP client: identifiable UA, JWT on v1 / Bearer on v2, rotated-token adoption.
//!
//! Never logs request or response bodies. A 401 on an authenticated call drops the token (session
//! expired) and fails; nothing here retries, so a bad token cannot loop.

use std::sync::Mutex;

use reqwest::{Method, Response, header};
use serde_json::Value;
use zeroize::Zeroizing;

use super::catalog::CatalogMem;
use super::errors::{Class, WfmError, error_detail};
use super::scheduler::{self, Kind, Priority, Scheduler};
use crate::services::net;

pub const BASE_V1: &str = "https://api.warframe.market/v1";
pub const BASE_V2: &str = "https://api.warframe.market/v2";
const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Api {
    V1,
    V2,
}

/// Outcome of a request that may stop at a redirect.
#[derive(Debug, PartialEq)]
pub enum Sent {
    Done(Value),
    Redirect(Option<String>),
}

/// `Some(token)`: the server rotated it. `None`: the session expired (401).
pub type TokenSink = Box<dyn Fn(Option<&str>) + Send + Sync>;

pub struct WfmClient {
    http: reqwest::Client,
    /// Same client without redirects, for the calls that must read a `Location` themselves.
    manual: reqwest::Client,
    v1: String,
    v2: String,
    token: Mutex<Option<String>>,
    sink: TokenSink,
    pub(super) catalog: tokio::sync::Mutex<CatalogMem>,
    sched: Scheduler,
}

fn network(e: reqwest::Error) -> WfmError {
    WfmError::Network {
        detail: e.without_url().to_string(),
    }
}

/// `Authorization: JWT x` / `Bearer x` / bare `x` -> `x`.
fn strip_scheme(v: &str) -> &str {
    let v = v.trim();
    for scheme in ["bearer ", "jwt "] {
        if v.len() > scheme.len() && v[..scheme.len()].eq_ignore_ascii_case(scheme) {
            return v[scheme.len()..].trim();
        }
    }
    v
}

impl WfmClient {
    pub fn new(v1: &str, v2: &str, sink: TokenSink) -> Result<Self, WfmError> {
        Ok(Self {
            http: net::client().map_err(WfmError::invalid)?,
            manual: net::client_no_redirect().map_err(WfmError::invalid)?,
            v1: v1.trim_end_matches('/').into(),
            v2: v2.trim_end_matches('/').into(),
            token: Mutex::new(None),
            sink,
            sched: Scheduler::default(),
            catalog: Default::default(),
        })
    }

    /// `new` for tests against a mock server: no contract-search throttling.
    #[cfg(test)]
    pub(crate) fn new_unthrottled(v1: &str, v2: &str, sink: TokenSink) -> Result<Self, WfmError> {
        Ok(Self {
            sched: Scheduler::without_contract_limit(),
            ..Self::new(v1, v2, sink)?
        })
    }

    #[cfg(test)]
    pub(super) fn lift_gate(&self) {
        self.sched.lift_gate();
    }

    pub fn token(&self) -> Option<String> {
        self.token.lock().unwrap().clone()
    }

    pub fn set_token(&self, token: Option<String>) {
        *self.token.lock().unwrap() = token;
    }

    pub(super) fn base(&self, api: Api) -> &str {
        match api {
            Api::V1 => &self.v1,
            Api::V2 => &self.v2,
        }
    }

    fn adopt_rotated(&self, res: &Response) {
        let Some(new) = res
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(strip_scheme)
            .filter(|t| !t.is_empty())
        else {
            return;
        };
        let mut cur = self.token.lock().unwrap();
        // Only a signed-in session adopts: an anonymous response must not log anyone in.
        if cur.as_deref().is_some_and(|c| c != new) {
            *cur = Some(new.to_owned());
            drop(cur);
            (self.sink)(Some(new));
        }
    }

    /// Maps a non-2xx response to an error; `Ok(res)` for 2xx.
    async fn check(res: Response, fallback: Option<&str>) -> Result<Response, WfmError> {
        let status = res.status();
        if status.is_success() {
            return Ok(res);
        }
        // 509 is "too many concurrent connections from one IP": back off like a 429.
        if matches!(status.as_u16(), 429 | 509) {
            let retry_after_secs = res
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u32>().ok());
            return Err(WfmError::RateLimited { retry_after_secs });
        }
        let body = read_body(res).await.unwrap_or_default();
        let detail = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|v| error_detail(&v, fallback))
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        Err(if status.as_u16() == 401 {
            WfmError::Unauthorized { detail }
        } else {
            WfmError::Api {
                status: status.as_u16(),
                detail,
            }
        })
    }

    /// JSON request against `api`, authenticated when a token is held. Adopts any rotated token.
    /// Scheduled at the task's priority (`scheduler::scoped`), a user's call unless set otherwise.
    pub async fn request(
        &self,
        api: Api,
        method: Method,
        path: &str,
        json: Option<Zeroizing<String>>,
    ) -> Result<Value, WfmError> {
        self.request_with(scheduler::current(), api, method, path, json)
            .await
    }

    pub async fn request_with(
        &self,
        priority: Priority,
        api: Api,
        method: Method,
        path: &str,
        json: Option<Zeroizing<String>>,
    ) -> Result<Value, WfmError> {
        match self
            .scheduled(&self.http, priority, api, method, path, json)
            .await?
        {
            Sent::Done(v) => Ok(v),
            Sent::Redirect(_) => Err(WfmError::invalid("unexpected redirect")),
        }
    }

    /// Like `request` but a 3xx is returned, not followed (`Location` as sent; `None` if absent).
    pub async fn request_manual(
        &self,
        api: Api,
        method: Method,
        path: &str,
        json: Option<Zeroizing<String>>,
    ) -> Result<Sent, WfmError> {
        let priority = scheduler::current();
        self.scheduled(&self.manual, priority, api, method, path, json)
            .await
    }

    /// `send` under the shared budget. Sign-in and the token probe skip it: they are one-off, and
    /// must work while a 429 gate is up.
    async fn scheduled(
        &self,
        http: &reqwest::Client,
        priority: Priority,
        api: Api,
        method: Method,
        path: &str,
        json: Option<Zeroizing<String>>,
    ) -> Result<Sent, WfmError> {
        let kind = if method == Method::GET
            && (path.contains("/contracts") || path.contains("/auctions"))
        {
            Kind::Contract
        } else {
            Kind::Normal
        };
        let queued = std::time::Instant::now();
        self.sched
            .run_kind(priority, kind, || async {
                let (wait, started) = (queued.elapsed(), std::time::Instant::now());
                let result = self.send(http, api, method, path, json).await;
                // The first path segment is a fixed word (orders, items, ...), never an id.
                let path_kind = path.trim_start_matches('/').split(['/', '?']).next();
                tracing::debug!(
                    "timing: wfm_request priority={priority:?} path_kind={} wait_ms={} ms={}",
                    path_kind.unwrap_or(""),
                    wait.as_millis(),
                    started.elapsed().as_millis()
                );
                result
            })
            .await
    }

    async fn send(
        &self,
        http: &reqwest::Client,
        api: Api,
        method: Method,
        path: &str,
        json: Option<Zeroizing<String>>,
    ) -> Result<Sent, WfmError> {
        let token = self.token();
        let mut req = http
            .request(method, format!("{}{path}", self.base(api)))
            .header(header::ACCEPT, "application/json")
            .header("Platform", "pc")
            .header("Language", "en");
        if let Some(t) = &token {
            let scheme = if api == Api::V2 { "Bearer" } else { "JWT" };
            req = req
                .header(header::AUTHORIZATION, format!("{scheme} {t}"))
                .header("auth_type", "header");
        }
        if let Some(b) = json {
            req = req
                .header(header::CONTENT_TYPE, "application/json")
                .body(b.as_str().to_owned());
        }
        let res = req.send().await.map_err(network)?;
        self.adopt_rotated(&res);
        if res.status().is_redirection() {
            let location = res
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            return Ok(Sent::Redirect(location));
        }
        match Self::check(res, None).await {
            Ok(res) => parse_json(res).await.map(Sent::Done),
            Err(e @ WfmError::Unauthorized { .. }) => {
                if token.is_some() {
                    self.set_token(None);
                    (self.sink)(None);
                }
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// Public v2 read, falling back to the v1 path on a transient error. No data (400/404) does not
    /// fall back: the item is simply not there.
    pub async fn get_with_fallback(&self, v2_path: &str, v1_path: &str) -> Result<Value, WfmError> {
        match self.request(Api::V2, Method::GET, v2_path, None).await {
            Err(e) if e.class() == Class::Transient => {
                self.request(Api::V1, Method::GET, v1_path, None).await
            }
            other => other,
        }
    }

    /// `GET /v2/me` with a candidate token, touching no session state: 200 means the token is a real login
    /// (an anonymous web JWT gets 401). Returns the body.
    pub async fn probe_me(&self, token: &str) -> Result<Value, WfmError> {
        let res = self
            .http
            .get(format!("{}/me", self.v2))
            .header(header::ACCEPT, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header("auth_type", "header")
            .send()
            .await
            .map_err(network)?;
        parse_json(Self::check(res, None).await?).await
    }

    /// v1 header-mode sign-in. Returns the JWT (response header, else `JWT` cookie) and the body.
    /// The session token is not touched; that is the session's job.
    pub async fn sign_in(
        &self,
        body: Zeroizing<String>,
    ) -> Result<(Option<String>, Value), WfmError> {
        let res = self
            .http
            .post(format!("{}/auth/signin", self.v1))
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Platform", "pc")
            .header("Language", "en")
            // Bare scheme opts out of the cookie session: no CSRF page needed.
            .header(header::AUTHORIZATION, "JWT")
            .header("auth_type", "header")
            .body(body.as_str().to_owned())
            .send()
            .await
            .map_err(network)?;
        let res = Self::check(res, Some("Invalid credentials.")).await?;
        let h = res.headers();
        let token = h
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .map(|v| strip_scheme(v).to_owned())
            .filter(|t| !t.is_empty())
            .or_else(|| {
                h.get_all(header::SET_COOKIE)
                    .iter()
                    .filter_map(|v| v.to_str().ok())
                    .find_map(|c| c.trim().strip_prefix("JWT="))
                    .map(|c| c.split(';').next().unwrap_or("").to_owned())
                    .filter(|t| !t.is_empty())
            });
        Ok((token, parse_json(res).await?))
    }
}

async fn read_body(mut res: Response) -> Result<Vec<u8>, WfmError> {
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(network)? {
        if body.len() + chunk.len() > MAX_BODY {
            return Err(WfmError::invalid("response exceeded 8 MiB"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

async fn parse_json(res: Response) -> Result<Value, WfmError> {
    let body = read_body(res).await?;
    if body.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&body).map_err(|_| WfmError::invalid("invalid JSON in response"))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::super::mock::{reply, spawn};
    use super::*;

    type Events = Arc<Mutex<Vec<Option<String>>>>;

    fn client(base: &str) -> (WfmClient, Events) {
        let events: Events = Arc::default();
        let log = events.clone();
        let c = WfmClient::new(
            &format!("{base}/v1"),
            &format!("{base}/v2"),
            Box::new(move |t| log.lock().unwrap().push(t.map(str::to_owned))),
        )
        .unwrap();
        (c, events)
    }

    async fn get(c: &WfmClient, api: Api, path: &str) -> Result<Value, WfmError> {
        c.request(api, Method::GET, path, None).await
    }

    #[tokio::test]
    async fn bearer_on_v2_jwt_on_v1_with_identifiable_ua() {
        let m = spawn(vec![reply(200, "{}"), reply(200, "{}")]);
        let (c, _) = client(&m.url);
        c.set_token(Some("tok".into()));
        get(&c, Api::V2, "/me").await.unwrap();
        get(&c, Api::V1, "/profile").await.unwrap();
        let reqs = m.requests();
        assert!(reqs[0].starts_with("get /v2/me "), "{}", reqs[0]);
        assert!(reqs[0].contains("authorization: bearer tok\r\n"));
        assert!(reqs[0].contains("auth_type: header\r\n"));
        assert!(reqs[0].contains(&format!(
            "user-agent: companionframe/{}",
            env!("CARGO_PKG_VERSION")
        )));
        assert!(reqs[1].starts_with("get /v1/profile "));
        assert!(reqs[1].contains("authorization: jwt tok\r\n"));
    }

    #[tokio::test]
    async fn anonymous_requests_carry_no_auth() {
        let m = spawn(vec![reply(200, "{}")]);
        let (c, _) = client(&m.url);
        get(&c, Api::V2, "/items").await.unwrap();
        let r = &m.requests()[0];
        assert!(
            !r.contains("authorization:") && !r.contains("cookie:") && !r.contains("auth_type")
        );
    }

    #[tokio::test]
    async fn adopts_rotated_token_from_any_scheme() {
        let m = spawn(vec![
            reply(200, "{}").header("Authorization", "Bearer rotated"),
            reply(200, "{}").header("Authorization", "JWT rotated2"),
            reply(200, "{}"),
        ]);
        let (c, events) = client(&m.url);
        c.set_token(Some("old".into()));
        get(&c, Api::V2, "/me").await.unwrap();
        assert_eq!(c.token().as_deref(), Some("rotated"));
        get(&c, Api::V2, "/me").await.unwrap();
        get(&c, Api::V2, "/me").await.unwrap();
        assert_eq!(c.token().as_deref(), Some("rotated2"));
        assert_eq!(
            *events.lock().unwrap(),
            [Some("rotated".into()), Some("rotated2".into())]
        );
        assert!(m.requests()[2].contains("authorization: bearer rotated2\r\n"));
    }

    #[tokio::test]
    async fn anonymous_response_token_is_not_adopted() {
        let m = spawn(vec![reply(200, "{}").header("Authorization", "JWT sneaky")]);
        let (c, events) = client(&m.url);
        get(&c, Api::V2, "/items").await.unwrap();
        assert_eq!(c.token(), None);
        assert!(events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unauthorized_expires_session_without_retry() {
        let m = spawn(vec![reply(401, r#"{"error":"app.errors.unauthorized"}"#)]);
        let (c, events) = client(&m.url);
        c.set_token(Some("tok".into()));
        let err = get(&c, Api::V2, "/me").await.unwrap_err();
        assert_eq!(
            err,
            WfmError::Unauthorized {
                detail: "app.errors.unauthorized".into()
            }
        );
        assert_eq!(c.token(), None);
        assert_eq!(*events.lock().unwrap(), [None]);
        assert_eq!(m.requests().len(), 1, "no retry");
    }

    #[tokio::test]
    async fn rate_limit_reports_retry_after() {
        let m = spawn(vec![reply(429, "").header("Retry-After", "7")]);
        let (c, _) = client(&m.url);
        let err = get(&c, Api::V2, "/items").await.unwrap_err();
        assert_eq!(
            err,
            WfmError::RateLimited {
                retry_after_secs: Some(7)
            }
        );
        assert_eq!(err.class(), Class::Transient);
    }

    #[tokio::test]
    async fn too_many_connections_509_is_a_rate_limit() {
        let m = spawn(vec![reply(509, "")]);
        let (c, _) = client(&m.url);
        let err = get(&c, Api::V2, "/items").await.unwrap_err();
        assert_eq!(
            err,
            WfmError::RateLimited {
                retry_after_secs: None
            }
        );
    }

    #[tokio::test]
    async fn status_classification_over_http() {
        for (status, class) in [
            (400, Class::NoData),
            (404, Class::NoData),
            (403, Class::Transient),
            (408, Class::Transient),
            (503, Class::Transient),
        ] {
            let m = spawn(vec![reply(status, "")]);
            let (c, _) = client(&m.url);
            let err = get(&c, Api::V2, "/x").await.unwrap_err();
            assert_eq!(err.class(), class, "{status}");
        }
    }

    #[tokio::test]
    async fn network_failure_is_transient() {
        let (c, _) = client("http://127.0.0.1:1");
        assert_eq!(
            get(&c, Api::V2, "/x").await.unwrap_err().class(),
            Class::Transient
        );
    }

    #[tokio::test]
    async fn fallback_to_v1_on_transient_only() {
        let m = spawn(vec![reply(503, ""), reply(200, r#"{"from":"v1"}"#)]);
        let (c, _) = client(&m.url);
        assert_eq!(c.get_with_fallback("/a", "/b").await.unwrap()["from"], "v1");
        assert!(m.requests()[1].starts_with("get /v1/b "));

        let m = spawn(vec![reply(404, ""), reply(200, "{}")]);
        let (c, _) = client(&m.url);
        let err = c.get_with_fallback("/a", "/b").await.unwrap_err();
        assert_eq!(err.class(), Class::NoData);
        assert_eq!(m.requests().len(), 1, "no data must not fall back");

        let m = spawn(vec![reply(503, ""), reply(403, "")]);
        let (c, _) = client(&m.url);
        let err = c.get_with_fallback("/a", "/b").await.unwrap_err();
        assert_eq!(
            err.class(),
            Class::Transient,
            "both failing is transient, not no-data"
        );
    }

    #[tokio::test]
    async fn sign_in_sends_header_mode_and_reads_token() {
        let m = spawn(vec![
            reply(200, r#"{"payload":{"user":{}}}"#).header("Authorization", "JWT abc"),
        ]);
        let (c, _) = client(&m.url);
        let (tok, _) = c
            .sign_in(Zeroizing::new(r#"{"email":"e"}"#.into()))
            .await
            .unwrap();
        assert_eq!(tok.as_deref(), Some("abc"));
        let r = &m.requests()[0];
        assert!(r.starts_with("post /v1/auth/signin "));
        assert!(r.contains("authorization: jwt\r\n"), "bare scheme: {r}");
        assert!(r.contains("auth_type: header\r\n"));
        assert!(r.ends_with(r#"{"email":"e"}"#));
    }

    #[tokio::test]
    async fn sign_in_falls_back_to_cookie_token() {
        let m = spawn(vec![
            reply(200, "{}").header("Set-Cookie", "JWT=cook; Path=/; HttpOnly"),
        ]);
        let (c, _) = client(&m.url);
        let (tok, _) = c.sign_in(Zeroizing::new("{}".into())).await.unwrap();
        assert_eq!(tok.as_deref(), Some("cook"));
    }

    #[tokio::test]
    async fn sign_in_bad_credentials_surface_detail_and_do_not_retry() {
        let m = spawn(vec![reply(
            401,
            r#"{"error":{"email":["app.account.email_not_found"]}}"#,
        )]);
        let (c, events) = client(&m.url);
        let err = c.sign_in(Zeroizing::new("{}".into())).await.unwrap_err();
        assert!(
            matches!(&err, WfmError::Unauthorized { detail } if detail.contains("app.account.")),
            "{err}"
        );
        assert_eq!(m.requests().len(), 1);
        assert!(events.lock().unwrap().is_empty());
    }
}
