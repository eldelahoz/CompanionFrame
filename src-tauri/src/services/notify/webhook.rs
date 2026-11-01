//! Webhook payloads and transport. The URL is a secret: nothing here logs it, and transport
//! errors are stripped of it before they are described.

use std::time::Duration;

use serde_json::{Map, Value, json};

use super::NotificationDispatch;
use super::channels::{BoxFuture, WebhookChannel};

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 16 * 1024;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);
const DEFAULT_RETRY_DELAY: Duration = Duration::from_secs(1);
const DISCORD_CONTENT_LIMIT: usize = 2_000;

#[derive(Clone, Debug, PartialEq)]
pub struct SendOutcome {
    pub ok: bool,
    pub rate_limited: bool,
    pub retry_after: Duration,
    pub error: String,
}

impl SendOutcome {
    pub fn sent() -> Self {
        Self {
            ok: true,
            rate_limited: false,
            retry_after: Duration::ZERO,
            error: String::new(),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            rate_limited: false,
            retry_after: Duration::ZERO,
            error: error.into(),
        }
    }
}

/// HTTP seam: the real one is reqwest, tests plug in a script.
pub trait Transport: Send + Sync {
    fn post(&self, url: String, body: String) -> BoxFuture<'_, SendOutcome>;
}

pub fn build_body(
    channel: WebhookChannel,
    payload: &NotificationDispatch,
    at: &str,
    ping_user_id: &str,
) -> String {
    if channel == WebhookChannel::Discord {
        let title = payload.title.trim();
        let body = payload.body.trim();
        let content = if body.is_empty() {
            format!("**{title}**")
        } else {
            format!("**{title}**\n{body}")
        };
        let mention = if ping_user_id.is_empty() {
            String::new()
        } else {
            format!("<@{ping_user_id}> ")
        };
        let room = DISCORD_CONTENT_LIMIT.saturating_sub(mention.chars().count());
        let content: String = mention.chars().chain(content.chars().take(room)).collect();
        // Item and player names reach Discord verbatim, so only the configured user may be pinged.
        let allowed = if ping_user_id.is_empty() {
            json!({ "parse": [] })
        } else {
            json!({ "parse": [], "users": [ping_user_id] })
        };
        return json!({ "content": content, "allowed_mentions": allowed }).to_string();
    }
    let mut generic = Map::new();
    generic.insert(
        "source".into(),
        serde_json::to_value(payload.source).unwrap(),
    );
    generic.insert("title".into(), payload.title.clone().into());
    generic.insert("body".into(), payload.body.clone().into());
    generic.insert("timestamp".into(), at.into());
    if let Some(meta) = &payload.meta {
        generic.insert("meta".into(), Value::Object(meta.clone()));
    }
    Value::Object(generic).to_string()
}

/// Discord reports whole seconds in the header and a float in the JSON body; the body wins.
pub fn retry_delay(header: Option<&str>, body: &str) -> Duration {
    let mut seconds = header.and_then(|h| h.trim().parse::<f64>().ok());
    if let Some(v) = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("retry_after").and_then(Value::as_f64))
        .filter(|v| v.is_finite())
    {
        seconds = Some(v);
    }
    match seconds {
        Some(s) if s.is_finite() && s >= 0.0 => {
            Duration::from_millis((s * 1000.0).round() as u64).min(MAX_RETRY_DELAY)
        }
        _ => DEFAULT_RETRY_DELAY,
    }
}

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            // A redirect could carry the secret to another host.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self { client })
    }

    async fn send(&self, url: String, body: String) -> SendOutcome {
        let sent = self
            .client
            .post(url)
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await;
        let mut res = match sent {
            Ok(res) => res,
            Err(e) => return SendOutcome::failed(e.without_url().to_string()),
        };
        let status = res.status().as_u16();
        let header = res
            .headers()
            .get("retry-after")
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        // The body only matters for the 429 hint; stop reading at the cap and drop the rest.
        let mut text = Vec::new();
        while text.len() < MAX_RESPONSE_BYTES {
            match res.chunk().await {
                Ok(Some(chunk)) => text.extend_from_slice(&chunk),
                _ => break,
            }
        }
        text.truncate(MAX_RESPONSE_BYTES);
        let text = String::from_utf8_lossy(&text);
        match status {
            300..=399 => SendOutcome::failed(format!("redirect refused ({status})")),
            429 => SendOutcome {
                ok: false,
                rate_limited: true,
                retry_after: retry_delay(header.as_deref(), &text),
                error: "429".into(),
            },
            200..=299 => SendOutcome::sent(),
            _ => SendOutcome::failed(format!("http {status}")),
        }
    }
}

impl Transport for ReqwestTransport {
    fn post(&self, url: String, body: String) -> BoxFuture<'_, SendOutcome> {
        Box::pin(self.send(url, body))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use super::super::channels::NotificationSource;
    use super::*;

    fn dispatch(title: &str, body: &str) -> NotificationDispatch {
        NotificationDispatch::new(NotificationSource::WorldState, title, body)
    }

    const PING: &str = "123456789012345678";

    #[test]
    fn discord_payload_is_unchanged_without_a_ping() {
        assert_eq!(
            build_body(
                WebhookChannel::Discord,
                &dispatch("Baro", "arrived"),
                "t",
                ""
            ),
            r#"{"content":"**Baro**\narrived","allowed_mentions":{"parse":[]}}"#
        );
    }

