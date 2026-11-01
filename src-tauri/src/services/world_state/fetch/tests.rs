//! Ported from WFHelper's `worldStateFetchChain.test.ts`, plus the cycle math.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::json;

use super::*;

const NOW: i64 = 1_800_000_000_000;

/// Answers from a route table; any URL without a route fails like a dead host.
#[derive(Default)]
struct Mock {
    routes: HashMap<String, Result<Value, String>>,
    calls: Mutex<Vec<String>>,
}

impl Mock {
    fn route(mut self, url: &str, body: Result<Value, &str>) -> Self {
        self.routes.insert(url.into(), body.map_err(str::to_owned));
        self
    }

    /// The world-state URLs that were requested, in order.
    fn world_state_calls(&self) -> Vec<String> {
        let world = [DE_URLS[0], DE_URLS[1], ORACLE_WORLD_STATE];
        let calls = self.calls.lock().unwrap();
        calls
            .iter()
            .filter(|u| world.contains(&u.as_str()))
            .cloned()
            .collect()
    }
}

impl Http for Mock {
    async fn get_json(&self, url: &str, _: Duration) -> Result<Value, String> {
        self.calls.lock().unwrap().push(url.to_owned());
        self.routes
            .get(url)
            .cloned()
            .unwrap_or_else(|| Err(format!("no route for {url}")))
    }
}

fn empty_state() -> Value {
    json!({ "ActiveMissions": [] })
}

async fn run(http: &Mock) -> Result<WorldState, String> {
    fetch_and_parse(http, &Ctx::default(), NOW).await
}

#[tokio::test]
async fn uses_de_first_and_never_touches_the_community_oracle() {
    let http = Mock::default().route(DE_URLS[0], Ok(empty_state()));
    run(&http).await.unwrap();
    assert_eq!(http.world_state_calls(), [DE_URLS[0]]);
}

#[tokio::test]
async fn falls_through_to_the_legacy_de_path_then_the_oracle() {
    let http = Mock::default()
        .route(DE_URLS[0], Err("HTTP 404"))
        .route(DE_URLS[1], Err("HTTP 404"))
        .route(ORACLE_WORLD_STATE, Ok(empty_state()));
    run(&http).await.unwrap();
    assert_eq!(
        http.world_state_calls(),
        [DE_URLS[0], DE_URLS[1], ORACLE_WORLD_STATE]
    );
}

#[tokio::test]
async fn rejects_an_empty_successful_response_and_tries_the_next_de_path() {
    let http = Mock::default()
        .route(DE_URLS[0], Ok(json!({})))
        .route(DE_URLS[1], Ok(empty_state()));
    run(&http).await.unwrap();
    assert_eq!(http.world_state_calls(), [DE_URLS[0], DE_URLS[1]]);
}

#[tokio::test]
async fn rejects_empty_responses_from_both_de_paths_before_using_the_oracle() {
    let http = Mock::default()
        .route(DE_URLS[0], Ok(json!({})))
        .route(DE_URLS[1], Ok(json!({})))
        .route(ORACLE_WORLD_STATE, Ok(empty_state()));
    run(&http).await.unwrap();
    assert_eq!(
        http.world_state_calls(),
        [DE_URLS[0], DE_URLS[1], ORACLE_WORLD_STATE]
    );
}

#[tokio::test]
async fn errors_instead_of_reporting_an_empty_world_when_every_source_fails() {
    let http = Mock::default()
        .route(DE_URLS[0], Err("HTTP 404"))
        .route(DE_URLS[1], Err("HTTP 404"))
        .route(ORACLE_WORLD_STATE, Err("HTTP 404"));
    let err = run(&http).await.unwrap_err();
    assert!(err.starts_with("every world-state source failed"), "{err}");
}

#[tokio::test]
async fn cycles_survive_every_community_api_being_down() {
    let state = run(&Mock::default().route(DE_URLS[0], Ok(empty_state())))
        .await
        .unwrap();
    // Vallis and Duviri are pure math; the rest degrade to None.
    assert!(state.vallis_cycle.is_some());
    assert!(state.duviri_cycle.as_ref().unwrap().state.is_some());
    assert!(state.cetus_cycle.is_none() && state.earth_cycle.is_none());
    assert!(state.steel_path.is_some());
}

