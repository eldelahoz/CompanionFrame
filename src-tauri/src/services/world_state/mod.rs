//! World state: DE's `worldState.php` (with community fallbacks) parsed into typed structs.
//! Desktop notifications hang off the [`Notifier`] trait (services/notify).

pub mod fetch;
pub mod notify;
pub mod parser;
pub mod regions;
pub mod steel_path;

use crate::services::clock::now_ms;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::services::bundled_data::BundledData;
use crate::services::item_db::ItemDbState;
use crate::services::notify::Notifications;
use regions::Ctx;

const REFRESH: Duration = Duration::from_secs(60);

#[derive(Serialize, Clone, Debug, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Fissure {
    pub expiry: Option<String>,
    pub tier: String,
    pub mission_type: String,
    pub node: String,
    pub node_id: String,
    pub is_hard: bool,
    pub is_storm: bool,
    pub expired: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TraderItem {
    pub unique_name: String,
    pub item: String,
    pub ducats: Option<u32>,
    pub credits: Option<u32>,
    pub image_override: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Trader {
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub location: String,
    pub inventory: Vec<TraderItem>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SortieMission {
    pub node: String,
    pub mission: String,
    pub modifier: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Sortie {
    pub id: String,
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub boss: String,
    pub missions: Vec<SortieMission>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchonMission {
    pub node: String,
    pub mission: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ArchonHunt {
    pub id: String,
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub boss: String,
    pub missions: Vec<ArchonMission>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NightwaveChallenge {
    pub id: String,
    /// Challenge path tail; joins the inventory's ChallengeProgress.
    pub name: String,
    pub title: String,
    pub description: String,
    pub standing: Option<u32>,
    pub required_count: u32,
    pub is_daily: bool,
    pub is_elite: bool,
    pub activation: Option<String>,
    pub expiry: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Nightwave {
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub season: u32,
    pub phase: u32,
    pub affiliation_tag: String,
    pub challenges: Vec<NightwaveChallenge>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TimeWindow {
    pub activation: Option<String>,
    pub expiry: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    /// `challenge`, `reward` or `upgrade`.
    pub kind: String,
    pub label: String,
    pub description: Option<String>,
    pub unique_name: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarDay {
    pub day: u32,
    pub events: Vec<CalendarEvent>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CalendarSeason {
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub season: String,
    pub days: Vec<CalendarDay>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AlertItem {
    pub name: String,
    pub count: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub node: String,
    pub mission: String,
    pub faction: String,
    pub min_level: u32,
    pub max_level: u32,
    pub credits: u32,
    pub items: Vec<AlertItem>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DailyDeal {
    pub unique_name: String,
    pub item: String,
    pub image_override: Option<String>,
    pub discount: u32,
    pub original_price: u32,
    pub sale_price: u32,
    pub total: u32,
    pub sold: u32,
    pub expiry: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CountedItem {
    pub count: u32,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InvasionReward {
    pub items: Vec<String>,
    pub counted_items: Vec<CountedItem>,
    pub credits: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InvasionSide {
    pub reward: InvasionReward,
    pub faction: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Invasion {
    pub id: String,
    pub node: String,
    pub desc: Option<String>,
    pub attacker: InvasionSide,
    pub defender: InvasionSide,
    pub vs_infestation: bool,
    pub completion: f64,
    pub completed: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BountyJob {
    #[serde(rename = "type")]
    pub kind: String,
    pub enemy_levels: [u32; 2],
    /// Seed bounties only: 0-based tier; reward pools match by index, not level.
    pub tier_index: Option<u32>,
    pub standing_stages: Vec<u32>,
    pub min_mr: u32,
    pub challenge_desc: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyndicateBounty {
    pub syndicate: String,
    pub syndicate_key: String,
    pub expiry: Option<String>,
    pub jobs: Vec<BountyJob>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
pub struct SteelPathReward {
    pub name: String,
    pub cost: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SteelPathUpcoming {
    pub name: String,
    pub cost: u32,
    pub activation: String,
    pub expiry: String,
    pub week_offset: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SteelPath {
    pub current_reward: SteelPathReward,
    pub activation: Option<String>,
    pub expiry: Option<String>,
    pub rotation: Vec<SteelPathReward>,
    pub upcoming: Vec<SteelPathUpcoming>,
    pub evergreens: Vec<SteelPathReward>,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DayCycle {
    pub is_day: bool,
    pub time_left: String,
    pub expiry: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VallisCycle {
    pub is_warm: bool,
    pub time_left: String,
    pub expiry: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CambionCycle {
    /// `fass` by day, `vome` by night.
    pub active: String,
    pub time_left: String,
    pub expiry: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, specta::Type)]
pub struct CircuitChoices {
    pub category: String,
    pub choices: Vec<String>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DuviriCycle {
    pub state: Option<String>,
    pub next_state: Option<String>,
    pub expiry: Option<String>,
    pub choices: Vec<CircuitChoices>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorldState {
    pub fissures: Vec<Fissure>,
    pub void_trader: Option<Trader>,
    pub vault_trader: Option<Trader>,
    pub sortie: Option<Sortie>,
    pub archon_hunt: Option<ArchonHunt>,
    pub nightwave: Option<Nightwave>,
    pub descents: Option<TimeWindow>,
    pub calendar_season: Option<CalendarSeason>,
    pub alerts: Vec<Alert>,
    pub steel_path: Option<SteelPath>,
    pub duviri_cycle: Option<DuviriCycle>,
    pub earth_cycle: Option<DayCycle>,
    pub cetus_cycle: Option<DayCycle>,
    pub vallis_cycle: Option<VallisCycle>,
    pub cambion_cycle: Option<CambionCycle>,
    pub invasions: Vec<Invasion>,
    pub bounties: Vec<SyndicateBounty>,
    pub bounty_rotation: Option<String>,
    pub daily_deals: Vec<DailyDeal>,
}

impl WorldState {
    pub fn empty(now_ms: i64) -> Self {
        Self {
            steel_path: Some(steel_path::compute(now_ms)),
            ..Self::default()
        }
    }
}

pub use crate::services::notify::{NoNotifier, Notifier};

/// Managed state: last good world state and the notification sink.
pub struct WorldService {
    last: Mutex<Option<(Instant, WorldState)>>,
    notifier: Arc<dyn Notifier>,
}

impl WorldService {
    pub fn new(notifier: Arc<dyn Notifier>) -> Self {
        Self {
            last: Mutex::new(None),
            notifier,
        }
    }

    fn fresh(&self) -> Option<WorldState> {
        let last = self.last.lock().unwrap();
        last.as_ref()
            .filter(|(at, _)| at.elapsed() < REFRESH)
            .map(|(_, s)| s.clone())
    }

    fn stale(&self) -> Option<WorldState> {
        self.last.lock().unwrap().as_ref().map(|(_, s)| s.clone())
    }

    fn store(&self, next: WorldState) {
        let mut last = self.last.lock().unwrap();
        self.notifier
            .world_state_changed(last.as_ref().map(|(_, s)| s), &next);
        *last = Some((Instant::now(), next));
    }
}

async fn refresh(app: &AppHandle) -> Result<WorldState, String> {
    let bundled = BundledData::from_app(app)?;
    let code = app.state::<ItemDbState>().ui_code();
    let ctx = tauri::async_runtime::spawn_blocking(move || Ctx::load(&bundled, code))
        .await
        .map_err(|e| e.to_string())?;
    let http = fetch::ReqwestHttp::new().map_err(|e| e.to_string())?;
    let next = fetch::fetch_and_parse(&http, &ctx, now_ms()).await?;
    app.state::<WorldService>().store(next.clone());
    let _ = app.emit("world-state-updated", ());
    Ok(next)
}

/// The cached state while fresh, else a refetch; a failed fetch emits `world-state-fetch-error`
/// and serves the last good state (or an empty one) instead of failing the view.
#[tauri::command]
#[specta::specta]
pub async fn get_world_state(app: AppHandle) -> WorldState {
    if let Some(s) = app.state::<WorldService>().fresh() {
        return s;
    }
    match refresh(&app).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("world state fetch failed: {e}");
            let _ = app.emit("world-state-fetch-error", e);
            app.state::<WorldService>()
                .stale()
                .unwrap_or_else(|| WorldState::empty(now_ms()))
        }
    }
}

/// Seeds the cache shortly after startup and keeps it current every minute.
pub fn setup(app: &tauri::App) {
    let data = app.path().app_data_dir().expect("app data dir");
    let prefs = Arc::new(notify::WorldNotifyStore::load(
        data.join("world-notify.json"),
    ));
    app.manage(prefs.clone());
    let notifier = Arc::new(notify::WorldNotifier::new(
        Arc::new(app.state::<Notifications>().inner().clone()),
        prefs,
    ));
    app.manage(WorldService::new(notifier.clone()));
    // Lead-time heads-ups need a finer clock than the minute refresh.
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            notifier.check_pre_cycle(now_ms());
        }
    });
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            if let Err(e) = refresh(&handle).await {
                tracing::warn!("world state refresh failed: {e}");
            }
            tokio::time::sleep(REFRESH).await;
        }
    });
}