    #[test]
    fn suppresses_every_mention_the_text_could_carry() {
        let body = build_body(
            WebhookChannel::Discord,
            &dispatch("@everyone", "<@1234> ping"),
            "t",
            "",
        );
        let sent: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["allowed_mentions"], json!({ "parse": [] }));
    }

    #[test]
    fn pings_only_the_configured_user() {
        let body = build_body(
            WebhookChannel::Discord,
            &dispatch("@everyone", "<@&42> <@1234> ping"),
            "t",
            PING,
        );
        let sent: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            sent,
            json!({
                "content": format!("<@{PING}> **@everyone**\n<@&42> <@1234> ping"),
                "allowed_mentions": { "parse": [], "users": [PING] },
            })
        );
    }

    #[test]
    fn keeps_the_ping_inside_discords_content_limit() {
        let body = build_body(
            WebhookChannel::Discord,
            &dispatch("Baro", &"x".repeat(3000)),
            "t",
            PING,
        );
        let sent: Value = serde_json::from_str(&body).unwrap();
        let content = sent["content"].as_str().unwrap();
        assert!(content.starts_with(&format!("<@{PING}> **Baro**\n")));
        assert_eq!(content.chars().count(), 2000);
    }

    #[test]
    fn the_generic_payload_carries_meta_and_never_the_ping() {
        let mut d = dispatch("Baro", "arrived");
        d.meta = Some(Map::from_iter([(
            "location".into(),
            "Larunda Relay".into(),
        )]));
        let body = build_body(
            WebhookChannel::Generic,
            &d,
            "2026-01-01T00:00:00.000Z",
            PING,
        );
        assert!(!body.contains(PING));
        let sent: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            sent,
            json!({
                "source": "worldState",
                "title": "Baro",
                "body": "arrived",
                "timestamp": "2026-01-01T00:00:00.000Z",
                "meta": { "location": "Larunda Relay" },
            })
        );
    }

    #[test]
    fn retry_delay_prefers_the_body_and_stays_bounded() {
        let none = Duration::from_secs(1);
        assert_eq!(retry_delay(Some("2"), ""), Duration::from_secs(2));
        assert_eq!(
            retry_delay(Some("2"), r#"{"retry_after":0.25}"#),
            Duration::from_millis(250)
        );
        assert_eq!(retry_delay(None, "not json"), none);
        assert_eq!(retry_delay(Some("-3"), ""), none);
        assert_eq!(retry_delay(Some("nope"), ""), none);
        assert_eq!(retry_delay(Some("9999"), ""), Duration::from_secs(60));
    }

    /// One-shot local HTTP server: answers with `reply` and reports the request it saw.
    fn serve(reply: Vec<u8>) -> (String, Arc<Mutex<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/hook", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(String::new()));
        let seen_in = seen.clone();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            *seen_in.lock().unwrap() = String::from_utf8_lossy(&buf[..n]).into_owned();
            let _ = stream.write_all(&reply);
        });
        (url, seen)
    }

    fn http(status: &str, headers: &str, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    #[tokio::test]
    async fn posts_json_and_accepts_a_204() {
        let (url, seen) = serve(http("204 No Content", "", ""));
        let out = ReqwestTransport::new()
            .unwrap()
            .post(url, "{\"a\":1}".into())
            .await;
        assert_eq!(out, SendOutcome::sent());
        let request = seen.lock().unwrap().to_lowercase();
        assert!(request.starts_with("post /hook"));
        assert!(request.contains("content-type: application/json"));
    }

    #[tokio::test]
    async fn treats_a_redirect_as_a_failure_instead_of_following_it() {
        let (url, _) = serve(http(
            "302 Found",
            "Location: http://127.0.0.1:1/elsewhere\r\n",
            "",
        ));
        let out = ReqwestTransport::new()
            .unwrap()
            .post(url, "{}".into())
            .await;
        assert_eq!(out, SendOutcome::failed("redirect refused (302)"));
    }

    #[tokio::test]
    async fn reads_the_rate_limit_hint() {
        let (url, _) = serve(http(
            "429 Too Many Requests",
            "Retry-After: 7\r\n",
            r#"{"retry_after":0.5}"#,
        ));
        let out = ReqwestTransport::new()
            .unwrap()
            .post(url, "{}".into())
            .await;
        assert!(out.rate_limited && !out.ok);
        assert_eq!(out.retry_after, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn reports_a_server_error_by_status_only() {
        let (url, _) = serve(http("500 Oops", "", "secret-looking body"));
        let out = ReqwestTransport::new()
            .unwrap()
            .post(url, "{}".into())
            .await;
        assert_eq!(out, SendOutcome::failed("http 500"));
    }

    #[tokio::test]
    async fn a_transport_error_never_carries_the_url() {
        // Nothing listens on the port the bound-then-dropped listener had.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("http://127.0.0.1:{port}/webhooks/1234/abcdefgh");
        let out = ReqwestTransport::new()
            .unwrap()
            .post(url, "{}".into())
            .await;
        assert!(!out.ok);
        assert!(!out.error.contains("webhooks/1234") && !out.error.contains("abcdefgh"));
    }

    #[tokio::test]
    async fn stops_reading_a_response_body_at_the_size_cap() {
        let (url, _) = serve(http("204 No Content", "", &"A".repeat(200 * 1024)));
        let out = ReqwestTransport::new()
            .unwrap()
            .post(url, "{}".into())
            .await;
        assert_eq!(out, SendOutcome::sent());
    }
}
