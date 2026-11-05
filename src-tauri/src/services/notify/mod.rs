//! Notification routing: desktop toast, sound, webhooks (Discord / generic) and the history log.
//!
//! Producers call [`Notifier::notify`]. [`Notifications`] looks up the source's route, shows the
//! native toast (unless held by the "only while Warframe runs" gate), records it in the history
//! and queues one webhook post per configured channel. Webhook URLs are secrets: they live in
//! the OS keyring, are re-validated before every send and only ever appear masked.

pub mod channels;
pub mod log;
pub mod sound;
pub mod webhook;

use std::collections::{BTreeMap, VecDeque};
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use self::channels::{
    ChannelConfig, ChannelState, DiscordPingError, NotificationKind, NotificationSource, Resolver,
    SourceToggles, SystemResolver, WebhookChannel, WebhookStatus, WebhookTestError,
    WebhookUrlError, mask_webhook_url, normalize_volume, parse_discord_user_id,
    validate_webhook_url,
};
use self::log::NotificationEntry;
use self::sound::{Player, SoundAsset, SoundStore, SoundUpload};
use self::webhook::{ReqwestTransport, SendOutcome, Transport, build_body};
use crate::db::Db;
use crate::services::atomic::write_atomic;
use crate::services::world_state::WorldState;

const MAX_QUEUE_ENTRIES: usize = 20;
const MAX_SEND_ATTEMPTS: u32 = 3;
const KEYRING_SERVICE: &str = "com.companionframe.app";
const CONFIG_FILE: &str = "notification-channels.json";
const SOUND_FILE: &str = "notification-sound.json";

#[derive(Clone, Debug)]
pub struct NotificationDispatch {
    pub source: NotificationSource,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    /// Extra fields for the generic webhook payload; never sent to Discord.
    pub meta: Option<Map<String, Value>>,
    /// The producer's own opt-out for the desktop toast (e.g. a legacy per-feature switch).
    pub native_enabled: bool,
}

impl NotificationDispatch {
    pub fn new(
        source: NotificationSource,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        let kind = match source {
            NotificationSource::WorldState => NotificationKind::World,
            NotificationSource::Whisper => NotificationKind::Message,
            NotificationSource::TradeToast => NotificationKind::Trade,
            _ => NotificationKind::App,
        };
        Self {
            source,
            kind,
            title: title.into(),
            body: body.into(),
            meta: None,
            native_enabled: true,
        }
    }
}

/// What producers (world state, market alerts, trackers) talk to.
pub trait Notifier: Send + Sync {
    fn notify(&self, dispatch: NotificationDispatch);

    /// Called by the world-state service with every refreshed snapshot. The default ignores it;
    /// the world-state notifier overrides it with the prev/next comparison.
    fn world_state_changed(&self, _prev: Option<&WorldState>, _next: &WorldState) {}
}

pub struct NoNotifier;

impl Notifier for NoNotifier {
    fn notify(&self, _: NotificationDispatch) {}
}

/// Side effects that need the running app; faked in tests.
pub trait Effects: Send + Sync {
    fn show(&self, title: &str, body: &str);
    /// `None` when unknown (not Windows, or the check failed).
    fn game_running(&self) -> Option<bool>;
    fn record(&self, kind: NotificationKind, title: &str, body: &str);
    fn play_sound(&self, volume: f32);
}

/// Webhook URL storage, one secret per channel.
pub trait WebhookSecrets: Send + Sync {
    fn get(&self, channel: WebhookChannel) -> Option<String>;
    fn set(&self, channel: WebhookChannel, url: &str) -> Result<(), String>;
    fn delete(&self, channel: WebhookChannel);
}

pub struct KeyringSecrets;

impl KeyringSecrets {
    fn entry(channel: WebhookChannel) -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new(
            KEYRING_SERVICE,
            &format!("notif-webhook-{}", channel.as_str()),
        )
    }
}

impl WebhookSecrets for KeyringSecrets {
    fn get(&self, channel: WebhookChannel) -> Option<String> {
        Self::entry(channel).ok()?.get_password().ok()
    }
    fn set(&self, channel: WebhookChannel, url: &str) -> Result<(), String> {
        Self::entry(channel)
            .and_then(|e| e.set_password(url))
            .map_err(|e| e.to_string())
    }
    fn delete(&self, channel: WebhookChannel) {
        if let Ok(e) = Self::entry(channel) {
            let _ = e.delete_credential();
        }
    }
}

/// Logs the "holding desktop notifications" line once per state change, not per notification.
#[derive(Default)]
struct HoldGate {
    logged: bool,
}

