//! Channel model: sources, per-source routes, persisted settings and the webhook URL policy
//! (HTTPS only, no private or reserved destination, secrets masked in everything user-visible).

use std::collections::BTreeMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::pin::Pin;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{Host, Url};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

const DNS_TIMEOUT: Duration = Duration::from_secs(5);

/// Stored in the history table; the tag is the column value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    Trade,
    Message,
    World,
    App,
}

impl NotificationKind {
    pub const ALL: [Self; 4] = [Self::Trade, Self::Message, Self::World, Self::App];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::Message => "message",
            Self::World => "world",
            Self::App => "app",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// Producers the channel layer routes. `Test` labels a webhook test fire.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum NotificationSource {
    WorldState,
    ArbiSchedule,
    Whisper,
    TradeToast,
    MarketAlerts,
    Test,
    InventorySelections,
}

impl NotificationSource {
    pub const ALL: [Self; 7] = [
        Self::WorldState,
        Self::ArbiSchedule,
        Self::Whisper,
        Self::TradeToast,
        Self::MarketAlerts,
        Self::Test,
        Self::InventorySelections,
    ];

    fn key(self) -> &'static str {
        match self {
            Self::WorldState => "worldState",
            Self::ArbiSchedule => "arbiSchedule",
            Self::Whisper => "whisper",
            Self::TradeToast => "tradeToast",
            Self::MarketAlerts => "marketAlerts",
            Self::Test => "test",
            Self::InventorySelections => "inventorySelections",
        }
    }

    /// Webhooks are opt-in: a producer routes to one only after the user turns it on.
    pub fn default_toggles(self) -> SourceToggles {
        let test = self == Self::Test;
        SourceToggles {
            native: !test,
            webhook: test,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SourceToggles {
    pub native: bool,
    pub webhook: bool,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "lowercase")]
pub enum WebhookChannel {
    Discord,
    Generic,
}

impl WebhookChannel {
    pub const ALL: [Self; 2] = [Self::Discord, Self::Generic];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::Generic => "generic",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct WebhookStatus {
    pub configured: bool,
    /// Scheme, host and the last four characters; the URL itself never leaves the backend.
    pub masked: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChannelState {
    pub webhooks: BTreeMap<WebhookChannel, WebhookStatus>,
    pub sources: BTreeMap<NotificationSource, SourceToggles>,
    /// Opt-in gate on desktop delivery only; webhooks keep firing either way.
    pub native_only_while_game_running: bool,
    /// Discord user the Discord webhook pings; empty pings nobody.
    pub discord_ping_user_id: String,
    pub sound_enabled: bool,
    pub sound_volume: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum WebhookUrlError {
    Empty,
    InvalidUrl,
    NotHttps,
    BlockedHost,
    DnsFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum DiscordPingError {
    InvalidUserId,
}

/// Coarse on purpose: a raw transport error could carry the webhook URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum WebhookTestError {
    NotConfigured,
    BlockedUrl,
    Failed,
}

/// Everything persisted in `notification-channels.json`; the webhook URLs are not here (keyring).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub sources: BTreeMap<NotificationSource, SourceToggles>,
    pub native_only_while_game_running: bool,
    pub discord_ping_user_id: String,
    pub sound_enabled: bool,
    pub sound_volume: f64,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            sources: NotificationSource::ALL
                .into_iter()
                .map(|s| (s, s.default_toggles()))
                .collect(),
            native_only_while_game_running: false,
            discord_ping_user_id: String::new(),
            sound_enabled: true,
            sound_volume: 1.0,
        }
    }
}

impl ChannelConfig {
    /// Shape only, field by field: the file is user-writable, so a bad field falls back to its
    /// default instead of discarding the rest.
    pub fn revive(raw: &Value) -> Option<Self> {
        let raw = raw.as_object()?;
        let mut config = Self::default();
        if let Some(b) = raw
            .get("nativeOnlyWhileGameRunning")
            .and_then(Value::as_bool)
        {
            config.native_only_while_game_running = b;
        }
        if let Some(b) = raw.get("soundEnabled").and_then(Value::as_bool) {
            config.sound_enabled = b;
        }
        if let Some(v) = raw.get("soundVolume").and_then(Value::as_f64) {
            config.sound_volume = normalize_volume(v);
        }
        if let Some(id) = raw.get("discordPingUserId").and_then(Value::as_str) {
            config.discord_ping_user_id = parse_discord_user_id(id).unwrap_or_default();
        }
        if let Some(sources) = raw.get("sources").and_then(Value::as_object) {
            for source in NotificationSource::ALL {
                let Some(entry) = sources.get(source.key()).and_then(Value::as_object) else {
                    continue;
                };
                let default = source.default_toggles();
                config.sources.insert(
                    source,
                    SourceToggles {
                        native: entry
                            .get("native")
                            .and_then(Value::as_bool)
                            .unwrap_or(default.native),
                        webhook: entry
                            .get("webhook")
                            .and_then(Value::as_bool)
                            .unwrap_or(default.webhook),
                    },
                );
            }
        }
        Some(config)
    }
}

/// Out-of-range volumes clamp into 0..=1; a non-finite one resets to 1.
pub fn normalize_volume(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Empty clears the ping; `None` is anything that is not a Discord user ID.
pub fn parse_discord_user_id(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() {
        return Some(String::new());
    }
    let ok = (17..=20).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| text.to_string())
}

pub fn mask_webhook_url(raw: &str) -> String {
    let Ok(url) = Url::parse(raw) else {
        return String::new();
    };
    let Some(host) = url.host_str() else {
        return String::new();
    };
    let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
    let tail: Vec<char> = raw.chars().rev().take(4).collect();
    let tail: String = tail.into_iter().rev().collect();
    format!("{}://{host}{port}/...{tail}", url.scheme())
}

fn is_blocked_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 0 // 0.0.0.0/8, which covers the unspecified address
        || a == 10
        || a == 127
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
}

