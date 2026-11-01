//! Source chain and planet cycles. DE's own worldState first (so normal polling never loads the
//! community oracle), then the oracle copy; cycles and invasions fill in from community APIs
//! and local math, each failing soft.

use std::future::Future;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use serde_json::Value;

use super::parser::{self, BOUNTY_SYNDICATES};
use super::regions::Ctx;
use super::{
    BountyJob, CambionCycle, CircuitChoices, CountedItem, DayCycle, DuviriCycle, Invasion,
    InvasionReward, InvasionSide, SyndicateBounty, VallisCycle, WorldState, steel_path,
};

pub const DE_URLS: [&str; 2] = [
    "https://api.warframe.com/cdn/worldState.php",
    "https://content.warframe.com/dynamic/worldState.php",
];
pub const ORACLE_WORLD_STATE: &str = "https://oracle.browse.wf/worldState.json";
pub const ORACLE_BOUNTY_CYCLE: &str = "https://oracle.browse.wf/bounty-cycle";
pub const WARFRAMESTAT: &str = "https://api.warframestat.us/pc";

const FETCH_TIMEOUT: Duration = Duration::from_secs(20);
const CYCLE_TIMEOUT: Duration = Duration::from_secs(4);
const MAX_BODY: usize = 32 * 1024 * 1024;

const VALLIS_PERIOD_MS: i64 = 1_600_000;
const VALLIS_WARM_MS: i64 = 400_000;
const POE_NIGHT_MS: i64 = 3_000_000;
const DUVIRI_MOOD_PERIOD_MS: i64 = 7_200_000;
const DUVIRI_MOODS: [&str; 5] = ["Sorrow", "Fear", "Joy", "Anger", "Envy"];

/// Keys that mark a payload as a real world state rather than an empty or error body.
const MARKERS: [&str; 6] = [
    "ActiveMissions",
    "VoidStorms",
    "Invasions",
    "VoidTraders",
    "SyndicateMissions",
    "EndlessXpSchedule",
];

/// GET a JSON document; errors on a non-2xx status or a timeout. Mocked in tests.
pub trait Http: Sync {
    fn get_json(
        &self,
        url: &str,
        timeout: Duration,
    ) -> impl Future<Output = Result<Value, String>> + Send;
}

pub struct ReqwestHttp(reqwest::Client);

impl ReqwestHttp {
    pub fn new() -> reqwest::Result<Self> {
        crate::services::net::client().map(Self)
    }
}

impl Http for ReqwestHttp {
    async fn get_json(&self, url: &str, timeout: Duration) -> Result<Value, String> {
        let get = async {
            let res = self
                .0
                .get(url)
                .header(reqwest::header::ACCEPT, "application/json")
                .timeout(timeout)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                return Err(format!("HTTP {} for {url}", res.status()));
            }
            let bytes = res.bytes().await.map_err(|e| e.to_string())?;
            if bytes.len() > MAX_BODY {
                return Err(format!("{url}: body exceeds {MAX_BODY} bytes"));
            }
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())
        };
        get.await
    }
}

fn is_world_state(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|o| MARKERS.iter().any(|m| o.contains_key(*m)))
}

fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// ---- cycles that are pure math ----

pub fn vallis_cycle(now: i64) -> VallisCycle {
    let epoch = Utc
        .with_ymd_and_hms(2018, 11, 10, 8, 13, 48)
        .unwrap()
        .timestamp_millis();
    let elapsed = (now - epoch).rem_euclid(VALLIS_PERIOD_MS);
    let is_warm = elapsed < VALLIS_WARM_MS;
    let left = if is_warm {
        VALLIS_WARM_MS - elapsed
    } else {
        VALLIS_PERIOD_MS - elapsed
    };
    VallisCycle {
        is_warm,
        time_left: String::new(),
        expiry: iso(now + left),
    }
}

/// Cetus and Cambion Drift share the bounty-cycle expiry; the last 50 minutes are night.
pub fn cetus_cambion(bounty_expiry: i64, now: i64) -> (DayCycle, CambionCycle) {
    let night_start = bounty_expiry - POE_NIGHT_MS;
    let is_day = now < night_start;
    let expiry = iso(if is_day { night_start } else { bounty_expiry });
    (
        DayCycle {
            is_day,
            time_left: String::new(),
            expiry: expiry.clone(),
        },
        CambionCycle {
            active: if is_day { "fass" } else { "vome" }.into(),
            time_left: String::new(),
            expiry,
        },
    )
}

pub fn duviri_mood(now: i64) -> DuviriCycle {
    let index = now / DUVIRI_MOOD_PERIOD_MS;
    let mood = |i: i64| DUVIRI_MOODS[(i as usize) % DUVIRI_MOODS.len()].to_owned();
    DuviriCycle {
        state: Some(mood(index)),
        next_state: Some(mood(index + 1)),
        expiry: Some(iso((index + 1) * DUVIRI_MOOD_PERIOD_MS)),
        choices: vec![],
    }
}

// ---- community fallbacks ----

