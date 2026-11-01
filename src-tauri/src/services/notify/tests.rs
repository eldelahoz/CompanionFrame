use std::future::pending;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use super::channels::BoxFuture;
use super::channels::testing::FakeResolver;
use super::*;

const DISCORD_URL: &str = "https://discord.com/api/webhooks/1234/abcdefgh";
const GENERIC_URL: &str = "https://hooks.example.com/services/wxyz";
const PING: &str = "123456789012345678";

#[derive(Default)]
struct FakeEffects {
    shown: Mutex<Vec<(String, String)>>,
    recorded: Mutex<Vec<(NotificationKind, String)>>,
    sounds: Mutex<Vec<f32>>,
    running: Mutex<Option<bool>>,
}

impl Effects for FakeEffects {
    fn show(&self, title: &str, body: &str) {
        self.shown.lock().unwrap().push((title.into(), body.into()));
    }
    fn game_running(&self) -> Option<bool> {
        *self.running.lock().unwrap()
    }
    fn record(&self, kind: NotificationKind, title: &str, _: &str) {
        self.recorded.lock().unwrap().push((kind, title.into()));
    }
    fn play_sound(&self, volume: f32) {
        self.sounds.lock().unwrap().push(volume);
    }
}

#[derive(Default)]
struct MemSecrets {
    map: Mutex<BTreeMap<WebhookChannel, String>>,
    /// Writes only (set/delete); reads are lazy and happen on first state lookup.
    writes: Mutex<usize>,
}

impl WebhookSecrets for MemSecrets {
    fn get(&self, c: WebhookChannel) -> Option<String> {
        self.map.lock().unwrap().get(&c).cloned()
    }
    fn set(&self, c: WebhookChannel, url: &str) -> Result<(), String> {
        *self.writes.lock().unwrap() += 1;
        self.map.lock().unwrap().insert(c, url.into());
        Ok(())
    }
    fn delete(&self, c: WebhookChannel) {
        *self.writes.lock().unwrap() += 1;
        self.map.lock().unwrap().remove(&c);
    }
}

/// Answers from a script (then 204 forever) and records every body posted.
#[derive(Default)]
struct ScriptTransport {
    script: Mutex<VecDeque<SendOutcome>>,
    posts: Mutex<Vec<(String, String)>>,
    hang: bool,
}

impl Transport for ScriptTransport {
    fn post(&self, url: String, body: String) -> BoxFuture<'_, SendOutcome> {
        self.posts.lock().unwrap().push((url, body));
        let next = self.script.lock().unwrap().pop_front();
        let hang = self.hang;
        Box::pin(async move {
            if hang {
                pending::<()>().await;
            }
            next.unwrap_or_else(SendOutcome::sent)
        })
    }
}

struct Rig {
    n: Notifications,
    effects: Arc<FakeEffects>,
    secrets: Arc<MemSecrets>,
    transport: Arc<ScriptTransport>,
    dir: PathBuf,
}

impl Rig {
    fn new() -> Self {
        Self::build(
            ScriptTransport::default(),
            Arc::new(MemSecrets::default()),
            None,
        )
    }

    fn with(transport: ScriptTransport) -> Self {
        Self::build(transport, Arc::new(MemSecrets::default()), None)
    }