// ::a.b.c.d, ::ffff:a.b.c.d and the NAT64 well-known prefix carry a v4 address in the low 32
// bits, so the v4 rules decide them. :: and ::1 fall out of that as 0.0.0.0 and 0.0.0.1.
fn is_blocked_v6(ip: Ipv6Addr) -> bool {
    let g = ip.segments();
    let mapped = g[..5] == [0; 5] && (g[5] == 0xffff || g[5] == 0);
    let nat64 = g[0] == 0x64 && g[1] == 0xff9b && g[2..6] == [0; 4];
    if mapped || nat64 {
        let v4 = Ipv4Addr::new((g[6] >> 8) as u8, g[6] as u8, (g[7] >> 8) as u8, g[7] as u8);
        if is_blocked_v4(v4) {
            return true;
        }
    }
    (g[0] & 0xfe00) == 0xfc00 || (g[0] & 0xffc0) == 0xfe80
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_hostname(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    matches!(
        host.as_str(),
        "localhost" | "localhost.localdomain" | "ip6-localhost" | "ip6-loopback"
    ) || host.ends_with(".localhost")
}

/// DNS seam so tests never touch the network.
pub trait Resolver: Send + Sync {
    /// `None` when the lookup failed.
    fn lookup(&self, host: String) -> BoxFuture<'_, Option<Vec<IpAddr>>>;
}

pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn lookup(&self, host: String) -> BoxFuture<'_, Option<Vec<IpAddr>>> {
        Box::pin(async move {
            tauri::async_runtime::spawn_blocking(move || {
                (host.as_str(), 443)
                    .to_socket_addrs()
                    .ok()
                    .map(|a| a.map(|s| s.ip()).collect())
            })
            .await
            .ok()
            .flatten()
        })
    }
}

/// A black-holed resolver would otherwise hang the settings save and wedge the drain loop.
pub async fn lookup_within(
    resolver: &dyn Resolver,
    host: &str,
    limit: Duration,
) -> Option<Vec<IpAddr>> {
    tokio::time::timeout(limit, resolver.lookup(host.to_string()))
        .await
        .ok()
        .flatten()
}