async fn fetch_earth_cycle(http: &impl Http) -> Option<DayCycle> {
    let data = match http.get_json(WARFRAMESTAT, CYCLE_TIMEOUT).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("world state: earth cycle fetch failed: {e}");
            return None;
        }
    };
    let earth = data
        .get("earthCycle")
        .filter(|c| c.is_object())
        .unwrap_or(&data);
    let expiry = earth
        .get("expiry")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())?
        .timestamp_millis();
    let state = ["state", "timeOfDay"]
        .iter()
        .find_map(|k| earth.get(k)?.as_str())
        .unwrap_or("")
        .to_lowercase();
    let is_day = earth
        .get("isDay")
        .and_then(Value::as_bool)
        .or(match state.as_str() {
            "day" => Some(true),
            "night" => Some(false),
            _ => None,
        })?;
    Some(DayCycle {
        is_day,
        time_left: earth
            .get("timeLeft")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        expiry: iso(expiry),
    })
}

/// Keep the Circuit empty when the fallback is unavailable.
async fn fetch_duviri_choices(http: &impl Http) -> Vec<CircuitChoices> {
    match http
        .get_json(&format!("{WARFRAMESTAT}/duviriCycle"), CYCLE_TIMEOUT)
        .await
    {
        Ok(data) => data
            .get("choices")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|e| {
                let category = e.get("category")?.as_str()?.to_owned();
                let choices: Vec<String> = e
                    .get("choices")?
                    .as_array()?
                    .iter()
                    .filter_map(|c| c.as_str().map(str::to_owned))
                    .collect();
                (!category.is_empty() && !choices.is_empty())
                    .then_some(CircuitChoices { category, choices })
            })
            .collect(),
        Err(e) => {
            tracing::warn!("world state: duviri choices fetch failed: {e}");
            vec![]
        }
    }
}

struct Cycles {
    earth: Option<DayCycle>,
    cetus: Option<DayCycle>,
    cambion: Option<CambionCycle>,
    vallis: VallisCycle,
    duviri: DuviriCycle,
    bounties: Vec<SyndicateBounty>,
    rotation: Option<String>,
}

async fn fetch_cycles(
    http: &impl Http,
    ctx: &Ctx,
    known_choices: Vec<CircuitChoices>,
    now: i64,
) -> Cycles {
    let choices = async {
        if known_choices.is_empty() {
            fetch_duviri_choices(http).await
        } else {
            known_choices.clone()
        }
    };
    let (oracle, earth, choices) = tokio::join!(
        http.get_json(ORACLE_BOUNTY_CYCLE, CYCLE_TIMEOUT),
        fetch_earth_cycle(http),
        choices
    );

    let mut cycles = Cycles {
        earth,
        cetus: None,
        cambion: None,
        vallis: vallis_cycle(now),
        duviri: DuviriCycle {
            choices,
            ..duviri_mood(now)
        },
        bounties: vec![],
        rotation: None,
    };
    match oracle {
        Ok(data) => {
            let expiry = data.get("expiry").and_then(Value::as_f64).unwrap_or(0.0) as i64;
            if expiry != 0 {
                let (cetus, cambion) = cetus_cambion(expiry, now);
                cycles.cetus = Some(cetus);
                cycles.cambion = Some(cambion);
            }
            cycles.bounties = parser::parse_bounty_cycle(&data, ctx);
            cycles.rotation = data
                .get("rot")
                .and_then(Value::as_str)
                .filter(|r| !r.is_empty())
                .map(str::to_owned);
        }
        Err(e) => tracing::warn!("world state: oracle bounty-cycle fetch failed: {e}"),
    }
    if cycles.earth.is_none() {
        cycles.earth = cycles.cetus.clone().map(|c| DayCycle {
            time_left: String::new(),
            ..c
        });
    }
    cycles
}

fn count(v: Option<&Value>) -> u32 {
    v.and_then(Value::as_f64).unwrap_or(0.0).max(0.0) as u32
}

fn str_or(v: &Value, key: &str, default: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_owned()
}

fn extra_side(side: Option<&Value>) -> InvasionSide {
    let empty = Value::Null;
    let side = side.unwrap_or(&empty);
    let reward = side.get("reward").unwrap_or(&empty);
    let strings = |key: &str| -> Vec<String> {
        reward
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .filter_map(|s| s.as_str().map(str::to_owned))
            .collect()
    };
    InvasionSide {
        reward: InvasionReward {
            items: strings("items"),
            counted_items: reward
                .get("countedItems")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|c| CountedItem {
                    count: count(c.get("count")),
                    kind: str_or(c, "type", ""),
                })
                .collect(),
            credits: count(reward.get("credits")),
        },
        faction: str_or(side, "faction", "Unknown"),
    }
}

