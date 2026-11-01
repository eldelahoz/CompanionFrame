//! Newer relic data over the bundled `@wfcd/items`, straight from npm and jsDelivr, trimmed here
//! the way WFHelper's Worker did. Rules port WFHelper's `config/shared/wfcdRelicRules.ts`. The app
//! runs on the bundled data alone when they are unreachable.

use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::db::Db;
use crate::services::cache;
use crate::services::net;

/// The full packument (about 4.5 MB): the abbreviated one has no per-version publish time.
const REGISTRY_URL: &str = "https://registry.npmjs.org/@wfcd%2Fitems";
/// A release must be this old, so a bad publish can be deprecated before the app takes it.
const MIN_RELEASE_AGE: chrono::Duration = chrono::Duration::hours(24);
const DATA_URL_BASE: &str = "https://cdn.jsdelivr.net/npm/@wfcd/items";
const MAX_REGISTRY_BYTES: usize = 16 * 1024 * 1024;
/// `Relics.json` of @wfcd/items 1.1276.6 is 8.8 MB raw.
const MAX_RAW_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const CACHE_KEY: &str = "wfcd-relics";
/// Releases found without usable relic data. npm releases never change, so this is kept for good.
const UNUSABLE_KEY: &str = "wfcd-relics-unusable";
const REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

// @wfcd/items 1.1276.6 has 3,196 rows that pass these rules.
const MIN_ROWS: usize = 2_500;
const MAX_ROWS: usize = 10_000;
const MAX_REWARDS: usize = 12;
const MAX_STRING: usize = 256;
// 1.1276.6: 608 distinct reward names, the longest 36 characters.
const MAX_REWARD_NAMES: usize = 2_000;
const MAX_REWARD_NAME_LENGTH: usize = 64;
pub const MAX_BYTES: usize = 20 * 1024 * 1024;

const RELIC_PATH_PREFIX: &str = "/Lotus/Types/Game/Projections/";
const RARITIES: [&str; 3] = ["Common", "Uncommon", "Rare"];

static RELIC_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(Lith|Meso|Neo|Axi|Requiem|Vanguard) \S+ (Intact|Exceptional|Flawless|Radiant)$")
        .unwrap()
});
static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,9})\.(\d{1,9})\.(\d{1,9})$").unwrap());

// ---- rules ----

fn bounded(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| s.chars().count() <= MAX_STRING)
}

/// Inventory rows and market orders take the refinement from the uniqueName suffix.
fn refinement_suffix(quality: &str) -> &'static str {
    match quality {
        "Intact" => "Bronze",
        "Exceptional" => "Silver",
        "Flawless" => "Gold",
        _ => "Platinum",
    }
}

/// A refinement row: a projection path whose suffix matches its `<Tier> <Code> <Quality>` name.
fn is_relic_identity(unique_name: &Value, name: &Value) -> bool {
    let (Some(u), Some(n)) = (bounded(unique_name), bounded(name)) else {
        return false;
    };
    u.starts_with(RELIC_PATH_PREFIX)
        && RELIC_NAME
            .captures(n)
            .is_some_and(|c| u.ends_with(refinement_suffix(&c[2])))
}

fn parse_version(v: &str) -> Option<[u64; 3]> {
    let c = VERSION.captures(v)?;
    Some([c[1].parse().ok()?, c[2].parse().ok()?, c[3].parse().ok()?])
}

pub fn is_version(v: &Value) -> bool {
    v.as_str().is_some_and(|s| parse_version(s).is_some())
}

/// Numeric `major.minor.patch` order; `Equal` when either side is not a plain version.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    match (parse_version(a), parse_version(b)) {
        (Some(l), Some(r)) => l.cmp(&r),
        _ => std::cmp::Ordering::Equal,
    }
}

/// A different major is refused, so a far-off version number cannot outrank every later release.
pub fn is_same_major(a: &str, b: &str) -> bool {
    matches!((parse_version(a), parse_version(b)), (Some(l), Some(r)) if l[0] == r[0])
}