impl HoldGate {
    /// `(hold, log_now)`. An unknown game state delivers: a failed reading must not silence
    /// notifications.
    fn check(&mut self, enabled: bool, running: Option<bool>) -> (bool, bool) {
        if !enabled || running != Some(false) {
            self.logged = false;
            return (false, false);
        }
        let first = !self.logged;
        self.logged = true;
        (true, first)
    }
}

struct Queued {
    id: u64,
    body: String,
    attempts: u32,
}

#[derive(Default)]
struct Queue {
    items: VecDeque<Queued>,
    draining: bool,
    blocked_until: Option<Instant>,
}

struct Inner {
    path: PathBuf,
    config: Mutex<ChannelConfig>,
    /// Loaded from the keyring on first use, so merely saving toggles never asks it anything.
    webhooks: Mutex<Option<BTreeMap<WebhookChannel, String>>>,
    secrets: Arc<dyn WebhookSecrets>,
    transport: Arc<dyn Transport>,
    resolver: Arc<dyn Resolver>,
    effects: Arc<dyn Effects>,
    sound: Arc<SoundStore>,
    queues: Mutex<BTreeMap<WebhookChannel, Queue>>,
    hold: Mutex<HoldGate>,
    next_id: AtomicU64,
}

/// Managed state and the production [`Notifier`].
#[derive(Clone)]
pub struct Notifications {
    inner: Arc<Inner>,
}

impl Notifications {
    pub fn new(
        dir: PathBuf,
        secrets: Arc<dyn WebhookSecrets>,
        transport: Arc<dyn Transport>,
        resolver: Arc<dyn Resolver>,
        effects: Arc<dyn Effects>,
        sound: Arc<SoundStore>,
    ) -> Self {
        let path = dir.join(CONFIG_FILE);
        let config = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| ChannelConfig::revive(&v))
            .unwrap_or_default();
        Self {
            inner: Arc::new(Inner {
                path,
                config: Mutex::new(config),
                webhooks: Mutex::new(None),
                secrets,
                transport,
                resolver,
                effects,
                sound,
                queues: Mutex::new(WebhookChannel::ALL.map(|c| (c, Queue::default())).into()),
                hold: Mutex::new(HoldGate::default()),
                next_id: AtomicU64::new(0),
            }),
        }
    }

    pub fn state(&self) -> ChannelState {
        self.inner.state()
    }

    pub async fn set_webhook(
        &self,
        channel: WebhookChannel,
        url: &str,
    ) -> Result<ChannelState, WebhookUrlError> {
        let url = validate_webhook_url(url, &*self.inner.resolver).await?;
        self.inner.with_webhooks(|w| w.insert(channel, url.clone()));
        if self.inner.secrets.set(channel, &url).is_err() {
            // Like an unavailable OS store: the live process keeps routing, a restart loses it.
            tracing::warn!(
                "keyring unavailable - {} webhook not persisted",
                channel.as_str()
            );
        }
        tracing::info!(
            "{} webhook set to {}",
            channel.as_str(),
            mask_webhook_url(&url)
        );
        Ok(self.state())
    }

    pub fn clear_webhook(&self, channel: WebhookChannel) -> ChannelState {
        self.inner.with_webhooks(|w| w.remove(&channel));
        self.inner.secrets.delete(channel);
        if let Some(q) = self.inner.queues.lock().unwrap().get_mut(&channel) {
            q.items.clear();
            q.blocked_until = None;
        }
        self.state()
    }

    pub fn set_source(&self, source: NotificationSource, toggles: SourceToggles) -> ChannelState {
        self.inner.update_config(|c| {
            c.sources.insert(source, toggles);
        });
        self.state()
    }

    pub fn set_game_gate(&self, enabled: bool) -> ChannelState {
        self.inner
            .update_config(|c| c.native_only_while_game_running = enabled);
        self.inner.hold.lock().unwrap().logged = false;
        self.state()
    }

    pub fn set_discord_ping(&self, raw: &str) -> Result<ChannelState, DiscordPingError> {
        let id = parse_discord_user_id(raw).ok_or(DiscordPingError::InvalidUserId)?;
        tracing::info!(
            "discord ping {}",
            if id.is_empty() { "cleared" } else { "set" }
        );
        self.inner.update_config(|c| c.discord_ping_user_id = id);
        Ok(self.state())
    }

    pub fn set_sound_prefs(&self, enabled: bool, volume: f64) -> ChannelState {
        self.inner.update_config(|c| {
            c.sound_enabled = enabled;
            c.sound_volume = normalize_volume(volume);
        });
        self.state()
    }

    pub fn sound(&self) -> &SoundStore {
        &self.inner.sound
    }

    /// Plays the current clip once at the saved volume, for the settings preview.
    pub fn preview_sound(&self) {
        let volume = self.inner.config.lock().unwrap().sound_volume;
        self.inner.effects.play_sound(volume as f32);
    }

    /// Awaited rather than queued: the settings button is useless without a result.
    pub async fn test_webhook(&self, channel: WebhookChannel) -> Result<(), WebhookTestError> {
        if self.inner.url_of(channel).is_none() {
            return Err(WebhookTestError::NotConfigured);
        }
        let payload = NotificationDispatch::new(
            NotificationSource::Test,
            "CompanionFrame test notification",
            "Notification channels are wired up.",
        );
        let body = self.inner.body_for(channel, &payload);
        let outcome = self.inner.send_webhook(channel, body).await;
        if outcome.ok {
            return Ok(());
        }
        if outcome.error.starts_with("blocked url") {
            return Err(WebhookTestError::BlockedUrl);
        }
        tracing::warn!(
            "{} test failed: {}",
            self.inner.describe(channel),
            outcome.error
        );
        Err(WebhookTestError::Failed)
    }

    #[cfg(test)]
    fn queue_len(&self, channel: WebhookChannel) -> usize {
        self.inner.queues.lock().unwrap()[&channel].items.len()
    }
}

