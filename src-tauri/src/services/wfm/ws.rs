//! warframe.market WebSocket (docs/wfm-auth.md section 4): subprotocol `wfm`, sign in with the session
//! token, reconnect with backoff, give up after repeated sign-in rejection.
//!
//! One task owns the socket. The token lives in a `watch` so a rotation (pushed from the client's
//! `TokenSink`) is what the next reconnect presents, never the dead one.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Instant, interval, sleep, timeout};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;

pub const URL: &str = "wss://ws.warframe.market/socket";
const PROTOCOL: &str = "wfm";
/// WFHelper sent this; the server accepts the handshake and sign-in without it, so production passes `None`.
pub const ORIGIN: &str = "https://warframe.market";
/// The edge answers 403 to a handshake without a User-Agent (probed 2026-10-25, docs/wfm-auth.md).
const USER_AGENT: &str = concat!("CompanionFrame/", env!("CARGO_PKG_VERSION"));
const MAX_SIGNIN_FAILURES: u32 = 3;
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Timing {
    pub base: Duration,
    pub cap: Duration,
    pub jitter_ms: u64,
    /// A connection that stayed signed in this long resets the backoff.
    pub stable: Duration,
    pub ping: Duration,
    pub connect: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            base: Duration::from_secs(1),
            cap: Duration::from_secs(60),
            jitter_ms: 500,
            stable: Duration::from_secs(60),
            ping: Duration::from_secs(30),
            connect: Duration::from_secs(15),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WsStatus {
    Connecting,
    Connected,
    Reconnecting,
    /// Sign-in was rejected repeatedly: stays down until the next manual login.
    GaveUp,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WsEvent {
    pub route: String,
    pub payload: Value,
}

pub enum Out {
    Status(WsStatus),
    Event(WsEvent),
    /// The `newOrders` feed is confirmed on (`true`) or was switched off (`false`). A dropped
    /// connection is a `Status` change, not a `Subscribed(false)`.
    Subscribed(bool),
}

type SetReply = oneshot::Sender<Result<Option<String>, String>>;

enum Cmd {
    SetStatus {
        status: String,
        duration: Option<u64>,
        reply: SetReply,
    },
}

pub struct Listener {
    token: watch::Sender<Option<String>>,
    /// Whether the `newOrders` feed should be on; the socket re-applies it after every sign-in.
    feed: watch::Sender<bool>,
    cmd: mpsc::Sender<Cmd>,
    task: JoinHandle<()>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn message_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..11].to_owned()
}

fn frame(route: &str, payload: Value) -> Message {
    Message::text(json!({ "route": route, "payload": payload, "id": message_id() }).to_string())
}

fn delay(attempt: u32, t: &Timing) -> Duration {
    let base = t.base.saturating_mul(1u32 << attempt.min(16)).min(t.cap);
    base + Duration::from_millis(rand_below(t.jitter_ms))
}

fn rand_below(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    u64::from(uuid::Uuid::new_v4().as_u128() as u32) % n
}

impl Listener {
    pub fn start(
        url: &str,
        origin: Option<&str>,
        token: Option<String>,
        timing: Timing,
        out: impl Fn(Out) + Send + Sync + 'static,
    ) -> Self {
        let (token_tx, token_rx) = watch::channel(token);
        let (cmd, cmd_rx) = mpsc::channel(8);
        let (feed, feed_rx) = watch::channel(false);
        let task = spawn(run(
            url.to_owned(),
            origin.map(str::to_owned),
            token_rx,
            feed_rx,
            cmd_rx,
            timing,
            out,
        ));
        Self {
            token: token_tx,
            feed,
            cmd,
            task,
        }
    }

    /// Token rotation (`Some`) or sign-out (`None`). Does not touch a live socket.
    pub fn set_token(&self, token: Option<String>) {
        self.token.send_if_modified(|cur| {
            let changed = *cur != token;
            *cur = token;
            changed
        });
    }

    /// Turn the live new-orders feed on or off (sent once signed in, and again after a reconnect).
    pub fn set_feed(&self, on: bool) {
        self.feed
            .send_if_modified(|cur| std::mem::replace(cur, on) != on);
    }

    /// Ask WFM to set the account status; resolves with the server-side expiry, if any.
    pub async fn set_status(
        &self,
        status: &str,
        duration_secs: Option<u64>,
    ) -> Result<Option<String>, String> {
        let (reply, rx) = oneshot::channel();
        self.cmd
            .send(Cmd::SetStatus {
                status: status.to_owned(),
                duration: duration_secs,
                reply,
            })
            .await
            .map_err(|_| "websocket stopped".to_owned())?;
        rx.await.map_err(|_| "websocket stopped".to_owned())?
    }
}

enum End {
    SignInRejected,
    /// Closed or errored; `held` says the sign-in lasted long enough to reset the backoff.
    Closed {
        held: bool,
    },
}

async fn run(
    url: String,
    origin: Option<String>,
    mut token: watch::Receiver<Option<String>>,
    mut feed: watch::Receiver<bool>,
    mut cmd: mpsc::Receiver<Cmd>,
    t: Timing,
    out: impl Fn(Out),
) {
    let (mut attempt, mut fails) = (0u32, 0u32);
    loop {
        let Some(tok) = token.borrow_and_update().clone() else {
            out(Out::Status(WsStatus::Stopped));
            if token.changed().await.is_err() {
                return;
            }
            continue;
        };
        out(Out::Status(WsStatus::Connecting));
        match session(
            &url,
            origin.as_deref(),
            &tok,
            &mut feed,
            &mut cmd,
            &t,
            &out,
            &mut fails,
        )
        .await
        {
            End::SignInRejected if fails >= MAX_SIGNIN_FAILURES => {
                tracing::warn!("wfm ws: sign-in rejected repeatedly, stopping");
                out(Out::Status(WsStatus::GaveUp));
                // Only a new token (manual login) revives it.
                while token.changed().await.is_ok() {
                    if token.borrow().is_some() {
                        (attempt, fails) = (0, 0);
                        break;
                    }
                }
                if token.has_changed().is_err() {
                    return;
                }
                continue;
            }
            End::Closed { held: true } => attempt = 0,
            _ => {}
        }
        out(Out::Status(WsStatus::Reconnecting));
        sleep(delay(attempt, &t)).await;
        attempt += 1;
    }
}

#[allow(clippy::too_many_arguments)] // the per-connection channels and state; a struct would only rename them
async fn session(
    url: &str,
    origin: Option<&str>,
    token: &str,
    feed: &mut watch::Receiver<bool>,
    cmd: &mut mpsc::Receiver<Cmd>,
    t: &Timing,
    out: &impl Fn(Out),
    fails: &mut u32,
) -> End {
    let closed = |held| End::Closed { held };
    let Ok(mut req) = url.into_client_request() else {
        return closed(false);
    };
    let h = req.headers_mut();
    h.insert("Sec-WebSocket-Protocol", HeaderValue::from_static(PROTOCOL));
    h.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
    if let Some(o) = origin.and_then(|o| HeaderValue::from_str(o).ok()) {
        h.insert("Origin", o);
    }
    let cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES));
    let conn = timeout(t.connect, connect_async_with_config(req, Some(cfg), false)).await;
    let Ok(Ok((mut ws, _))) = conn else {
        tracing::warn!("wfm ws: connect failed");
        return closed(false);
    };
    if ws
        .send(frame("@wfm|cmd/auth/signIn", json!({ "token": token })))
        .await
        .is_err()
    {
        return closed(false);
    }

    let mut authed_at: Option<Instant> = None;
    let mut pending: Option<SetReply> = None;
    let mut feed_sent = false;
    let mut ping = interval(t.ping);
    ping.tick().await;
    let end = loop {
        tokio::select! {
            msg = ws.next() => {
                let text = match msg {
                    Some(Ok(Message::Text(s))) => s,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break closed(false),
                    Some(Ok(_)) => continue,
                };
                let Ok(Value::Object(mut m)) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                let route = match m.remove("route") {
                    Some(Value::String(r)) => r,
                    _ => continue,
                };
                let payload = m.remove("payload").unwrap_or(Value::Null);
                tracing::debug!(%route, "wfm ws <-");
                if route.contains("auth/signIn:ok") {
                    *fails = 0;
                    authed_at = Some(Instant::now());
                    out(Out::Status(WsStatus::Connected));
                    let want = *feed.borrow_and_update();
                    if !sync_feed(&mut ws, want, &mut feed_sent).await {
                        break closed(false);
                    }
                } else if route.contains("auth/signIn:error") {
                    *fails += 1;
                    break End::SignInRejected;
                } else if route.contains("status/set:ok") {
                    if let Some(r) = pending.take() {
                        let until = payload.get("statusUntil").and_then(Value::as_str);
                        let _ = r.send(Ok(until.map(str::to_owned)));
                    }
                } else if route.contains("unsubscribe/newOrders") {
                    // Nothing to learn: `notSubscribed` just means it was already off.
                } else if route.contains("subscribe/newOrders") {
                    let ok = route.ends_with(":ok")
                        || payload.to_string().contains("alreadySubscribed");
                    if ok && feed_sent {
                        out(Out::Subscribed(true));
                    } else if !ok {
                        tracing::warn!(%route, "wfm ws: newOrders subscribe failed");
                    }
                } else if route.ends_with(":error") {
                    tracing::warn!(%route, "wfm ws server error");
                    if let Some(r) = pending.take() {
                        let _ = r.send(Err(format!("{route}: {payload}")));
                    }
                } else if !route.contains("auth/") {
                    out(Out::Event(WsEvent { route, payload }));
                }
            }
            Ok(()) = feed.changed() => {
                let want = *feed.borrow_and_update();
                if authed_at.is_some() {
                    if !want && feed_sent {
                        out(Out::Subscribed(false));
                    }
                    if !sync_feed(&mut ws, want, &mut feed_sent).await {
                        break closed(false);
                    }
                }
            }
            Some(c) = cmd.recv() => {
                let Cmd::SetStatus { status, duration, reply } = c;
                if authed_at.is_none() || pending.is_some() {
                    let _ = reply.send(Err("websocket not ready".into()));
                } else if ws.send(frame("@wfm|cmd/status/set", json!({ "status": status, "duration": duration }))).await.is_err() {
                    let _ = reply.send(Err("websocket closed".into()));
                    break closed(false);
                } else {
                    pending = Some(reply);
                }
            }
            _ = ping.tick() => {
                if ws.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break closed(false);
                }
            }
        }
    };
    if let Some(r) = pending {
        let _ = r.send(Err("websocket closed".into()));
    }
    match end {
        End::Closed { .. } => closed(authed_at.is_some_and(|a| a.elapsed() >= t.stable)),
        e => e,
    }
}