pub fn is_newer(candidate: &str, baseline: &str) -> bool {
    compare_versions(candidate, baseline).is_gt()
}

// ---- validation ----

#[derive(Clone, Debug, PartialEq)]
pub struct RelicDoc {
    pub version: String,
    pub published_at: Option<String>,
    /// Trimmed `@wfcd/items` relic rows, the shape `relics::build` reads.
    pub relics: Vec<Value>,
}

fn read_reward(raw: &Value) -> Option<Value> {
    let chance = raw
        .get("chance")?
        .as_f64()
        .filter(|c| c.is_finite() && *c > 0.0 && *c <= 100.0)?;
    let rarity = raw
        .get("rarity")?
        .as_str()
        .filter(|r| RARITIES.contains(r))?;
    let item = raw.get("item")?.as_object()?;
    let name = item
        .get("name")?
        .as_str()
        .filter(|n| !n.is_empty() && n.chars().count() <= MAX_REWARD_NAME_LENGTH)?;
    let unique_name = bounded(item.get("uniqueName")?)?;
    let mut out = json!({"uniqueName": unique_name, "name": name});
    if let Some(v) = item.get("imageName") {
        out["imageName"] = json!(bounded(v)?);
    }
    if let Some(v) = item.get("ducats") {
        out["ducats"] = json!(v.as_f64().filter(|d| d.is_finite())?);
    }
    if let Some(market) = item.get("warframeMarket") {
        let market = market.as_object()?;
        let mut copy = Map::new();
        for key in ["id", "urlName"] {
            if let Some(v) = market.get(key) {
                copy.insert(key.into(), json!(bounded(v)?));
            }
        }
        out["warframeMarket"] = Value::Object(copy);
    }
    Some(json!({"chance": chance, "rarity": rarity, "item": out}))
}

fn read_relic_row(raw: &Value) -> Option<Value> {
    let row = raw.as_object()?;
    let (unique_name, name) = (row.get("uniqueName")?, row.get("name")?);
    if !is_relic_identity(unique_name, name) {
        return None;
    }
    let vaulted = row.get("vaulted")?.as_bool()?;
    let drop_count = row.get("dropCount")?.as_u64()?;
    let rewards = row.get("rewards")?.as_array()?;
    if rewards.is_empty() || rewards.len() > MAX_REWARDS {
        return None;
    }
    let rewards: Option<Vec<Value>> = rewards.iter().map(read_reward).collect();
    let mut out = json!({
        "uniqueName": unique_name, "name": name, "vaulted": vaulted,
        "dropCount": drop_count, "rewards": rewards?,
    });
    if let Some(v) = row.get("imageName") {
        out["imageName"] = json!(bounded(v)?);
    }
    Some(out)
}

/// Applies the Worker's rules: bad rows drop out, a bad version or row count rejects all.
pub fn validate_doc(raw: &Value) -> Result<RelicDoc, String> {
    let doc = raw.as_object().ok_or("not an object")?;
    if !doc.get("version").is_some_and(is_version) {
        return Err("bad version".into());
    }
    let list = doc
        .get("relics")
        .and_then(Value::as_array)
        .ok_or("no relic list")?;
    let relics: Vec<Value> = list.iter().filter_map(read_relic_row).collect();
    if !(MIN_ROWS..=MAX_ROWS).contains(&relics.len()) {
        return Err(format!("{} valid relics", relics.len()));
    }
    if too_many_reward_names(&relics) {
        return Err("too many reward names".into());
    }
    Ok(RelicDoc {
        version: doc["version"].as_str().unwrap_or_default().to_owned(),
        published_at: doc
            .get("publishedAt")
            .and_then(bounded)
            .filter(|t| chrono::DateTime::parse_from_rfc3339(t).is_ok())
            .map(str::to_owned),
        relics,
    })
}