impl Notifier for Notifications {
    /// Routes one notification. The desktop path runs synchronously, so a dead or slow webhook
    /// cannot delay it. Safe to call from any thread, inside or outside a runtime.
    fn notify(&self, d: NotificationDispatch) {
        let inner = &self.inner;
        let (routes, gate_on) = {
            let c = inner.config.lock().unwrap();
            let routes = c
                .sources
                .get(&d.source)
                .copied()
                .unwrap_or_else(|| d.source.default_toggles());
            (routes, c.native_only_while_game_running)
        };

        if routes.native && d.native_enabled && !inner.hold_native(gate_on) {
            inner.deliver_native(&d);
        }

        if !routes.webhook {
            return;
        }
        for channel in WebhookChannel::ALL {
            if inner.url_of(channel).is_some() {
                inner.enqueue(channel, inner.body_for(channel, &d));
            }
        }
    }
}

impl Inner {
    fn state(&self) -> ChannelState {
        let c = self.config.lock().unwrap().clone();
        let webhooks = WebhookChannel::ALL
            .into_iter()
            .map(|ch| {
                let status = match self.url_of(ch) {
                    Some(url) => WebhookStatus {
                        configured: true,
                        masked: mask_webhook_url(&url),
                    },
                    None => WebhookStatus {
                        configured: false,
                        masked: String::new(),
                    },
                };
                (ch, status)
            })
            .collect();
        ChannelState {
            webhooks,
            sources: c.sources,
            native_only_while_game_running: c.native_only_while_game_running,
            discord_ping_user_id: c.discord_ping_user_id,
            sound_enabled: c.sound_enabled,
            sound_volume: c.sound_volume,
        }
    }

    fn update_config(&self, f: impl FnOnce(&mut ChannelConfig)) {
        let mut c = self.config.lock().unwrap();
        f(&mut c);
        let saved = serde_json::to_vec_pretty(&*c)
            .map_err(|e| e.to_string())
            .and_then(|b| write_atomic(&self.path, &b).map_err(|e| e.to_string()));
        if let Err(e) = saved {
            tracing::warn!("notification settings not saved: {e}");
        }
    }

    fn with_webhooks<R>(&self, f: impl FnOnce(&mut BTreeMap<WebhookChannel, String>) -> R) -> R {
        let mut guard = self.webhooks.lock().unwrap();
        let map = guard.get_or_insert_with(|| {
            WebhookChannel::ALL
                .into_iter()
                .filter_map(|ch| {
                    let url = self.secrets.get(ch)?;
                    // Shape only; the destination is re-validated before every send.
                    url.starts_with("https://").then_some((ch, url))
                })
                .collect()
        });
        f(map)
    }

    fn url_of(&self, channel: WebhookChannel) -> Option<String> {
        self.with_webhooks(|w| w.get(&channel).cloned())
    }

    fn describe(&self, channel: WebhookChannel) -> String {
        self.url_of(channel)
            .map(|u| mask_webhook_url(&u))
            .unwrap_or_else(|| channel.as_str().to_string())
    }

    fn body_for(&self, channel: WebhookChannel, d: &NotificationDispatch) -> String {
        let at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let ping = self.config.lock().unwrap().discord_ping_user_id.clone();
        build_body(channel, d, &at, &ping)
    }

