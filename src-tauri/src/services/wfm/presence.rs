//! Account presence (online / ingame / invisible), ported from WFHelper's `wfmPresence.ts`.
//!
//! Rules that can own the status: `auto` (Warframe running -> ingame) and `away` (idle PC, or game
//! closed -> invisible). A manual pick wins and disarms the away rules until the next game or idle
//! edge. The state sits behind one async mutex held across the push, so a poll can never double-send.
//! No timers here: the owner calls `tick` (hold expiry, push retry) from a single task it can abort.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Mutex;

pub const HOLD_MINUTES: [u32; 5] = [0, 30, 60, 120, 240];
const AWAY_IDLE_DEFAULT: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Online,
    Ingame,
    Invisible,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Ingame => "ingame",
            Self::Invisible => "invisible",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "online" => Some(Self::Online),
            "ingame" => Some(Self::Ingame),
            "invisible" => Some(Self::Invisible),
            _ => None,
        }
    }
    /// Invisible is already the resting state, so only a visible status can expire.
    fn can_expire(self) -> bool {
        self != Self::Invisible
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Override {
    Auto,
    Away,
}

impl Override {
    fn status(self) -> Status {
        match self {
            Self::Auto => Status::Ingame,
            Self::Away => Status::Invisible,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct PresenceState {
    pub status: Option<Status>,
    /// Epoch ms the hold ends (status drops to invisible); null while held indefinitely.
    pub expires_at: Option<f64>,
    pub auto_active: bool,
    pub away_active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, specta::Type)]
pub struct PresenceOptions {
    pub auto_ingame_enabled: bool,
    pub hold_minutes: u32,
    pub away_idle_enabled: bool,
    pub away_idle_minutes: u32,
    pub away_when_closed_enabled: bool,
}

/// Where statuses go: the live socket in production, a fake in tests.
pub trait Pusher: Send + Sync {
    fn ready(&self) -> bool;
    /// Resolves with the server-side expiry, if any.
    fn push(
        &self,
        status: Status,
        duration_secs: Option<u64>,
    ) -> impl Future<Output = Result<Option<String>, String>> + Send;
}

struct State {
    status: Option<Status>,
    expires_at: Option<i64>,
    over: Option<Override>,
    /// What to put back when the override ends; None = never captured.
    pre: Option<Status>,
    auto: bool,
    hold_minutes: u32,
    away_idle: bool,
    away_idle_minutes: u32,
    away_closed: bool,
    game_open: bool,
    idle_secs: f64,
    idle_away: bool,
    /// A manual pick disarms the away rules; the next idle or game edge re-arms them.
    armed: bool,
    /// A push failed: `tick` tries again (edges alone would never).
    retry: bool,
}

impl State {
    fn new() -> Self {
        Self {
            status: None,
            expires_at: None,
            over: None,
            pre: None,
            auto: false,
            hold_minutes: 0,
            away_idle: false,
            away_idle_minutes: AWAY_IDLE_DEFAULT,
            away_closed: false,
            game_open: false,
            idle_secs: 0.0,
            idle_away: false,
            armed: true,
            retry: false,
        }
    }

    fn view(&self) -> PresenceState {
        PresenceState {
            status: self.status,
            expires_at: self.expires_at.map(|v| v as f64),
            auto_active: self.over == Some(Override::Auto),
            away_active: self.over == Some(Override::Away),
        }
    }

    /// Auto-driven presence is bounded by the game running, so it goes up without an expiry.
    fn duration_for(&self, status: Status, auto: bool) -> Option<u64> {
        (!auto && self.hold_minutes > 0 && status.can_expire())
            .then(|| u64::from(self.hold_minutes) * 60)
    }

    fn track_deadline(&mut self, until: Option<&str>) {
        self.expires_at = until
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.timestamp_millis());
    }

    /// Recompute the idle flag; an edge either way re-arms the away rules.
    fn refresh_idle(&mut self) -> bool {
        let away = self.away_idle && self.idle_secs >= f64::from(self.away_idle_minutes) * 60.0;
        if away == self.idle_away {
            return false;
        }
        self.idle_away = away;
        self.armed = true;
        true
    }

    fn wanted(&self) -> Option<Override> {
        if self.armed && self.away_idle && self.idle_away {
            Some(Override::Away)
        } else if self.game_open && self.auto {
            Some(Override::Auto)
        } else if !self.armed {
            None
        } else if self.away_closed && !self.game_open {
            Some(Override::Away)
        } else {
            None
        }
    }
}

pub struct Presence<P: Pusher> {
    st: Mutex<State>,
    pusher: P,
    on_change: Box<dyn Fn(PresenceState) + Send + Sync>,
}

impl<P: Pusher> Presence<P> {
    pub fn new(pusher: P, on_change: impl Fn(PresenceState) + Send + Sync + 'static) -> Self {
        Self {
            st: Mutex::new(State::new()),
            pusher,
            on_change: Box::new(on_change),
        }
    }

    pub async fn state(&self) -> PresenceState {
        self.st.lock().await.view()
    }

    fn emit(&self, st: &State) {
        (self.on_change)(st.view());
    }

    async fn push(&self, st: &mut State, status: Status, auto: bool) -> Result<(), String> {
        if !self.pusher.ready() {
            return Err("Not connected to Warframe.market.".into());
        }
        let until = self
            .pusher
            .push(status, st.duration_for(status, auto))
            .await?;
        st.status = Some(status);
        st.track_deadline(until.as_deref());
        self.emit(st);
        Ok(())
    }

    /// Re-run the auto/away rules against the current game, idle and option state.
    async fn apply(&self, st: &mut State) {
        if !self.pusher.ready() {
            return;
        }
        let wanted = st.wanted();
        if wanted == st.over {
            return;
        }
        let prev = st.over;
        let ok = match wanted {
            Some(Override::Auto) => {
                // An "ingame" at rest is a stale echo of a previous push, not a restore target.
                if prev.is_none() {
                    st.pre = st.status.filter(|s| *s != Status::Ingame);
                }
                st.over = wanted;
                self.push(st, Status::Ingame, true).await.is_ok()
            }
            Some(Override::Away) => {
                // Already hidden by hand: nothing to replace and nothing to put back.
                if prev.is_none() && st.status == Some(Status::Invisible) {
                    return;
                }
                if prev.is_none() {
                    st.pre = st.status;
                }
                st.over = wanted;
                self.push(st, Status::Invisible, false).await.is_ok()
            }
            None => {
                // Unknown prior status stays hidden rather than guessing someone visible.
                let restore = st.pre.unwrap_or(Status::Invisible);
                st.over = None;
                let ok = self.push(st, restore, false).await.is_ok();
                if ok {
                    st.pre = None;
                }
                ok
            }
        };
        if !ok {
            st.over = prev;
        }
        st.retry = !ok;
    }

    /// Persisted settings, on boot and on every save.
    pub async fn set_options(&self, o: PresenceOptions) {
        let hold = if HOLD_MINUTES.contains(&o.hold_minutes) {
            o.hold_minutes
        } else {
            0
        };
        let idle_min = o.away_idle_minutes.clamp(1, 60);
        let mut st = self.st.lock().await;
        let hold_changed = hold != st.hold_minutes;
        let auto_changed = o.auto_ingame_enabled != st.auto;
        let away_changed = o.away_idle_enabled != st.away_idle
            || idle_min != st.away_idle_minutes
            || o.away_when_closed_enabled != st.away_closed;
        st.hold_minutes = hold;
        st.auto = o.auto_ingame_enabled;
        st.away_idle = o.away_idle_enabled;
        st.away_idle_minutes = idle_min;
        st.away_closed = o.away_when_closed_enabled;
        // A new duration only takes effect by re-sending the status, same as the site.
        if hold_changed
            && st.over.is_none()
            && let Some(s) = st.status.filter(|s| s.can_expire())
        {
            let _ = self.push(&mut st, s, false).await;
        }
        if away_changed {
            st.armed = true;
            st.refresh_idle();
        }
        if auto_changed || away_changed {
            self.apply(&mut st).await;
        }
        if hold_changed || auto_changed || away_changed {
            self.emit(&st);
        }
    }

    /// Re-evaluate the rules (after connecting / signing in).
    pub async fn resync(&self) {
        let mut st = self.st.lock().await;
        self.apply(&mut st).await;
    }

    /// User picked a status: it wins over auto and away until the next edge.
    pub async fn set_manual(&self, status: Status) -> Result<(), String> {
        let mut st = self.st.lock().await;
        st.over = None;
        st.pre = None;
        st.armed = false;
        self.push(&mut st, status, false).await
    }

    pub async fn sync_game(&self, open: bool) {
        let mut st = self.st.lock().await;
        if open != st.game_open {
            st.armed = true;
        }
        st.game_open = open;
        self.apply(&mut st).await;
    }

    pub async fn sync_idle(&self, seconds: f64) {
        let mut st = self.st.lock().await;
        st.idle_secs = if seconds.is_finite() && seconds > 0.0 {
            seconds
        } else {
            0.0
        };
        if st.refresh_idle() {
            self.apply(&mut st).await;
        }
    }

    /// `@wfm|event/status/set`: the authoritative status and expiry, on sign-in or after a change
    /// made elsewhere.
    pub async fn apply_server_status(&self, payload: &Value) {
        let Some(status) = payload
            .get("status")
            .and_then(Value::as_str)
            .and_then(Status::parse)
        else {
            return;
        };
        let mut st = self.st.lock().await;
        // A status the site set during an override is the newest wish: restore to that instead.
        if st.over.is_some_and(|o| o.status() != status) {
            st.pre = Some(status);
        }
        st.status = Some(status);
        st.track_deadline(payload.get("statusUntil").and_then(Value::as_str));
        self.emit(&st);
    }

    /// Signed out: forget everything the account state implied.
    pub async fn reset(&self) {
        let mut st = self.st.lock().await;
        let (auto, hold, idle, idle_min, closed) = (
            st.auto,
            st.hold_minutes,
            st.away_idle,
            st.away_idle_minutes,
            st.away_closed,
        );
        let (game, secs) = (st.game_open, st.idle_secs);
        *st = State::new();
        (
            st.auto,
            st.hold_minutes,
            st.away_idle,
            st.away_idle_minutes,
            st.away_closed,
        ) = (auto, hold, idle, idle_min, closed);
        (st.game_open, st.idle_secs) = (game, secs);
        self.emit(&st);
    }

    /// Periodic driver: mirrors a hold the server already expired, and retries a failed push.
    pub async fn tick(&self, now_ms: i64) {
        let mut st = self.st.lock().await;
        if st.expires_at.is_some_and(|e| e <= now_ms) {
            st.expires_at = None;
            st.status = Some(Status::Invisible);
            self.emit(&st);
        }
        if st.retry {
            self.apply(&mut st).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct Fake {
        ready: AtomicBool,
        fail: AtomicBool,
        sent: StdMutex<Vec<(Status, Option<u64>)>>,
        until: StdMutex<Option<String>>,
    }

    impl Pusher for Arc<Fake> {
        fn ready(&self) -> bool {
            self.ready.load(Ordering::SeqCst)
        }
        async fn push(&self, s: Status, d: Option<u64>) -> Result<Option<String>, String> {
            if self.fail.load(Ordering::SeqCst) {
                return Err("down".into());
            }
            self.sent.lock().unwrap().push((s, d));
            Ok(self.until.lock().unwrap().clone())
        }
    }

    fn setup() -> (Presence<Arc<Fake>>, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        fake.ready.store(true, Ordering::SeqCst);
        (Presence::new(fake.clone(), |_| {}), fake)
    }

    fn opts(auto: bool, idle: bool, closed: bool) -> PresenceOptions {
        PresenceOptions {
            auto_ingame_enabled: auto,
            hold_minutes: 0,
            away_idle_enabled: idle,
            away_idle_minutes: 10,
            away_when_closed_enabled: closed,
        }
    }

    fn sent(f: &Fake) -> Vec<Status> {
        f.sent.lock().unwrap().iter().map(|s| s.0).collect()
    }

    #[tokio::test]
    async fn game_start_sets_ingame_without_expiry_and_stop_restores_the_prior_status() {
        let (p, f) = setup();
        p.set_options(PresenceOptions {
            hold_minutes: 30,
            ..opts(true, false, false)
        })
        .await;
        p.set_manual(Status::Online).await.unwrap();
        p.sync_game(true).await;
        p.sync_game(false).await;
        assert_eq!(sent(&f), [Status::Online, Status::Ingame, Status::Online]);
        // manual online carries the hold; the auto ingame push does not
        let durations: Vec<_> = f.sent.lock().unwrap().iter().map(|s| s.1).collect();
        assert_eq!(durations, [Some(1800), None, Some(1800)]);
        assert!(!p.state().await.auto_active);
    }

    #[tokio::test]
    async fn stale_ingame_is_not_a_restore_target() {
        let (p, f) = setup();
        p.apply_server_status(&json!({ "status": "ingame" })).await;
        p.set_options(opts(true, false, false)).await;
        p.sync_game(true).await;
        p.sync_game(false).await;
        assert_eq!(sent(&f), [Status::Ingame, Status::Invisible]);
    }

    #[tokio::test]
    async fn idle_away_hides_and_releasing_restores() {
        let (p, f) = setup();
        p.set_options(opts(false, true, false)).await;
        p.set_manual(Status::Online).await.unwrap();
        p.sync_idle(10.0 * 60.0).await;
        assert!(p.state().await.away_active);
        p.sync_idle(5.0).await;
        assert_eq!(
            sent(&f),
            [Status::Online, Status::Invisible, Status::Online]
        );
    }

    #[tokio::test]
    async fn already_invisible_by_hand_is_left_alone_when_away_fires() {
        let (p, f) = setup();
        p.set_manual(Status::Invisible).await.unwrap();
        p.set_options(opts(false, false, true)).await; // manual pick disarmed the away rules
        p.sync_game(true).await; // edge re-arms
        p.sync_game(false).await; // away wanted, but already hidden by hand
        assert_eq!(sent(&f), [Status::Invisible]);
        assert!(!p.state().await.away_active);
    }

    #[tokio::test]
    async fn manual_pick_disarms_away_until_the_next_edge() {
        let (p, _) = setup();
        p.set_options(opts(false, false, true)).await; // game closed + rule on -> away right away        p.set_manual(Status::Online).await.unwrap();        p.sync_game(false).await; // no edge: still disarmed        assert_eq!(sent(&f), [Status::Invisible, Status::Online]);        p.sync_game(true).await; // edge re-arms; game open and no auto -> nothing wanted        p.sync_game(false).await; // closed again -> away        assert_eq!(sent(&f), [Status::Invisible, Status::Online, Status::Invisible]);
    }

    #[tokio::test]
    async fn failed_push_keeps_the_old_override_and_tick_retries() {
        let (p, f) = setup();
        p.set_options(opts(true, false, false)).await;
        f.fail.store(true, Ordering::SeqCst);
        p.sync_game(true).await;
        assert!(!p.state().await.auto_active);
        f.fail.store(false, Ordering::SeqCst);
        p.tick(0).await;
        assert!(p.state().await.auto_active);
        assert_eq!(sent(&f), [Status::Ingame]);
    }

    #[tokio::test]
    async fn nothing_is_pushed_while_not_ready_and_resync_catches_up() {
        let (p, f) = setup();
        f.ready.store(false, Ordering::SeqCst);
        p.set_options(opts(true, false, false)).await;
        p.sync_game(true).await;
        assert!(sent(&f).is_empty());
        f.ready.store(true, Ordering::SeqCst);
        p.resync().await;
        assert_eq!(sent(&f), [Status::Ingame]);
    }

    #[tokio::test]
    async fn server_status_during_an_override_becomes_the_restore_target() {
        let (p, f) = setup();
        p.set_options(opts(true, false, false)).await;
        p.sync_game(true).await;
        p.apply_server_status(&json!({ "status": "online" })).await; // changed on the site
        p.sync_game(false).await;
        assert_eq!(sent(&f), [Status::Ingame, Status::Online]);
    }

    #[tokio::test]
    async fn server_expiry_is_tracked_and_tick_mirrors_the_drop() {
        let (p, _) = setup();
        p.apply_server_status(
            &json!({ "status": "online", "statusUntil": "2030-01-01T00:00:00Z" }),
        )
        .await;
        let until = 1_893_456_000_000_i64;
        assert_eq!(p.state().await.expires_at, Some(until as f64));
        p.tick(until - 1).await;
        assert_eq!(p.state().await.status, Some(Status::Online));
        p.tick(until).await;
        let s = p.state().await;
        assert_eq!((s.status, s.expires_at), (Some(Status::Invisible), None));
    }

    #[tokio::test]
    async fn changing_the_hold_resends_a_visible_status_but_not_during_an_override() {
        let (p, f) = setup();
        p.set_manual(Status::Online).await.unwrap();
        p.set_options(PresenceOptions {
            hold_minutes: 60,
            ..opts(false, false, false)
        })
        .await;
        assert_eq!(
            f.sent.lock().unwrap().last().unwrap(),
            &(Status::Online, Some(3600))
        );
        // junk hold snaps to 0
        p.set_options(PresenceOptions {
            hold_minutes: 7,
            ..opts(false, false, false)
        })
        .await;
        assert_eq!(
            f.sent.lock().unwrap().last().unwrap(),
            &(Status::Online, None)
        );
    }

    #[tokio::test]
    async fn bad_server_payloads_are_ignored_and_reset_keeps_options() {
        let (p, f) = setup();
        p.apply_server_status(&json!({ "status": "dnd" })).await;
        p.apply_server_status(&json!(null)).await;
        assert_eq!(p.state().await.status, None);
        p.set_options(opts(true, false, false)).await;
        p.set_manual(Status::Online).await.unwrap();
        p.reset().await;
        assert_eq!(p.state().await.status, None);
        p.sync_game(true).await; // option survived the reset
        assert_eq!(sent(&f), [Status::Online, Status::Ingame]);
    }

    #[tokio::test]
    async fn manual_set_reports_failure() {
        let (p, f) = setup();
        f.ready.store(false, Ordering::SeqCst);
        assert!(p.set_manual(Status::Online).await.is_err());
    }
}