fn too_many_reward_names(relics: &[Value]) -> bool {
    let mut names = std::collections::HashSet::new();
    for relic in relics {
        for reward in relic["rewards"].as_array().into_iter().flatten() {
            if let Some(n) = reward["item"]["name"].as_str() {
                names.insert(n);
            }
        }
        if names.len() > MAX_REWARD_NAMES {
            return true;
        }
    }
    false
}

// ---- update state machine ----

/// What the cache holds. Without a doc once the bundle caught up: the tag alone still earns a 304.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cached {
    pub etag: Option<String>,
    pub doc: Option<Value>,
}

pub enum Outcome {
    Body(Value, Option<String>),
    NotModified,
    /// The data was unusable: the update failed.
    Failed(String),
    /// npm or jsDelivr could not be reached: not a failure, the app just keeps what it has.
    Offline(String),
    /// No eligible release newer than the bundle has usable data: back to the bundled relics.
    Bundled,
}

#[derive(Debug, PartialEq)]
pub enum Swap {
    Apply(RelicDoc),
    /// Back to the bundled relics.
    Revert,
}

/// What the caller must do after one step; the step itself only moves the state.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    pub write_cache: Option<Cached>,
    pub swap: Option<Swap>,
}

#[derive(Default)]
pub struct Updater {
    etag: Option<String>,
    applied: Option<String>,
    /// Why the last check did not produce usable data; `None` after a good one.
    error: Option<String>,
    /// `error` is a network problem rather than bad data.
    offline: bool,
}

fn to_cached(etag: &Option<String>, doc: Option<&RelicDoc>) -> Cached {
    Cached {
        etag: etag.clone(),
        doc: doc.map(|d| {
            json!({
                "version": d.version, "publishedAt": d.published_at, "relics": d.relics,
            })
        }),
    }
}

/// `None` when the doc may be used; otherwise the reason it may not.
fn not_newer_reason(version: &str, bundled: Option<&str>) -> Option<String> {
    let Some(bundled) = bundled.filter(|b| parse_version(b).is_some()) else {
        return Some("bundled version unknown".into());
    };
    if !is_same_major(version, bundled) {
        return Some(format!("major differs from bundled {bundled}"));
    }
    (!is_newer(version, bundled)).then(|| format!("not newer than bundled {bundled}"))
}

impl Updater {
    fn revert(&mut self) -> bool {
        self.applied.take().is_some()
    }

    fn apply(&mut self, doc: RelicDoc) -> Swap {
        self.applied = Some(doc.version.clone());
        Swap::Apply(doc)
    }

    pub fn applied_version(&self) -> Option<&str> {
        self.applied.as_deref()
    }