    fn hold_native(&self, gate_on: bool) -> bool {
        let running = if gate_on {
            self.effects.game_running()
        } else {
            None
        };
        let (hold, log_now) = self.hold.lock().unwrap().check(gate_on, running);
        if log_now {
            tracing::info!("Warframe is not running - holding desktop notifications");
        }
        hold
    }

    fn deliver_native(&self, d: &NotificationDispatch) {
        self.effects.record(d.kind, &d.title, &d.body);
        self.effects.show(&d.title, &d.body);
        let (enabled, volume) = {
            let c = self.config.lock().unwrap();
            (c.sound_enabled, c.sound_volume)
        };
        if enabled && volume > 0.0 {
            self.effects.play_sound(volume as f32);
        }
    }

    async fn send_webhook(&self, channel: WebhookChannel, body: String) -> SendOutcome {
        let Some(url) = self.url_of(channel) else {
            return SendOutcome::failed("not configured");
        };
        match validate_webhook_url(&url, &*self.resolver).await {
            Ok(url) => self.transport.post(url, body).await,
            Err(e) => SendOutcome::failed(format!("blocked url ({e:?})")),
        }
    }

    fn enqueue(self: &Arc<Self>, channel: WebhookChannel, body: String) {
        let spawn = {
            let mut queues = self.queues.lock().unwrap();
            let q = queues.get_mut(&channel).unwrap();
            q.items.push_back(Queued {
                id: self.next_id.fetch_add(1, Ordering::Relaxed),
                body,
                attempts: 0,
            });
            while q.items.len() > MAX_QUEUE_ENTRIES {
                q.items.pop_front();
                tracing::warn!(
                    "{} queue is full - dropped the oldest notification",
                    channel.as_str()
                );
            }
            !std::mem::replace(&mut q.draining, true)
        };
        if spawn {
            // Not tokio::spawn: producers run from Tauri setup and sync commands, outside a runtime.
            tauri::async_runtime::spawn(self.clone().drain(channel));
        }
    }

    async fn drain(self: Arc<Self>, channel: WebhookChannel) {
        loop {
            let wait = {
                let queues = self.queues.lock().unwrap();
                queues[&channel]
                    .blocked_until
                    .map(|t| t.saturating_duration_since(Instant::now()))
            };
            if let Some(wait) = wait.filter(|w| !w.is_zero()) {
                tokio::time::sleep(wait).await;
            }
            let (id, attempts, body) = {
                let mut queues = self.queues.lock().unwrap();
                let q = queues.get_mut(&channel).unwrap();
                match q.items.front() {
                    Some(i) => (i.id, i.attempts, i.body.clone()),
                    None => {
                        q.draining = false;
                        return;
                    }
                }
            };

            let outcome = self.send_webhook(channel, body).await;

            let mut queues = self.queues.lock().unwrap();
            let q = queues.get_mut(&channel).unwrap();
            // The head can be dropped by an overflow, or cleared, while the request is in flight.
            let head_is_ours = q.items.front().map(|i| i.id) == Some(id);
            if outcome.rate_limited {
                let attempts = attempts + 1;
                q.blocked_until = Some(Instant::now() + outcome.retry_after);
                if head_is_ours && attempts >= MAX_SEND_ATTEMPTS {
                    q.items.pop_front();
                } else if head_is_ours {
                    q.items.front_mut().unwrap().attempts = attempts;
                }
                if attempts >= MAX_SEND_ATTEMPTS {
                    tracing::warn!(
                        "{} dropped a notification after {attempts} 429s",
                        channel.as_str()
                    );
                }
                continue;
            }
            if head_is_ours {
                q.items.pop_front();
            }
            drop(queues);
            if !outcome.ok {
                tracing::warn!("{} post failed: {}", self.describe(channel), outcome.error);
            }
        }
    }
}

/// Whisper tabs: EE.log cannot tell sent from received, so a tab opened while the game is
/// focused counts as outgoing unless the user opted in.
pub struct MessageOptions {
    pub focused: Option<bool>,
    pub while_focused: bool,
    /// The legacy per-feature switch for the desktop toast.
    pub native_enabled: bool,
}

pub fn notify_in_game_message(notifier: &dyn Notifier, player: &str, opts: &MessageOptions) {
    if !opts.while_focused && opts.focused == Some(true) {
        tracing::info!("Warframe focused - skipping message notification");
        return;
    }
    let mut d = NotificationDispatch::new(
        NotificationSource::Whisper,
        "New in-game conversation",
        format!("from {player}"),
    );
    d.native_enabled = opts.native_enabled;
    notifier.notify(d);
}