/// Sends subscribe/unsubscribe when the wanted state differs from the one last sent on this
/// connection. `false` means the socket is gone.
async fn sync_feed<S: futures_util::Sink<Message> + Unpin>(
    ws: &mut S,
    want: bool,
    sent: &mut bool,
) -> bool {
    if want == *sent {
        return true;
    }
    *sent = want;
    let route = if want {
        "@wfm|cmd/subscribe/newOrders"
    } else {
        "@wfm|cmd/unsubscribe/newOrders"
    };
    ws.send(frame(
        route,
        json!({ "platform": "pc", "crossplay": false }),
    ))
    .await
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    enum Act {
        Send(&'static str, Value),
        Close,
    }

    type Script = Arc<dyn Fn(usize, &str, &Value) -> Vec<Act> + Send + Sync>;

    #[derive(Default)]
    struct Seen {
        handshakes: Vec<(Option<String>, Option<String>)>,
        frames: Vec<(usize, String, Value)>,
    }

    #[allow(clippy::result_large_err)] // the handshake callback signature is tungstenite's
    /// Local WS server: `script(conn, route, payload)` says how to answer each client frame.
    async fn serve(script: Script) -> (String, Arc<Mutex<Seen>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Seen::default()));
        let s2 = seen.clone();
        tokio::spawn(async move {
            let mut idx = 0;
            while let Ok((tcp, _)) = listener.accept().await {
                let (conn, script, seen) = (idx, script.clone(), s2.clone());
                idx += 1;
                tokio::spawn(async move {
                    let seen_hs = seen.clone();
                    let cb = move |req: &Request, mut resp: Response| {
                        let h = |k: &str| {
                            req.headers()
                                .get(k)
                                .and_then(|v| v.to_str().ok())
                                .map(str::to_owned)
                        };
                        seen_hs
                            .lock()
                            .unwrap()
                            .handshakes
                            .push((h("origin"), h("sec-websocket-protocol")));
                        resp.headers_mut()
                            .insert("Sec-WebSocket-Protocol", HeaderValue::from_static("wfm"));
                        Ok(resp)
                    };
                    let Ok(mut ws) = accept_hdr_async(tcp, cb).await else {
                        return;
                    };
                    while let Some(Ok(Message::Text(t))) = ws.next().await {
                        let v: Value = serde_json::from_str(t.as_str()).unwrap();
                        let route = v["route"].as_str().unwrap().to_owned();
                        seen.lock().unwrap().frames.push((
                            conn,
                            route.clone(),
                            v["payload"].clone(),
                        ));
                        for a in script(conn, &route, &v["payload"]) {
                            match a {
                                Act::Send(r, p) => {
                                    let _ = ws.send(frame(r, p)).await;
                                }
                                Act::Close => {
                                    let _ = ws.close(None).await;
                                    return;
                                }
                            }
                        }
                    }
                });
            }
        });
        (url, seen)
    }

    fn fast() -> Timing {
        Timing {
            base: Duration::from_millis(10),
            cap: Duration::from_millis(40),
            jitter_ms: 0,
            stable: Duration::from_secs(60),
            ping: Duration::from_secs(60),
            connect: Duration::from_secs(2),
        }
    }

    fn ok() -> Act {
        Act::Send("@wfm|cmd/auth/signIn:ok", json!({}))
    }

    fn err() -> Act {
        Act::Send("@wfm|cmd/auth/signIn:error", json!({ "code": 401 }))
    }

    type Log = Arc<Mutex<Vec<String>>>;
    type Events = Arc<Mutex<Vec<WsEvent>>>;

    fn start(url: &str, token: Option<&str>, origin: Option<&str>) -> (Listener, Log, Events) {
        let (log, events): (Log, Events) = Default::default();
        let (l2, e2) = (log.clone(), events.clone());
        let l = Listener::start(
            url,
            origin,
            token.map(str::to_owned),
            fast(),
            move |o| match o {
                Out::Status(s) => l2.lock().unwrap().push(format!("{s:?}")),
                Out::Event(e) => e2.lock().unwrap().push(e),
                Out::Subscribed(_) => {}
            },
        );
        (l, log, events)
    }

    async fn until(mut f: impl FnMut() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not met in time");
    }

    fn conns(seen: &Arc<Mutex<Seen>>) -> usize {
        seen.lock().unwrap().handshakes.len()
    }

    fn has(log: &Log, s: &str) -> bool {
        log.lock().unwrap().iter().any(|x| x == s)
    }

    #[tokio::test]
    async fn signs_in_with_subprotocol_origin_and_forwards_events() {
        let (url, seen) = serve(Arc::new(|_, route, _| match route {
            "@wfm|cmd/auth/signIn" => vec![ok(), Act::Send("event/orders/new", json!({ "id": 1 }))],
            _ => vec![],
        }))
        .await;
        let (_l, log, events) = start(&url, Some("tok"), Some(ORIGIN));
        until(|| !events.lock().unwrap().is_empty()).await;
        assert_eq!(events.lock().unwrap()[0].route, "event/orders/new");
        assert!(has(&log, "Connected"));
        let s = seen.lock().unwrap();
        assert_eq!(s.handshakes[0], (Some(ORIGIN.into()), Some("wfm".into())));
        assert_eq!(s.frames[0].2, json!({ "token": "tok" }));
    }

    #[tokio::test]
    async fn no_origin_header_when_none_configured() {
        let (url, seen) = serve(Arc::new(|_, _, _| vec![ok()])).await;
        let (_l, log, _) = start(&url, Some("t"), None);
        until(|| has(&log, "Connected")).await;
        assert_eq!(seen.lock().unwrap().handshakes[0].0, None);
    }

    #[tokio::test]
    async fn transient_signin_error_retries_then_recovers() {
        let (url, seen) = serve(Arc::new(|conn, _, _| {
            vec![if conn == 0 { err() } else { ok() }]
        }))
        .await;
        let (_l, log, _) = start(&url, Some("t"), None);
        until(|| has(&log, "Connected")).await;
        assert_eq!(conns(&seen), 2);
    }

    #[tokio::test]
    async fn three_signin_rejections_give_up_until_a_new_token() {
        let (url, seen) = serve(Arc::new(|_, _, _| vec![err()])).await;
        let (l, log, _) = start(&url, Some("t"), None);
        until(|| has(&log, "GaveUp")).await;
        sleep(Duration::from_millis(200)).await;
        assert_eq!(conns(&seen), 3);
        l.set_token(Some("fresh".into()));
        until(|| conns(&seen) == 4).await;
        assert_eq!(
            seen.lock().unwrap().frames[3].2,
            json!({ "token": "fresh" })
        );
    }

    #[tokio::test]
    async fn a_successful_signin_resets_the_failure_count() {
        // fail, ok then drop, fail, fail, ok: never three in a row, so it must not give up.
        let (url, _) = serve(Arc::new(|conn, _, _| match conn {
            1 => vec![ok(), Act::Close],
            0 | 2 | 3 => vec![err()],
            _ => vec![ok()],
        }))
        .await;
        let (_l, log, _) = start(&url, Some("t"), None);
        until(|| {
            log.lock()
                .unwrap()
                .iter()
                .filter(|s| *s == "Connected")
                .count()
                >= 2
        })
        .await;
        assert!(!has(&log, "GaveUp"));
    }

    #[tokio::test]
    async fn reconnect_presents_the_rotated_token() {
        let (url, seen) = serve(Arc::new(|conn, _, _| {
            if conn == 0 {
                vec![ok(), Act::Close]
            } else {
                vec![ok()]
            }
        }))
        .await;
        let (l, _, _) = start(&url, Some("first"), None);
        l.set_token(Some("second".into()));
        until(|| conns(&seen) >= 2).await;
        let s = seen.lock().unwrap();
        // The first connect may or may not have read "first"; the reconnect must carry "second".
        assert_eq!(s.frames.last().unwrap().2, json!({ "token": "second" }));
    }

    #[tokio::test]
    async fn set_status_resolves_with_expiry_and_reports_errors() {
        let (url, seen) = serve(Arc::new(|_, route, p| match route {
            "@wfm|cmd/auth/signIn" => vec![ok()],
            _ if p["status"] == "bad" => {
                vec![Act::Send(
                    "@wfm|cmd/status/set:error",
                    json!({ "code": 400 }),
                )]
            }
            _ => vec![Act::Send(
                "@wfm|cmd/status/set:ok",
                json!({ "statusUntil": "2030-01-01T00:00:00Z" }),
            )],
        }))
        .await;
        let (l, log, _) = start(&url, Some("t"), None);
        until(|| has(&log, "Connected")).await;
        let until_ = l.set_status("ingame", Some(1800)).await.unwrap();
        assert_eq!(until_.as_deref(), Some("2030-01-01T00:00:00Z"));
        assert_eq!(
            seen.lock().unwrap().frames[1].2,
            json!({ "status": "ingame", "duration": 1800 })
        );
        assert!(l.set_status("bad", None).await.is_err());
    }

    #[tokio::test]
    async fn set_status_before_signin_fails_fast() {
        let (url, _) = serve(Arc::new(|_, _, _| vec![])).await;
        let (l, _, _) = start(&url, Some("t"), None);
        sleep(Duration::from_millis(50)).await;
        assert!(l.set_status("online", None).await.is_err());
    }

    fn feed_frames(seen: &Arc<Mutex<Seen>>) -> Vec<(usize, String)> {
        seen.lock()
            .unwrap()
            .frames
            .iter()
            .filter(|f| f.1.contains("newOrders"))
            .map(|f| (f.0, f.1.clone()))
            .collect()
    }

    #[tokio::test]
    async fn feed_is_subscribed_after_signin_again_after_reconnect_and_unsubscribed_on_off() {
        let (url, seen) = serve(Arc::new(|conn, route, _| match route {
            "@wfm|cmd/auth/signIn" => vec![ok()],
            "@wfm|cmd/subscribe/newOrders" if conn == 0 => vec![
                Act::Send("@wfm|cmd/subscribe/newOrders:ok", json!({})),
                Act::Close,
            ],
            "@wfm|cmd/subscribe/newOrders" => {
                vec![Act::Send("@wfm|cmd/subscribe/newOrders:ok", json!({}))]
            }
            _ => vec![],
        }))
        .await;
        let (l, _, _) = start(&url, Some("t"), None);
        l.set_feed(true);
        until(|| feed_frames(&seen).len() >= 2).await;
        let f = feed_frames(&seen);
        assert_eq!(f[0], (0, "@wfm|cmd/subscribe/newOrders".to_owned()));
        assert_eq!(f[1], (1, "@wfm|cmd/subscribe/newOrders".to_owned()));
        let payload = seen
            .lock()
            .unwrap()
            .frames
            .iter()
            .find(|f| f.1.contains("newOrders"))
            .unwrap()
            .2
            .clone();
        assert_eq!(payload, json!({ "platform": "pc", "crossplay": false }));
        l.set_feed(false);
        until(|| feed_frames(&seen).len() >= 3).await;
        assert_eq!(feed_frames(&seen)[2].1, "@wfm|cmd/unsubscribe/newOrders");
    }

    #[tokio::test]
    async fn feed_off_sends_nothing_and_confirmation_is_reported() {
        let (url, seen) = serve(Arc::new(|_, route, _| match route {
            "@wfm|cmd/auth/signIn" => vec![ok()],
            "@wfm|cmd/subscribe/newOrders" => {
                vec![Act::Send("@wfm|cmd/subscribe/newOrders:ok", json!({}))]
            }
            _ => vec![],
        }))
        .await;
        let subs: Arc<Mutex<Vec<bool>>> = Default::default();
        let s2 = subs.clone();
        let l = Listener::start(&url, None, Some("t".into()), fast(), move |o| {
            if let Out::Subscribed(b) = o {
                s2.lock().unwrap().push(b);
            }
        });
        sleep(Duration::from_millis(100)).await;
        assert!(feed_frames(&seen).is_empty());
        l.set_feed(true);
        until(|| *subs.lock().unwrap() == [true]).await;
        l.set_feed(false);
        until(|| *subs.lock().unwrap() == [true, false]).await;
    }

    #[test]
    fn start_outside_a_tokio_runtime_does_not_panic() {
        // Tauri's setup hook has no runtime context; this must use the app runtime.
        let l = Listener::start("ws://127.0.0.1:1", None, None, fast(), |_| {});
        drop(l);
    }

    #[tokio::test]
    async fn dropping_the_listener_stops_reconnecting() {
        let (url, seen) = serve(Arc::new(|_, _, _| vec![ok(), Act::Close])).await;
        let (l, _, _) = start(&url, Some("t"), None);
        until(|| conns(&seen) >= 2).await;
        drop(l);
        sleep(Duration::from_millis(50)).await;
        let n = conns(&seen);
        sleep(Duration::from_millis(150)).await;
        assert_eq!(conns(&seen), n);
    }

    #[test]
    fn backoff_doubles_and_caps() {
        let t = Timing {
            jitter_ms: 0,
            ..Timing::default()
        };
        let ms = |a| delay(a, &t).as_millis();
        assert_eq!((ms(0), ms(1), ms(3), ms(20)), (1000, 2000, 8000, 60_000));
    }
}

#[cfg(test)]
mod live_probe {
    use super::*;

    /// Anonymous handshake against the real server, no credentials:
    /// `cargo test live_handshake -- --ignored --nocapture`. Prints the status per header variant.
    #[tokio::test]
    #[ignore = "hits the real warframe.market socket"]
    async fn live_handshake() {
        for (origin, ua) in [(true, false), (false, false), (true, true), (false, true)] {
            let mut req = URL.into_client_request().unwrap();
            let h = req.headers_mut();
            h.insert("Sec-WebSocket-Protocol", HeaderValue::from_static(PROTOCOL));
            if ua {
                h.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
            }
            if origin {
                h.insert("Origin", HeaderValue::from_static(ORIGIN));
            }
            let r = timeout(
                Duration::from_secs(15),
                connect_async_with_config(req, None, false),
            )
            .await;
            let status = match &r {
                Ok(Ok((_, resp))) => resp.status().to_string(),
                Ok(Err(e)) => e.to_string(),
                Err(_) => "timeout".into(),
            };
            println!("origin={origin} ua={ua} -> {status}");
        }
    }
}