    pub fn etag(&self) -> Option<&str> {
        self.etag.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn offline(&self) -> bool {
        self.offline
    }

    /// Start of a run: the stored copy, if it is still newer than the bundle.
    pub fn apply_cache(&mut self, cached: Option<Cached>, bundled: Option<&str>) -> Step {
        let Some(cached) = cached else {
            return Step::default();
        };
        self.etag = cached.etag;
        let Some(Ok(doc)) = cached.doc.as_ref().map(validate_doc) else {
            return Step::default();
        };
        if let Some(reason) = not_newer_reason(&doc.version, bundled) {
            tracing::info!("[RelicData] cached {} unused: {reason}", doc.version);
            return Step {
                write_cache: Some(to_cached(&self.etag, None)),
                swap: None,
            };
        }
        Step {
            swap: Some(self.apply(doc)),
            ..Step::default()
        }
    }

    pub fn handle(&mut self, outcome: Outcome, bundled: Option<&str>) -> Step {
        self.error = None;
        self.offline = false;
        match outcome {
            Outcome::NotModified => Step::default(),
            Outcome::Offline(reason) => {
                tracing::info!("[RelicData] offline: {reason}");
                self.error = Some(reason);
                self.offline = true;
                Step::default()
            }
            Outcome::Failed(reason) => {
                tracing::warn!("[RelicData] update failed: {reason}");
                self.error = Some(reason);
                Step::default()
            }
            Outcome::Body(body, tag) => self.handle_body(&body, tag, bundled),
            Outcome::Bundled => {
                let reverted = self.revert();
                let had_tag = self.etag.take().is_some();
                Step {
                    write_cache: (reverted || had_tag).then(|| to_cached(&None, None)),
                    swap: reverted.then_some(Swap::Revert),
                }
            }
        }
    }

    fn handle_body(&mut self, body: &Value, tag: Option<String>, bundled: Option<&str>) -> Step {
        let doc = match validate_doc(body) {
            Ok(d) => d,
            Err(reason) => {
                tracing::warn!("[RelicData] download invalid: {reason}");
                self.error = Some(format!("download invalid: {reason}"));
                return Step::default();
            }
        };
        if self.applied.as_deref() == Some(doc.version.as_str()) {
            let write = (tag != self.etag).then(|| {
                self.etag = tag;
                to_cached(&self.etag, Some(&doc))
            });
            return Step {
                write_cache: write,
                swap: None,
            };
        }
        self.etag = tag;
        if let Some(reason) = not_newer_reason(&doc.version, bundled) {
            tracing::info!("[RelicData] downloaded {} unused: {reason}", doc.version);
            let reverted = self.revert();
            return Step {
                write_cache: (self.etag.is_some() || reverted).then(|| to_cached(&self.etag, None)),
                swap: reverted.then_some(Swap::Revert),
            };
        }
        let write = to_cached(&self.etag, Some(&doc));
        Step {
            write_cache: Some(write),
            swap: Some(self.apply(doc)),
        }
    }
}

// ---- IO ----

pub fn read_cache(db: &Db) -> Option<Cached> {
    let text = cache::kv_get(db, CACHE_KEY).ok()??;
    if text.len() > MAX_BYTES {
        return None;
    }
    let cached: Cached = serde_json::from_str(&text).ok()?;
    let etag_ok = cached
        .etag
        .as_deref()
        .is_none_or(|t| t.chars().count() <= MAX_STRING);
    // A doc that no longer validates is dropped, but a bare tag survives.
    match &cached.doc {
        Some(d) if validate_doc(d).is_err() => None,
        _ if !etag_ok => None,
        _ => Some(cached),
    }
}

pub fn write_cache(db: &Db, cached: &Cached) {
    match serde_json::to_string(cached) {
        Ok(text) => {
            if let Err(e) = cache::kv_put(db, CACHE_KEY, &text) {
                tracing::warn!("[RelicData] could not save the downloaded copy: {e}");
            }
        }
        Err(e) => tracing::warn!("[RelicData] could not save the downloaded copy: {e}"),
    }
}

/// Trims one raw `Relics.json` row to the shape `read_relic_row` validates; `None` drops the row.
fn trim_relic(raw: &Value) -> Option<Value> {
    let row = raw.as_object()?;
    let rewards: Option<Vec<Value>> = row
        .get("rewards")?
        .as_array()?
        .iter()
        .map(trim_reward)
        .collect();
    let mut out = json!({
        "uniqueName": row.get("uniqueName")?, "name": row.get("name")?,
        "vaulted": row.get("vaulted").and_then(Value::as_bool).unwrap_or(false),
        "dropCount": row.get("drops").and_then(Value::as_array).map_or(0, Vec::len),
        "rewards": rewards?,
    });
    copy_nonempty_str(&mut out, row, "imageName");
    Some(out)
}

fn trim_reward(raw: &Value) -> Option<Value> {
    let item = raw.get("item")?.as_object()?;
    let mut out = json!({
        "uniqueName": item.get("uniqueName")?, "name": item.get("name")?,
    });
    copy_nonempty_str(&mut out, item, "imageName");
    if let Some(d) = item.get("ducats").filter(|d| d.as_f64().is_some()) {
        out["ducats"] = d.clone();
    }
    if let Some(m) = item.get("warframeMarket").and_then(Value::as_object) {
        let mut market = json!({});
        copy_nonempty_str(&mut market, m, "id");
        copy_nonempty_str(&mut market, m, "urlName");
        if market.get("urlName").is_none()
            && let Some(u) = m.get("url_name")
        {
            market["urlName"] = u.clone();
        }
        out["warframeMarket"] = market;
    }
    Some(json!({"chance": raw.get("chance")?, "rarity": raw.get("rarity")?, "item": out}))
}

fn copy_nonempty_str(out: &mut Value, from: &Map<String, Value>, key: &str) {
    if let Some(v) = from
        .get(key)
        .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
    {
        out[key] = v.clone();
    }
}

/// Plain x.y.z releases that are old enough, not deprecated and not above `dist-tags.latest`,
/// newest first.
fn pick_releases(packument: &Value, now: chrono::DateTime<chrono::Utc>) -> Vec<String> {
    let (Some(versions), Some(times)) = (
        packument.get("versions").and_then(Value::as_object),
        packument.get("time").and_then(Value::as_object),
    ) else {
        return Vec::new();
    };
    let latest = packument["dist-tags"]["latest"].as_str();
    let mut found: Vec<String> = times
        .iter()
        .filter(|(v, _)| parse_version(v).is_some_and(|_| versions.contains_key(*v)))
        .filter(|(v, _)| {
            versions[*v]
                .get("deprecated")
                .is_none_or(|d| d.is_null() || d == false)
        })
        .filter(|(v, _)| latest.is_none_or(|l| !is_newer(v, l)))
        .filter(|(_, t)| {
            t.as_str()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .is_some_and(|t| now - t.to_utc() >= MIN_RELEASE_AGE)
        })
        .map(|(v, _)| v.clone())
        .collect();
    found.sort_by(|a, b| compare_versions(b, a));
    found
}

/// Raw `Relics.json` bytes to trimmed rows; unparseable input gives none.
fn trim_table(body: &[u8]) -> Vec<Value> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|v| {
            v.as_array()
                .map(|a| a.iter().filter_map(trim_relic).collect())
        })
        .unwrap_or_default()
}