    fn build(transport: ScriptTransport, secrets: Arc<MemSecrets>, dir: Option<PathBuf>) -> Self {
        let dir = dir.unwrap_or_else(|| {
            let d = std::env::temp_dir().join(format!("cf-notify-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&d).unwrap();
            d
        });
        let effects = Arc::new(FakeEffects::default());
        let transport = Arc::new(transport);
        let n = Notifications::new(
            dir.clone(),
            secrets.clone(),
            transport.clone(),
            Arc::new(FakeResolver::public()),
            effects.clone(),
            Arc::new(SoundStore::new(dir.join(SOUND_FILE))),
        );
        Self {
            n,
            effects,
            secrets,
            transport,
            dir,
        }
    }

    /// A fresh service over the same directory and keyring, as after a restart.
    fn reload(&self) -> Rig {
        Self::build(
            ScriptTransport::default(),
            self.secrets.clone(),
            Some(self.dir.clone()),
        )
    }

    fn seed(&self, channel: WebhookChannel, url: &str) {
        self.secrets.map.lock().unwrap().insert(channel, url.into());
    }

    fn posts(&self) -> Vec<(String, String)> {
        self.transport.posts.lock().unwrap().clone()
    }

    fn shown(&self) -> usize {
        self.effects.shown.lock().unwrap().len()
    }

    fn world(&self) -> NotificationDispatch {
        NotificationDispatch::new(NotificationSource::WorldState, "Baro", "arrived")
    }

    /// World state with a Discord webhook on and the toast on.
    fn webhook_for_world(&self) {
        self.seed(WebhookChannel::Discord, DISCORD_URL);
        self.n.set_source(
            NotificationSource::WorldState,
            SourceToggles {
                native: true,
                webhook: true,
            },
        );
    }
}

async fn until(what: &str, cond: impl Fn() -> bool) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn delivers_natively_and_leaves_webhooks_alone_by_default() {
    let rig = Rig::new();
    rig.seed(WebhookChannel::Discord, DISCORD_URL);
    rig.n.notify(rig.world());
    assert_eq!(rig.shown(), 1);
    assert_eq!(
        rig.effects.recorded.lock().unwrap()[0],
        (NotificationKind::World, "Baro".into())
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(rig.posts().is_empty());
}

#[tokio::test]
async fn honours_the_per_source_toggles() {
    let rig = Rig::new();
    rig.seed(WebhookChannel::Discord, DISCORD_URL);
    rig.n.set_source(
        NotificationSource::WorldState,
        SourceToggles {
            native: false,
            webhook: true,
        },
    );
    rig.n.notify(rig.world());
    rig.n.notify(NotificationDispatch::new(
        NotificationSource::Whisper,
        "Message",
        "from Tenno",
    ));
    assert_eq!(rig.effects.shown.lock().unwrap()[0].0, "Message");
    assert_eq!(rig.shown(), 1);
    until("the post", || rig.posts().len() == 1).await;
    assert_eq!(
        rig.posts()[0].1,
        r#"{"content":"**Baro**\narrived","allowed_mentions":{"parse":[]}}"#
    );
}

#[tokio::test]
async fn the_producers_own_switch_suppresses_only_the_toast() {
    let rig = Rig::new();
    rig.webhook_for_world();
    let mut d = rig.world();
    d.native_enabled = false;
    rig.n.notify(d);
    assert_eq!(rig.shown(), 0);
    until("the post", || rig.posts().len() == 1).await;
}

#[tokio::test]
async fn posts_the_ping_to_discord_only() {
    let rig = Rig::new();
    rig.seed(WebhookChannel::Discord, DISCORD_URL);
    rig.seed(WebhookChannel::Generic, GENERIC_URL);
    rig.n.set_discord_ping(PING).unwrap();
    rig.n.set_source(
        NotificationSource::WorldState,
        SourceToggles {
            native: true,
            webhook: true,
        },
    );
    rig.n.notify(rig.world());
    until("both posts", || rig.posts().len() == 2).await;
    let posts = rig.posts();
    let discord = posts.iter().find(|(u, _)| u == DISCORD_URL).unwrap();
    let generic = posts.iter().find(|(u, _)| u == GENERIC_URL).unwrap();
    assert!(discord.1.contains(&format!("<@{PING}>")));
    assert!(!generic.1.contains(PING));
}

#[tokio::test]
async fn a_dead_webhook_never_delays_the_toast() {
    let rig = Rig::with(ScriptTransport {
        hang: true,
        ..Default::default()
    });
    rig.webhook_for_world();
    rig.n.notify(rig.world());
    // Asserted before any await: the toast must not wait on the request.
    assert_eq!(rig.shown(), 1);
}

#[tokio::test]
async fn a_failing_webhook_leaves_the_toast_and_the_service_alone() {
    let rig = Rig::new();
    rig.transport
        .script
        .lock()
        .unwrap()
        .push_back(SendOutcome::failed("socket hang up"));
    rig.webhook_for_world();
    rig.n.notify(rig.world());
    assert_eq!(rig.shown(), 1);
    until("the failed post to leave the queue", || {
        rig.posts().len() == 1 && rig.n.queue_len(WebhookChannel::Discord) == 0
    })
    .await;
    rig.n.notify(rig.world());
    until("the next post", || rig.posts().len() == 2).await;
}

#[tokio::test]
async fn holds_the_toast_but_still_posts_while_the_game_is_closed() {
    let rig = Rig::new();
    rig.webhook_for_world();
    rig.n.set_game_gate(true);
    *rig.effects.running.lock().unwrap() = Some(false);
    rig.n.notify(rig.world());
    assert_eq!(rig.shown(), 0);
    assert!(rig.effects.recorded.lock().unwrap().is_empty());
    until("the post", || rig.posts().len() == 1).await;
}

#[tokio::test]
async fn delivers_while_the_game_runs_whenever_its_state_is_unknown_and_with_the_option_off() {
    let rig = Rig::new();
    rig.n.set_game_gate(true);
    *rig.effects.running.lock().unwrap() = Some(true);
    rig.n.notify(rig.world());
    *rig.effects.running.lock().unwrap() = None;
    rig.n.notify(rig.world());
    rig.n.set_game_gate(false);
    *rig.effects.running.lock().unwrap() = Some(false);
    rig.n.notify(rig.world());
    assert_eq!(rig.shown(), 3);
}

#[test]
fn logs_the_hold_once_per_state_change() {
    let mut gate = HoldGate::default();
    let logged = |gate: &mut HoldGate, running| gate.check(true, running).1;
    assert_eq!(
        [Some(false), Some(false), Some(false)].map(|r| logged(&mut gate, r)),
        [true, false, false]
    );
    assert_eq!(gate.check(true, Some(true)), (false, false));
    assert_eq!(gate.check(true, Some(false)), (true, true));
    assert_eq!(gate.check(false, Some(false)), (false, false));
}

#[tokio::test]
async fn waits_out_retry_after_and_then_delivers() {
    let rig = Rig::new();
    rig.transport.script.lock().unwrap().push_back(SendOutcome {
        ok: false,
        rate_limited: true,
        retry_after: Duration::from_millis(20),
        error: "429".into(),
    });
    rig.webhook_for_world();
    rig.n.notify(rig.world());
    until("the retry", || rig.posts().len() == 2).await;
    until("an empty queue", || {
        rig.n.queue_len(WebhookChannel::Discord) == 0
    })
    .await;
}

#[tokio::test]
async fn drops_a_notification_once_the_429_attempts_run_out() {
    let rig = Rig::new();
    for _ in 0..5 {
        rig.transport.script.lock().unwrap().push_back(SendOutcome {
            ok: false,
            rate_limited: true,
            retry_after: Duration::ZERO,
            error: "429".into(),
        });
    }
    rig.webhook_for_world();
    rig.n.notify(rig.world());
    until("an empty queue", || {
        rig.posts().len() == 3 && rig.n.queue_len(WebhookChannel::Discord) == 0
    })
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(rig.posts().len(), 3);
}

#[tokio::test]
async fn keeps_the_queue_bounded_and_drops_the_oldest_first() {
    let rig = Rig::with(ScriptTransport {
        hang: true,
        ..Default::default()
    });
    rig.webhook_for_world();
    for i in 0..25 {
        rig.n.notify(NotificationDispatch::new(
            NotificationSource::WorldState,
            format!("n{i}"),
            "",
        ));
    }
    until("the first post", || rig.posts().len() == 1).await;
    assert_eq!(rig.n.queue_len(WebhookChannel::Discord), 20);
}

#[tokio::test]
async fn rejects_an_unusable_url_and_leaves_the_config_untouched() {
    let rig = Rig::new();
    for (url, error) in [
        ("http://127.0.0.1/hook", WebhookUrlError::NotHttps),
        ("https://127.0.0.1/hook", WebhookUrlError::BlockedHost),
        ("https://192.168.1.2/hook", WebhookUrlError::BlockedHost),
        ("", WebhookUrlError::Empty),
    ] {
        assert_eq!(
            rig.n
                .set_webhook(WebhookChannel::Discord, url)
                .await
                .unwrap_err(),
            error
        );
    }
    assert!(!rig.n.state().webhooks[&WebhookChannel::Discord].configured);
    assert!(rig.secrets.map.lock().unwrap().is_empty());
    assert!(!rig.dir.join(CONFIG_FILE).exists());
}

#[tokio::test]
async fn masks_the_saved_url_and_keeps_it_out_of_the_config_file() {
    let rig = Rig::new();
    let state = rig
        .n
        .set_webhook(WebhookChannel::Discord, DISCORD_URL)
        .await
        .unwrap();
    assert_eq!(
        state.webhooks[&WebhookChannel::Discord],
        WebhookStatus {
            configured: true,
            masked: "https://discord.com/...efgh".into()
        }
    );
    assert!(
        !serde_json::to_string(&state)
            .unwrap()
            .contains("webhooks/1234")
    );
    // Only toggles reach disk; the URL went to the keyring.
    rig.n.set_game_gate(true);
    let on_disk = std::fs::read_to_string(rig.dir.join(CONFIG_FILE)).unwrap();
    assert!(!on_disk.contains("webhooks/1234") && !on_disk.contains("discord.com"));
    assert_eq!(
        rig.secrets.map.lock().unwrap()[&WebhookChannel::Discord],
        DISCORD_URL
    );
}

#[tokio::test]
async fn round_trips_the_url_and_the_settings_across_a_reload() {
    let rig = Rig::new();
    rig.n
        .set_webhook(WebhookChannel::Generic, GENERIC_URL)
        .await
        .unwrap();
    rig.n.set_source(
        NotificationSource::Whisper,
        SourceToggles {
            native: false,
            webhook: true,
        },
    );
    rig.n.set_discord_ping(PING).unwrap();
    rig.n.set_game_gate(true);
    rig.n.set_sound_prefs(false, 0.4);

    let state = rig.reload().n.state();
    assert_eq!(
        state.webhooks[&WebhookChannel::Generic].masked,
        "https://hooks.example.com/...wxyz"
    );
    assert!(!state.webhooks[&WebhookChannel::Discord].configured);
    assert_eq!(
        state.sources[&NotificationSource::Whisper],
        SourceToggles {
            native: false,
            webhook: true
        }
    );
    assert_eq!(
        state.sources[&NotificationSource::WorldState],
        SourceToggles {
            native: true,
            webhook: false
        }
    );
    assert_eq!(state.discord_ping_user_id, PING);
    assert!(state.native_only_while_game_running);
    assert!(!state.sound_enabled);
    assert_eq!(state.sound_volume, 0.4);
}

#[tokio::test]
async fn clearing_a_webhook_stops_routing_and_forgets_the_secret() {
    let rig = Rig::new();
    rig.webhook_for_world();
    let state = rig.n.clear_webhook(WebhookChannel::Discord);
    assert_eq!(
        state.webhooks[&WebhookChannel::Discord],
        WebhookStatus {
            configured: false,
            masked: String::new()
        }
    );
    rig.n.notify(rig.world());
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(rig.posts().is_empty());
    assert!(!rig.reload().n.state().webhooks[&WebhookChannel::Discord].configured);
}

#[tokio::test]
async fn ignores_a_stored_url_that_is_not_https() {
    let rig = Rig::new();
    rig.seed(
        WebhookChannel::Discord,
        "http://discord.com/api/webhooks/1/abcd",
    );
    assert!(!rig.n.state().webhooks[&WebhookChannel::Discord].configured);
}

// Writing the OS keyring can prompt (Linux), so settings that touch no webhook never do.
#[test]
fn saving_toggles_never_writes_the_keyring() {
    let rig = Rig::new();
    rig.n.set_source(
        NotificationSource::Whisper,
        SourceToggles {
            native: false,
            webhook: true,
        },
    );
    rig.n.set_game_gate(true);
    rig.n.set_discord_ping(PING).unwrap();
    rig.n.set_sound_prefs(true, 1.0);
    assert_eq!(*rig.secrets.writes.lock().unwrap(), 0);
}

#[test]
fn only_a_discord_user_id_is_accepted_as_the_ping_target() {
    let rig = Rig::new();
    for raw in [
        "1234567890123456",
        "12345678901234567a",
        "<@123456789012345678>",
        "@everyone",
    ] {
        assert_eq!(
            rig.n.set_discord_ping(raw).unwrap_err(),
            DiscordPingError::InvalidUserId
        );
    }
    assert!(!rig.dir.join(CONFIG_FILE).exists());
    assert_eq!(
        rig.n
            .set_discord_ping(" 12345678901234567 ")
            .unwrap()
            .discord_ping_user_id,
        "12345678901234567"
    );
    assert_eq!(
        rig.n.set_discord_ping("  ").unwrap().discord_ping_user_id,
        ""
    );
}

#[tokio::test]
async fn re_checks_the_stored_url_so_a_hand_edited_private_host_never_posts() {
    let rig = Rig::new();
    rig.seed(WebhookChannel::Discord, "https://127.0.0.1/hook");
    assert_eq!(
        rig.n.test_webhook(WebhookChannel::Discord).await,
        Err(WebhookTestError::BlockedUrl)
    );
    assert!(rig.posts().is_empty());
}

#[tokio::test]
async fn reports_an_unconfigured_channel_instead_of_firing_a_test() {
    let rig = Rig::new();
    assert_eq!(
        rig.n.test_webhook(WebhookChannel::Generic).await,
        Err(WebhookTestError::NotConfigured)
    );
    assert!(rig.posts().is_empty());
}

#[tokio::test]
async fn a_failed_test_is_reported_coarsely() {
    let rig = Rig::new();
    rig.seed(WebhookChannel::Discord, DISCORD_URL);
    rig.transport
        .script
        .lock()
        .unwrap()
        .push_back(SendOutcome::failed("http 500"));
    assert_eq!(
        rig.n.test_webhook(WebhookChannel::Discord).await,
        Err(WebhookTestError::Failed)
    );
}

/// Sends every post to a local plain-HTTP server instead, keeping the real reqwest transport.
struct ToLocal(String, ReqwestTransport);

impl Transport for ToLocal {
    fn post(&self, _: String, body: String) -> BoxFuture<'_, SendOutcome> {
        self.1.post(self.0.clone(), body)
    }
}

#[tokio::test]
async fn the_test_fire_reaches_a_local_server_with_the_discord_payload() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local = format!("http://{}/hook", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
            .unwrap();
        String::from_utf8_lossy(&buf[..n]).into_owned()
    });
    let dir = std::env::temp_dir().join(format!("cf-notify-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let secrets = Arc::new(MemSecrets::default());
    secrets
        .map
        .lock()
        .unwrap()
        .insert(WebhookChannel::Discord, DISCORD_URL.into());
    let n = Notifications::new(
        dir.clone(),
        secrets,
        Arc::new(ToLocal(local, ReqwestTransport::new().unwrap())),
        Arc::new(FakeResolver::public()),
        Arc::new(FakeEffects::default()),
        Arc::new(SoundStore::new(dir.join(SOUND_FILE))),
    );
    assert_eq!(n.test_webhook(WebhookChannel::Discord).await, Ok(()));
    let request = server.join().unwrap();
    assert!(request.starts_with("POST /hook"));
    assert!(request.contains("CompanionFrame test notification"));
    assert!(request.contains(r#""allowed_mentions":{"parse":[]}"#));
}

#[tokio::test]
async fn plays_the_sound_with_the_toast_only_when_enabled_and_audible() {
    let rig = Rig::new();
    rig.n.notify(rig.world());
    rig.n.set_sound_prefs(true, 0.5);
    rig.n.notify(rig.world());
    rig.n.set_sound_prefs(true, 0.0);
    rig.n.notify(rig.world());
    rig.n.set_sound_prefs(false, 1.0);
    rig.n.notify(rig.world());
    assert_eq!(*rig.effects.sounds.lock().unwrap(), [1.0, 0.5]);
    assert_eq!(rig.n.set_sound_prefs(true, 9.0).sound_volume, 1.0);
}

#[derive(Default)]
struct Recorder(Mutex<Vec<NotificationDispatch>>);

impl Notifier for Recorder {
    fn notify(&self, d: NotificationDispatch) {
        self.0.lock().unwrap().push(d);
    }
}

#[test]
fn a_whisper_reaches_the_notifier_with_the_legacy_switch_as_the_toast_gate() {
    let recorder = Recorder::default();
    let opts = |native_enabled| MessageOptions {
        focused: Some(false),
        while_focused: false,
        native_enabled,
    };
    notify_in_game_message(&recorder, "Tenno", &opts(false));
    notify_in_game_message(&recorder, "Tenno", &opts(true));
    let sent = recorder.0.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert_eq!(
        (sent[0].source, sent[0].kind),
        (NotificationSource::Whisper, NotificationKind::Message)
    );
    assert_eq!(sent[0].body, "from Tenno");
    assert_eq!(
        (sent[0].native_enabled, sent[1].native_enabled),
        (false, true)
    );
}

#[test]
fn a_whisper_tab_opened_while_the_game_is_focused_is_skipped_unless_opted_in() {
    let recorder = Recorder::default();
    let focused = |while_focused| MessageOptions {
        focused: Some(true),
        while_focused,
        native_enabled: true,
    };
    notify_in_game_message(&recorder, "Tenno", &focused(false));
    assert!(recorder.0.lock().unwrap().is_empty());
    notify_in_game_message(&recorder, "Tenno", &focused(true));
    notify_in_game_message(
        &recorder,
        "Tenno",
        &MessageOptions {
            focused: None,
            while_focused: false,
            native_enabled: true,
        },
    );
    assert_eq!(recorder.0.lock().unwrap().len(), 2);
}

#[test]
fn the_resolver_seam_is_object_safe() {
    let _: Arc<dyn Resolver> = Arc::new(FakeResolver::with(Some(vec![
        "1.1.1.1".parse::<IpAddr>().unwrap(),
    ])));
}