struct AppEffects {
    app: AppHandle,
    sound: Arc<SoundStore>,
    player: Player,
}

impl Effects for AppEffects {
    fn show(&self, title: &str, body: &str) {
        if let Err(e) = self
            .app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            tracing::warn!("native notification failed: {e}");
        }
    }

    fn game_running(&self) -> Option<bool> {
        #[cfg(windows)]
        return Some(crate::platform::windows::process::warframe_running());
        #[cfg(not(windows))]
        None
    }

    fn record(&self, kind: NotificationKind, title: &str, body: &str) {
        // A failed history write costs the entry only, never the notification.
        let db = self.app.state::<Db>();
        let entry = {
            let conn = db.lock();
            log::record(&conn, kind, title, body)
        };
        match entry {
            Ok(entry) => {
                let _ = self.app.emit("notification-added", entry);
            }
            Err(e) => tracing::warn!("notification history write failed: {e}"),
        }
    }

    fn play_sound(&self, volume: f32) {
        self.player.play(self.sound.bytes(), volume);
    }
}

/// Registers the managed [`Notifications`]. Must run after the database is managed and before
/// anything that takes a notifier (world state).
pub fn setup(app: &tauri::App) -> Result<(), Box<dyn Error>> {
    let data = app.path().app_data_dir()?;
    let sound = Arc::new(SoundStore::new(data.join(SOUND_FILE)));
    let effects = Arc::new(AppEffects {
        app: app.handle().clone(),
        sound: sound.clone(),
        player: Player::spawn(),
    });
    app.manage(Notifications::new(
        data,
        Arc::new(KeyringSecrets),
        Arc::new(ReqwestTransport::new()?),
        Arc::new(SystemResolver),
        effects,
        sound,
    ));
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn notif_get_state(n: State<'_, Notifications>) -> ChannelState {
    n.state()
}

#[tauri::command]
#[specta::specta]
pub async fn notif_set_webhook(
    n: State<'_, Notifications>,
    channel: WebhookChannel,
    url: String,
) -> Result<ChannelState, WebhookUrlError> {
    n.set_webhook(channel, &url).await
}

#[tauri::command]
#[specta::specta]
pub fn notif_clear_webhook(n: State<'_, Notifications>, channel: WebhookChannel) -> ChannelState {
    n.clear_webhook(channel)
}

#[tauri::command]
#[specta::specta]
pub async fn notif_test_webhook(
    n: State<'_, Notifications>,
    channel: WebhookChannel,
) -> Result<(), WebhookTestError> {
    n.test_webhook(channel).await
}

#[tauri::command]
#[specta::specta]
pub fn notif_set_source(
    n: State<'_, Notifications>,
    source: NotificationSource,
    toggles: SourceToggles,
) -> ChannelState {
    n.set_source(source, toggles)
}

#[tauri::command]
#[specta::specta]
pub fn notif_set_game_gate(n: State<'_, Notifications>, enabled: bool) -> ChannelState {
    n.set_game_gate(enabled)
}

#[tauri::command]
#[specta::specta]
pub fn notif_set_discord_ping(
    n: State<'_, Notifications>,
    user_id: String,
) -> Result<ChannelState, DiscordPingError> {
    n.set_discord_ping(&user_id)
}

#[tauri::command]
#[specta::specta]
pub fn notif_log_get(db: State<'_, Db>) -> Result<Vec<NotificationEntry>, String> {
    log::get_all(&db.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn notif_log_remove(db: State<'_, Db>, id: String) -> Result<bool, String> {
    log::remove(&db.lock(), &id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn notif_log_clear(db: State<'_, Db>) -> Result<(), String> {
    log::clear(&db.lock()).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn notif_sound_get(n: State<'_, Notifications>) -> Option<SoundAsset> {
    n.sound().get()
}

#[tauri::command]
#[specta::specta]
pub fn notif_sound_set(
    n: State<'_, Notifications>,
    upload: SoundUpload,
) -> Result<SoundAsset, String> {
    n.sound().save(&upload)
}

#[tauri::command]
#[specta::specta]
pub fn notif_sound_reset(n: State<'_, Notifications>) {
    n.sound().reset();
}

#[tauri::command]
#[specta::specta]
pub fn notif_sound_set_prefs(
    n: State<'_, Notifications>,
    enabled: bool,
    volume: f64,
) -> ChannelState {
    n.set_sound_prefs(enabled, volume)
}

#[tauri::command]
#[specta::specta]
pub fn notif_sound_preview(n: State<'_, Notifications>) {
    n.preview_sound();
}

#[cfg(test)]
mod tests;