/// Downloads per run at most: @wfcd/items 1.1276.30 onward shipped `Relics.json` with every
/// reward list empty, so a fresh install walks back about a dozen releases to the last good one.
const MAX_TRIES: usize = 24;

/// Newest first, down to the bundled version: the first usable release wins. The one in use
/// (`known`) ends the walk without a download; `bad` releases are skipped and grow as found.
async fn walk<F, Fut>(
    releases: &[String],
    known: Option<&str>,
    bundled: Option<&str>,
    bad: &mut std::collections::HashSet<String>,
    mut fetch: F,
) -> Outcome
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, String>>,
{
    let Some(bundled) = bundled.filter(|b| parse_version(b).is_some()) else {
        return Outcome::Bundled;
    };
    let mut tried = 0;
    for version in releases.iter().take_while(|v| is_newer(v, bundled)) {
        if bad.contains(version) {
            continue;
        }
        if known == Some(version.as_str()) {
            return Outcome::NotModified;
        }
        if tried == MAX_TRIES {
            return Outcome::Failed(format!(
                "{MAX_TRIES} releases had no usable relic data; the next check goes further back"
            ));
        }
        tried += 1;
        let body = match fetch(version.clone()).await {
            Ok(b) => b,
            Err(e) => return Outcome::Offline(e),
        };
        let doc = json!({"version": version, "relics": trim_table(&body)});
        match validate_doc(&doc) {
            Ok(_) => return Outcome::Body(doc, Some(version.clone())),
            Err(reason) => {
                tracing::info!("[RelicData] {version} unusable: {reason}");
                bad.insert(version.clone());
            }
        }
    }
    Outcome::Bundled
}