/// warframestat.us invasions and syndicate missions, used only to fill what DE's payload lacks.
async fn fetch_extras(http: &impl Http) -> (Vec<Invasion>, Vec<SyndicateBounty>) {
    let (invasions_url, syndicates_url) = (
        format!("{WARFRAMESTAT}/invasions"),
        format!("{WARFRAMESTAT}/syndicateMissions"),
    );
    let (invasions, syndicates) = tokio::join!(
        http.get_json(&invasions_url, CYCLE_TIMEOUT),
        http.get_json(&syndicates_url, CYCLE_TIMEOUT)
    );
    let rows = |r: Result<Value, String>, what: &str| match r {
        Ok(Value::Array(a)) => a,
        Ok(_) => vec![],
        Err(e) => {
            tracing::warn!("world state: {what} fetch failed: {e}");
            vec![]
        }
    };

    let invasions = rows(invasions, "invasions")
        .iter()
        .filter(|i| i.is_object() && i.get("completed") != Some(&Value::Bool(true)))
        .map(|i| Invasion {
            id: str_or(i, "id", ""),
            node: str_or(i, "node", "Unknown"),
            desc: Some(str_or(i, "desc", "")),
            attacker: extra_side(i.get("attacker")),
            defender: extra_side(i.get("defender")),
            vs_infestation: i.get("vsInfestation") == Some(&Value::Bool(true)),
            completion: i
                .get("completion")
                .and_then(Value::as_f64)
                .map_or(0.0, |c| (c * 10.0).round() / 10.0),
            completed: false,
        })
        .collect();

    let bounties = rows(syndicates, "syndicateMissions")
        .iter()
        .filter(|sm| BOUNTY_SYNDICATES.contains(&str_or(sm, "syndicate", "").as_str()))
        .filter_map(|sm| {
            let jobs = sm.get("jobs")?.as_array().filter(|j| !j.is_empty())?;
            Some(SyndicateBounty {
                syndicate: str_or(sm, "syndicate", ""),
                syndicate_key: str_or(sm, "syndicateKey", ""),
                expiry: sm.get("expiry").and_then(Value::as_str).map(str::to_owned),
                jobs: jobs
                    .iter()
                    .map(|j| {
                        let level = |at: usize| count(j.get("enemyLevels").and_then(|l| l.get(at)));
                        BountyJob {
                            kind: str_or(j, "type", "Unknown"),
                            enemy_levels: [level(0), level(1)],
                            tier_index: None,
                            standing_stages: j
                                .get("standingStages")
                                .and_then(Value::as_array)
                                .map(Vec::as_slice)
                                .unwrap_or_default()
                                .iter()
                                .map(|s| count(Some(s)))
                                .collect(),
                            min_mr: count(j.get("minMR")),
                            challenge_desc: None,
                        }
                    })
                    .collect(),
            })
        })
        .collect();
    (invasions, bounties)
}

// ---- the chain ----

async fn fetch_de(http: &impl Http) -> Option<Value> {
    for url in DE_URLS {
        match http.get_json(url, FETCH_TIMEOUT).await {
            Ok(raw) if is_world_state(&raw) => return Some(raw),
            Ok(_) => tracing::warn!("world state: {url} returned an invalid payload"),
            Err(e) => tracing::warn!("world state: {url} failed: {e}"),
        }
    }
    None
}

async fn fetch_oracle(http: &impl Http) -> Result<Value, String> {
    let raw = http.get_json(ORACLE_WORLD_STATE, FETCH_TIMEOUT).await?;
    if is_world_state(&raw) {
        Ok(raw)
    } else {
        Err("oracle returned an invalid payload".into())
    }
}

pub async fn fetch_and_parse(http: &impl Http, ctx: &Ctx, now: i64) -> Result<WorldState, String> {
    let raw = match fetch_de(http).await {
        Some(raw) => raw,
        None => fetch_oracle(http)
            .await
            .map_err(|e| format!("every world-state source failed: {e}"))?,
    };
    let mut state =
        parser::parse_raw(&raw, now, ctx).ok_or("world-state payload could not be parsed")?;

    let known = state
        .duviri_cycle
        .as_ref()
        .map(|d| d.choices.clone())
        .unwrap_or_default();
    let (cycles, (extra_invasions, extra_bounties)) =
        tokio::join!(fetch_cycles(http, ctx, known, now), fetch_extras(http));

    // Raw bounties first, warframestat fills gaps (by display name), the seed oracle fills the rest.
    let mut bounties = std::mem::take(&mut state.bounties);
    let raw_names: Vec<&str> = bounties
        .iter()
        .filter_map(|b| parser::raw_bounty_name(&b.syndicate_key))
        .collect();
    let from_extras: Vec<_> = extra_bounties
        .into_iter()
        .filter(|b| !b.syndicate_key.is_empty() && !raw_names.contains(&b.syndicate_key.as_str()))
        .collect();
    for b in from_extras.into_iter().chain(cycles.bounties) {
        if !b.syndicate_key.is_empty()
            && !bounties.iter().any(|x| x.syndicate_key == b.syndicate_key)
        {
            bounties.push(b);
        }
    }

    if state.invasions.is_empty() {
        state.invasions = extra_invasions;
    }
    state.bounties = bounties;
    state.bounty_rotation = cycles.rotation;
    state.earth_cycle = cycles.earth;
    state.cetus_cycle = cycles.cetus;
    state.cambion_cycle = cycles.cambion;
    state.vallis_cycle = Some(cycles.vallis);
    state.duviri_cycle = Some(cycles.duviri);
    state.steel_path = Some(steel_path::compute(now));
    Ok(state)
}

#[cfg(test)]
mod tests;