#[tokio::test]
async fn oracle_expiry_drives_cetus_cambion_and_the_earth_fallback() {
    let expiry = NOW + 10_000_000;
    let http = Mock::default().route(DE_URLS[0], Ok(empty_state())).route(
        ORACLE_BOUNTY_CYCLE,
        Ok(json!({ "expiry": expiry, "rot": "B" })),
    );
    let state = run(&http).await.unwrap();
    let cetus = state.cetus_cycle.unwrap();
    assert!(cetus.is_day);
    assert_eq!(cetus.expiry, iso(expiry - POE_NIGHT_MS));
    assert_eq!(state.cambion_cycle.unwrap().active, "fass");
    assert_eq!(state.earth_cycle.unwrap().expiry, cetus.expiry);
    assert_eq!(state.bounty_rotation.as_deref(), Some("B"));
}

#[tokio::test]
async fn warframestat_fills_invasions_and_bounty_gaps_only() {
    let http = Mock::default()
        .route(
            DE_URLS[0],
            Ok(
                json!({ "SyndicateMissions": [{ "Tag": "CetusSyndicate", "Expiry": {
                "$date": { "$numberLong": (NOW + 1000).to_string() } },
                "Jobs": [{ "jobType": "/x/FooBountyExt" }] }] }),
            ),
        )
        .route(
            &format!("{WARFRAMESTAT}/invasions"),
            Ok(json!([
                { "id": "a", "node": "Pacific (Earth)", "completion": 12.345,
                  "attacker": { "faction": "Grineer", "reward": { "credits": 1 } } },
                { "id": "b", "completed": true },
            ])),
        )
        .route(
            &format!("{WARFRAMESTAT}/syndicateMissions"),
            Ok(json!([
                { "syndicate": "Ostrons", "syndicateKey": "Ostrons", "jobs": [{ "type": "dup" }] },
                { "syndicate": "Solaris United", "syndicateKey": "Solaris United",
                  "jobs": [{ "type": "Spy", "enemyLevels": [5, 15], "minMR": 2 }] },
                { "syndicate": "Not Tracked", "jobs": [{ "type": "x" }] },
            ])),
        );
    let state = run(&http).await.unwrap();
    assert_eq!(state.invasions.len(), 1);
    assert_eq!(state.invasions[0].completion, 12.3);
    let keys: Vec<_> = state
        .bounties
        .iter()
        .map(|b| b.syndicate_key.as_str())
        .collect();
    // The raw Cetus bounty wins over warframestat's duplicate Ostrons entry.
    assert_eq!(keys, ["CetusSyndicate", "Solaris United"]);
    assert_eq!(state.bounties[1].jobs[0].enemy_levels, [5, 15]);
}

#[test]
fn vallis_alternates_warm_and_cold() {
    let epoch = Utc
        .with_ymd_and_hms(2018, 11, 10, 8, 13, 48)
        .unwrap()
        .timestamp_millis();
    let warm = vallis_cycle(epoch + 1000);
    assert!(warm.is_warm);
    assert_eq!(warm.expiry, iso(epoch + VALLIS_WARM_MS));
    let cold = vallis_cycle(epoch + VALLIS_WARM_MS + 1000);
    assert!(!cold.is_warm);
    assert_eq!(cold.expiry, iso(epoch + VALLIS_PERIOD_MS));
}

#[test]
fn duviri_mood_advances_every_two_hours() {
    let mood = duviri_mood(0);
    assert_eq!(
        (mood.state.as_deref(), mood.next_state.as_deref()),
        (Some("Sorrow"), Some("Fear"))
    );
    let later = duviri_mood(DUVIRI_MOOD_PERIOD_MS * 5);
    assert_eq!(later.state.as_deref(), Some("Sorrow"));
    assert_eq!(later.expiry, Some(iso(DUVIRI_MOOD_PERIOD_MS * 6)));
}

#[test]
fn cetus_turns_night_in_the_last_fifty_minutes() {
    let expiry = NOW + POE_NIGHT_MS / 2;
    let (cetus, cambion) = cetus_cambion(expiry, NOW);
    assert!(!cetus.is_day);
    assert_eq!(cambion.active, "vome");
    assert_eq!(cetus.expiry, iso(expiry));
}