/// The release in use is the "tag": a repeat of it costs the registry document, not 3 MB.
async fn request(
    client: &reqwest::Client,
    known: Option<&str>,
    bundled: Option<&str>,
    bad: &mut std::collections::HashSet<String>,
) -> Outcome {
    let packument = match net::fetch(client, REGISTRY_URL, None, MAX_REGISTRY_BYTES).await {
        Ok(net::Fetched::Modified { body, .. }) => body,
        Ok(net::Fetched::NotModified) => return Outcome::NotModified,
        Err(e) => return Outcome::Offline(e),
    };
    let releases = serde_json::from_slice::<Value>(&packument)
        .map(|v| pick_releases(&v, chrono::Utc::now()))
        .unwrap_or_default();
    if releases.is_empty() {
        return Outcome::Failed("npm lists no eligible release".into());
    }
    walk(&releases, known, bundled, bad, |version| async move {
        let url = format!("{DATA_URL_BASE}@{version}/data/json/Relics.json");
        match net::fetch(client, &url, None, MAX_RAW_BYTES).await? {
            net::Fetched::Modified { body, .. } => Ok(body),
            net::Fetched::NotModified => Err("unexpected 304 from jsDelivr".into()),
        }
    })
    .await
}

fn read_unusable(db: &Db) -> std::collections::HashSet<String> {
    cache::kv_get(db, UNUSABLE_KEY)
        .ok()
        .flatten()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .into_iter()
        .flatten()
        .filter(|v| parse_version(v).is_some())
        .collect()
}

fn write_unusable(db: &Db, bad: &std::collections::HashSet<String>) {
    let mut list: Vec<&String> = bad.iter().collect();
    list.sort_by(|a, b| compare_versions(a, b));
    if let Err(e) = cache::kv_put(db, UNUSABLE_KEY, &json!(list).to_string()) {
        tracing::warn!("[RelicData] could not save the unusable releases: {e}");
    }
}

/// True when a release newer than the data in use was skipped for having no relic data.
pub fn newer_unusable(bad: &std::collections::HashSet<String>, in_use: Option<&str>) -> bool {
    in_use.is_some_and(|u| bad.iter().any(|v| is_newer(v, u)))
}

/// Applies a cached copy, then checks npm now and every six hours. `swapped` runs after
/// each change of relic data source: newer data or a revert to the bundled relics.
pub fn start(app: &tauri::AppHandle) {
    use crate::services::bundled_data::BundledData;
    use crate::services::relics::RelicState;
    use tauri::{Emitter, Manager};

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut updater = Updater::default();
        let db = handle.state::<Db>();
        let state = handle.state::<RelicState>();
        let bundled = BundledData::from_app(&handle)
            .ok()
            .and_then(|b| b.wfcd_version());
        let mut bad = read_unusable(&db);
        let run = |step: Step| {
            if let Some(c) = &step.write_cache {
                write_cache(&db, c);
            }
            if let Some(swap) = step.swap {
                state.set_downloaded(match swap {
                    Swap::Apply(d) => {
                        tracing::info!(
                            "[RelicData] update succeeded: {} ({} relics)",
                            d.version,
                            d.relics.len()
                        );
                        Some(d)
                    }
                    Swap::Revert => None,
                });
                let _ = handle.emit("relic-db-updated", ());
            }
        };
        run(updater.apply_cache(read_cache(&db), bundled.as_deref()));
        let Ok(client) = net::https_client(true) else {
            return;
        };
        let mut tick = tokio::time::interval(REFRESH_INTERVAL);
        loop {
            tick.tick().await;
            let known_bad = bad.len();
            let outcome = request(&client, updater.etag(), bundled.as_deref(), &mut bad).await;
            if bad.len() != known_bad {
                write_unusable(&db, &bad);
            }
            run(updater.handle(outcome, bundled.as_deref()));
            let in_use = updater.applied_version().or(bundled.as_deref());
            if state.set_update_error(
                updater.error(),
                updater.offline(),
                newer_unusable(&bad, in_use),
            ) {
                let _ = handle.emit("relic-db-updated", ());
            }
        }
    });
}

#[cfg(test)]
mod tests;