/// HTTPS only, no private or reserved destination. The lookup still leaves a rebinding window
/// between here and connect(); a resolved public address is the strongest check available.
pub async fn validate_webhook_url(
    raw: &str,
    resolver: &dyn Resolver,
) -> Result<String, WebhookUrlError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(WebhookUrlError::Empty);
    }
    let url = Url::parse(text).map_err(|_| WebhookUrlError::InvalidUrl)?;
    if url.scheme() != "https" {
        return Err(WebhookUrlError::NotHttps);
    }
    match url.host() {
        None => Err(WebhookUrlError::BlockedHost),
        Some(Host::Ipv4(ip)) if is_blocked_v4(ip) => Err(WebhookUrlError::BlockedHost),
        Some(Host::Ipv6(ip)) if is_blocked_v6(ip) => Err(WebhookUrlError::BlockedHost),
        Some(Host::Ipv4(_) | Host::Ipv6(_)) => Ok(text.to_string()),
        Some(Host::Domain(domain)) => {
            if domain.is_empty() || is_blocked_hostname(domain) {
                return Err(WebhookUrlError::BlockedHost);
            }
            let resolved = lookup_within(resolver, domain, DNS_TIMEOUT)
                .await
                .filter(|r| !r.is_empty())
                .ok_or(WebhookUrlError::DnsFailed)?;
            if resolved.into_iter().any(is_blocked_ip) {
                return Err(WebhookUrlError::BlockedHost);
            }
            Ok(text.to_string())
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Mutex;

    /// Answers every lookup with a canned result and counts the calls.
    pub struct FakeResolver {
        pub answer: Mutex<Option<Vec<IpAddr>>>,
        pub calls: Mutex<usize>,
    }

    impl FakeResolver {
        pub fn public() -> Self {
            Self::with(Some(vec!["93.184.216.34".parse().unwrap()]))
        }
        pub fn with(answer: Option<Vec<IpAddr>>) -> Self {
            Self {
                answer: Mutex::new(answer),
                calls: Mutex::new(0),
            }
        }
    }

    impl Resolver for FakeResolver {
        fn lookup(&self, _: String) -> BoxFuture<'_, Option<Vec<IpAddr>>> {
            *self.calls.lock().unwrap() += 1;
            let answer = self.answer.lock().unwrap().clone();
            Box::pin(async move { answer })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::FakeResolver;
    use super::*;

    const DISCORD_URL: &str = "https://discord.com/api/webhooks/1234/abcdefgh";

    async fn check(url: &str) -> Result<String, WebhookUrlError> {
        validate_webhook_url(url, &FakeResolver::public()).await
    }

    #[tokio::test]
    async fn refuses_anything_that_is_not_an_https_url() {
        assert_eq!(check("").await, Err(WebhookUrlError::Empty));
        assert_eq!(check("   ").await, Err(WebhookUrlError::Empty));
        assert_eq!(check("not a url").await, Err(WebhookUrlError::InvalidUrl));
        assert_eq!(
            check("http://example.com/hook").await,
            Err(WebhookUrlError::NotHttps)
        );
        assert_eq!(
            check("file:///etc/passwd").await,
            Err(WebhookUrlError::NotHttps)
        );
    }

    #[tokio::test]
    async fn blocks_every_reserved_literal_address() {
        let resolver = FakeResolver::public();
        for url in [
            "https://127.0.0.1/hook",
            "https://127.13.9.4/hook",
            "https://10.0.0.1/hook",
            "https://10.255.255.255/hook",
            "https://172.16.0.1/hook",
            "https://172.31.255.254/hook",
            "https://192.168.1.10/hook",
            "https://169.254.169.254/hook",
            "https://0.0.0.0/hook",
            "https://[::1]/hook",
            "https://[::]/hook",
            "https://[fc00::1]/hook",
            "https://[fd12:3456::1]/hook",
            "https://[fe80::1]/hook",
            "https://[febf::1]/hook",
            "https://[::ffff:127.0.0.1]/hook",
            "https://[::ffff:192.168.0.5]/hook",
            "https://[::ffff:7f00:1]/hook",
            // NAT64 synthesis reaches the same v4 addresses on a DNS64 network.
            "https://[64:ff9b::7f00:1]/hook",
            "https://[64:ff9b::a00:1]/hook",
            // Alternate v4 spellings are normalised by the URL parser before the check.
            "https://0x7f.1/hook",
            "https://2130706433/hook",
        ] {
            assert_eq!(
                validate_webhook_url(url, &resolver).await,
                Err(WebhookUrlError::BlockedHost),
                "{url}"
            );
        }
        assert_eq!(*resolver.calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn blocks_localhost_names_without_asking_dns() {
        let resolver = FakeResolver::public();
        for url in [
            "https://localhost/hook",
            "https://LOCALHOST/hook",
            "https://localhost./hook",
            "https://api.localhost/hook",
            "https://ip6-localhost/hook",
        ] {
            assert_eq!(
                validate_webhook_url(url, &resolver).await,
                Err(WebhookUrlError::BlockedHost),
                "{url}"
            );
        }
        assert_eq!(*resolver.calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn accepts_a_public_literal_just_outside_a_blocked_range() {
        for url in ["https://172.32.0.1/hook", "https://[2001:db8::1]/hook"] {
            assert_eq!(check(url).await.as_deref(), Ok(url));
        }
    }

    #[tokio::test]
    async fn blocks_a_public_name_that_resolves_into_a_private_range() {
        let resolver = FakeResolver::with(Some(vec![
            "93.184.216.34".parse().unwrap(),
            "192.168.4.4".parse().unwrap(),
        ]));
        assert_eq!(
            validate_webhook_url("https://rebind.example.com/hook", &resolver).await,
            Err(WebhookUrlError::BlockedHost)
        );
    }

    #[tokio::test]
    async fn reports_a_failed_or_empty_lookup_instead_of_guessing() {
        for answer in [None, Some(vec![])] {
            assert_eq!(
                validate_webhook_url("https://nope.example.com/hook", &FakeResolver::with(answer))
                    .await,
                Err(WebhookUrlError::DnsFailed)
            );
        }
    }

    #[tokio::test]
    async fn gives_up_on_a_lookup_that_never_answers() {
        struct Hang;
        impl Resolver for Hang {
            fn lookup(&self, _: String) -> BoxFuture<'_, Option<Vec<IpAddr>>> {
                Box::pin(std::future::pending())
            }
        }
        assert_eq!(
            lookup_within(&Hang, "slow.example.com", Duration::from_millis(30)).await,
            None
        );
    }

    #[test]
    fn masks_scheme_host_and_last_four_characters() {
        assert_eq!(mask_webhook_url(DISCORD_URL), "https://discord.com/...efgh");
        assert_eq!(
            mask_webhook_url("https://example.com:8443/a/b/token1234"),
            "https://example.com:8443/...1234"
        );
        assert_eq!(mask_webhook_url("garbage"), "");
    }

    #[test]
    fn accepts_only_a_discord_user_id_as_ping_target() {
        for raw in [
            "1234567890123456",
            "12345678901234567a",
            "<@123456789012345678>",
            "123456789012345678901",
            "@everyone",
        ] {
            assert_eq!(parse_discord_user_id(raw), None, "{raw}");
        }
        assert_eq!(
            parse_discord_user_id(" 12345678901234567 ").as_deref(),
            Some("12345678901234567")
        );
        assert_eq!(
            parse_discord_user_id("12345678901234567890").as_deref(),
            Some("12345678901234567890")
        );
        assert_eq!(parse_discord_user_id("   ").as_deref(), Some(""));
    }

    #[test]
    fn every_source_has_a_default_route_and_webhooks_are_opt_in() {
        let config = ChannelConfig::default();
        assert_eq!(config.sources.len(), NotificationSource::ALL.len());
        for (source, toggles) in &config.sources {
            assert_eq!(toggles.webhook, *source == NotificationSource::Test);
            assert_eq!(toggles.native, *source != NotificationSource::Test);
        }
    }

    #[test]
    fn revive_keeps_good_fields_and_defaults_bad_ones() {
        let raw = serde_json::json!({
            "nativeOnlyWhileGameRunning": "yes",
            "discordPingUserId": "<@&42>",
            "soundVolume": 7,
            "sources": {
                "worldState": { "native": false, "webhook": true },
                "whisper": { "native": 1 },
                "bogus": { "native": false, "webhook": true },
            },
        });
        let config = ChannelConfig::revive(&raw).unwrap();
        assert!(!config.native_only_while_game_running);
        assert_eq!(config.discord_ping_user_id, "");
        assert_eq!(config.sound_volume, 1.0);
        assert_eq!(
            config.sources[&NotificationSource::WorldState],
            SourceToggles {
                native: false,
                webhook: true
            }
        );
        assert_eq!(
            config.sources[&NotificationSource::Whisper],
            NotificationSource::Whisper.default_toggles()
        );
        assert_eq!(ChannelConfig::revive(&serde_json::json!([1])), None);
    }

    #[test]
    fn the_state_never_serialises_a_url() {
        let state = ChannelState {
            webhooks: BTreeMap::from([(
                WebhookChannel::Discord,
                WebhookStatus {
                    configured: true,
                    masked: mask_webhook_url(DISCORD_URL),
                },
            )]),
            sources: BTreeMap::new(),
            native_only_while_game_running: false,
            discord_ping_user_id: String::new(),
            sound_enabled: true,
            sound_volume: 1.0,
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("https://discord.com/...efgh") && !json.contains("webhooks/1234"));
    }
}
