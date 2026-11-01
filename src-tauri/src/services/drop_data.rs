//! WFCD drop tables flattened into searchable rows. Port of WFHelper's `services/dropData.ts`.
//! The flattened set is cached in `kv_cache`; search runs over the in-memory copy.

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;
use crate::services::cache;
use crate::services::item_db::corrected_drop_rarity;
use crate::services::net;

const INFO_URL: &str = "https://drops.warframestat.us/data/info.json";
const ALL_URL: &str = "https://drops.warframestat.us/data/all.json";
const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const CACHE_KEY: &str = "drop-data";
/// Bumped when a row gains a field: an older cache has no `kind`.
const CACHE_VERSION: u32 = 2;
const DOJO_RESEARCH: &str = include_str!("../../data/dojo_research.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DropKind {
    Enemy,
    Mission,
    Bounty,
    Relic,
    Sortie,
    Quest,
    Syndicate,
    Dojo,
    Other,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DropRow {
    pub item: String,
    /// Where it drops (e.g. "Arbitrations, Rotation C").
    pub place: String,
    pub rarity: String,
    pub chance: Option<f64>,
    pub kind: DropKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DropSearchMode {
    #[default]
    Item,
    Place,
    Enemy,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct DropSearchResult {
    pub rows: Vec<DropRow>,
    pub total: u32,
}

#[derive(Serialize, Deserialize)]
struct CachePayload {
    version: u32,
    hash: String,
    rows: Vec<DropRow>,
}

// ---- flattening ----

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn reward_name(r: &Value) -> Option<&str> {
    str_of(r, "itemName")
        .or_else(|| str_of(r, "item"))
        .or_else(|| str_of(r, "modName"))
}

fn push_row(out: &mut Vec<DropRow>, item: Option<&str>, place: &str, r: &Value, kind: DropKind) {
    let Some(item) = item.filter(|i| !i.is_empty()) else {
        return;
    };
    if place.is_empty() {
        return;
    }
    // Chance may arrive as a number or a numeric string; anything else counts as 0.
    let chance = r
        .get("chance")
        .and_then(|c| c.as_f64().or_else(|| c.as_str()?.trim().parse().ok()))
        .filter(|c| c.is_finite())
        .unwrap_or(0.0);
    out.push(DropRow {
        item: item.to_owned(),
        place: place.to_owned(),
        rarity: corrected_drop_rarity(place, chance, str_of(r, "rarity").unwrap_or("")),
        chance: Some(chance),
        kind,
        source_url: None,
    });
}

/// Rewards may be a flat array or a `{rotation: reward[]}` map; emit either way.
fn push_container(out: &mut Vec<DropRow>, base: &str, rewards: &Value, kind: DropKind) {
    let mut emit = |place: String, list: &[Value]| {
        for r in list {
            let mut p = place.clone();
            if let Some(rot) = str_of(r, "rotation") {
                p += &format!(", Rotation {rot}");
            }
            if let Some(stage) = str_of(r, "stage") {
                p += &format!(" ({stage})");
            }
            push_row(out, reward_name(r), &p, r, kind);
        }
    };
    match rewards {
        Value::Array(list) => emit(base.to_owned(), list),
        Value::Object(map) => {
            for (rotation, list) in map {
                if let Value::Array(list) = list {
                    emit(format!("{base}, Rotation {rotation}"), list);
                }
            }
        }
        _ => {}
    }
}

fn list<'a>(data: &'a Value, key: &str) -> &'a [Value] {
    data.get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

const BOUNTY_KEYS: [&str; 6] = [
    "cetusBountyRewards",
    "solarisBountyRewards",
    "deimosRewards",
    "zarimanRewards",
    "entratiLabRewards",
    "hexRewards",
];

pub fn flatten(data: &Value) -> Vec<DropRow> {
    let mut out = Vec::new();
    let empty = Value::Array(vec![]);

    if let Some(Value::Object(planets)) = data.get("missionRewards") {
        for (planet, nodes) in planets {
            let Value::Object(nodes) = nodes else {
                continue;
            };
            for (node, info) in nodes {
                let rewards = info.get("rewards").unwrap_or(&empty);
                push_container(
                    &mut out,
                    &format!("{node} ({planet})"),
                    rewards,
                    DropKind::Mission,
                );
            }
        }
    }
    for relic in list(data, "relics") {
        // Intact only: the other refinements repeat the same rows.
        if str_of(relic, "state").is_some_and(|s| s != "Intact") {
            continue;
        }
        let place = format!(
            "{} {} Relic",
            str_of(relic, "tier").unwrap_or("undefined"),
            str_of(relic, "relicName").unwrap_or("undefined")
        );
        push_container(
            &mut out,
            &place,
            relic.get("rewards").unwrap_or(&empty),
            DropKind::Relic,
        );
    }
    for t in list(data, "transientRewards") {
        let place = str_of(t, "objectiveName").unwrap_or("Mission");
        push_container(
            &mut out,
            place,
            t.get("rewards").unwrap_or(&empty),
            DropKind::Mission,
        );
    }
    for r in list(data, "sortieRewards") {
        push_row(&mut out, reward_name(r), "Sortie", r, DropKind::Sortie);
    }
    for k in list(data, "keyRewards") {
        let place = str_of(k, "keyName").unwrap_or("Quest");
        push_container(
            &mut out,
            place,
            k.get("rewards").unwrap_or(&empty),
            DropKind::Quest,
        );
    }
    for key in BOUNTY_KEYS {
        for b in list(data, key) {
            let place = str_of(b, "bountyLevel").unwrap_or("Bounty");
            push_container(
                &mut out,
                place,
                b.get("rewards").unwrap_or(&empty),
                DropKind::Bounty,
            );
        }
    }

    // item -> enemies
    for (table, name_key, rows_key) in [
        ("modLocations", "modName", "enemies"),
        ("blueprintLocations", "itemName", "enemies"),
    ] {
        for m in list(data, table) {
            for e in m
                .get(rows_key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                push_row(
                    &mut out,
                    str_of(m, name_key),
                    str_of(e, "enemyName").unwrap_or(""),
                    e,
                    DropKind::Enemy,
                );
            }
        }
    }
    // enemy -> items; the "byAvatar" tables are keyed by enemy too.
    for (table, who_key, rows_key) in [
        ("enemyModTables", "enemyName", "mods"),
        ("enemyBlueprintTables", "enemyName", "items"),
        ("resourceByAvatar", "source", "items"),
        ("sigilByAvatar", "source", "items"),
        ("additionalItemByAvatar", "source", "items"),
    ] {
        for e in list(data, table) {
            let who = str_of(e, who_key).unwrap_or("");
            for it in e
                .get(rows_key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                push_row(&mut out, reward_name(it), who, it, DropKind::Enemy);
            }
        }
    }
    // syndicates carry their own place
    if let Some(Value::Object(syndicates)) = data.get("syndicates") {
        for rewards in syndicates.values().filter_map(Value::as_array) {
            for r in rewards {
                let place = str_of(r, "place").unwrap_or("Syndicate");
                push_row(&mut out, reward_name(r), place, r, DropKind::Syndicate);
            }
        }
    }

    // Upstream repeats reward entries; collapse identical rows. An enemy row wins a tie
    // so the name stays clickable when a table lists it twice.
    let mut by_key: HashMap<String, usize> = HashMap::new();
    let mut deduped: Vec<DropRow> = Vec::new();
    for row in out {
        let key = format!("{}|{}|{}|{:?}", row.item, row.place, row.rarity, row.chance);
        match by_key.get(&key) {
            None => {
                by_key.insert(key, deduped.len());
                deduped.push(row);
            }
            Some(&i) if deduped[i].kind != DropKind::Enemy && row.kind == DropKind::Enemy => {
                deduped[i] = row;
            }
            _ => {}
        }
    }
    deduped
}

// ---- bundled supplementary rows ----

/// Quest completion rewards are absent from the random drop tables.
fn acquisition_rows() -> Vec<DropRow> {
    vec![DropRow {
        item: "Helminth Archon Shard Segment Blueprint".into(),
        place: "Veilbreaker".into(),
        rarity: String::new(),
        chance: None,
        kind: DropKind::Quest,
        source_url: Some("https://www.warframe.com/en/patch-notes/pc/35-0-0".into()),
    }]
}

/// Dojo research is absent upstream; the table is built from the wiki at dev time.
fn dojo_rows(text: &str) -> Vec<DropRow> {
    let Ok(Value::Object(doc)) = serde_json::from_str::<Value>(text) else {
        tracing::warn!("dojo research table unreadable");
        return vec![];
    };
    let Some(Value::Array(entries)) = doc.get("entries") else {
        tracing::warn!("dojo research table has no entries array");
        return vec![];
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in entries {
        let (Some(item), Some(lab)) = (
            e.get("item").and_then(Value::as_str).map(str::trim),
            e.get("lab").and_then(Value::as_str).map(str::trim),
        ) else {
            continue;
        };
        if item.is_empty() || lab.is_empty() || !seen.insert((item.to_owned(), lab.to_owned())) {
            continue;
        }
        // Guaranteed once the research is done, like a syndicate offering.
        out.push(DropRow {
            item: item.into(),
            place: lab.into(),
            rarity: "Common".into(),
            chance: Some(100.0),
            kind: DropKind::Dojo,
            source_url: None,
        });
    }
    out
}

/// Upstream rows plus the bundled ones; rebuilt from upstream each time so nothing doubles up.
fn served_rows(upstream: Vec<DropRow>, dojo_text: &str) -> Vec<DropRow> {
    let extra: Vec<DropRow> = acquisition_rows()
        .into_iter()
        .filter(|s| {
            !upstream
                .iter()
                .any(|r| r.item == s.item && r.place == s.place)
        })
        .collect();
    [upstream, dojo_rows(dojo_text), extra].concat()
}

// ---- search ----

/// A dojo row's place is the lab alone, but "dojo" is what a user types; the word rides
/// along in the search field so the column stays clean.
fn place_field(row: &DropRow) -> String {
    if row.kind == DropKind::Dojo {
        format!("{} Dojo", row.place)
    } else {
        row.place.clone()
    }
}

/// Substring search by item (default), place or enemy, ranked: prefix > word-start > contains.
pub fn search(
    rows: &[DropRow],
    query: &str,
    mode: DropSearchMode,
    limit: usize,
) -> DropSearchResult {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return DropSearchResult {
            rows: vec![],
            total: 0,
        };
    }
    let mut scored: Vec<(u8, &DropRow)> = Vec::new();
    for row in rows {
        // "enemy" is the place search narrowed to the enemy tables.
        if mode == DropSearchMode::Enemy && row.kind != DropKind::Enemy {
            continue;
        }
        let field = if mode == DropSearchMode::Item {
            row.item.to_lowercase()
        } else {
            place_field(row).to_lowercase()
        };
        let Some(idx) = field.find(&q) else {
            continue;
        };
        let score = match field[..idx].chars().next_back() {
            None => 0,
            Some(c) if c.is_whitespace() => 1,
            Some(_) => 2,
        };
        scored.push((score, row));
    }
    let chance = |r: &DropRow| r.chance.unwrap_or(0.0);
    scored.sort_by(|(sa, a), (sb, b)| {
        sa.cmp(sb)
            .then(chance(b).total_cmp(&chance(a)))
            .then_with(|| a.item.cmp(&b.item))
            .then_with(|| a.place.cmp(&b.place))
    });
    DropSearchResult {
        total: scored.len() as u32,
        rows: scored
            .into_iter()
            .take(limit)
            .map(|(_, r)| r.clone())
            .collect(),
    }
}

/// Rows for one item under its exact name or as a stack ("2X Orokin Cell"), best chance first.
pub fn for_item(rows: &[DropRow], name: &str) -> Vec<DropRow> {
    let target = name.trim().to_lowercase();
    if target.is_empty() {
        return vec![];
    }
    let mut hits: Vec<DropRow> = rows
        .iter()
        .filter(|r| strip_stack(&r.item.to_lowercase()) == target)
        .cloned()
        .collect();
    hits.sort_by(|a, b| {
        b.chance
            .unwrap_or(0.0)
            .total_cmp(&a.chance.unwrap_or(0.0))
            .then_with(|| a.place.cmp(&b.place))
    });
    hits
}

fn strip_stack(s: &str) -> &str {
    let digits = s.bytes().take_while(u8::is_ascii_digit).count();
    s[digits..]
        .strip_prefix("x ")
        .filter(|_| digits > 0)
        .unwrap_or(s)
}

// ---- store ----

#[derive(Default)]
pub struct DropStore {
    rows: RwLock<Vec<DropRow>>,
    hash: RwLock<Option<String>>,
}

impl DropStore {
    fn set(&self, upstream: Vec<DropRow>, hash: &str) {
        *self.rows.write().unwrap() = served_rows(upstream, DOJO_RESEARCH);
        *self.hash.write().unwrap() = Some(hash.to_owned());
    }

    /// Restores the cached flatten; false when there is none (or it is from an older layout).
    pub fn load_cached(&self, db: &Db) -> bool {
        let cached = cache::kv_get(db, CACHE_KEY)
            .ok()
            .flatten()
            .and_then(|t| serde_json::from_str::<CachePayload>(&t).ok())
            .filter(|c| c.version == CACHE_VERSION);
        let Some(c) = cached else {
            return false;
        };
        self.set(c.rows, &c.hash);
        true
    }

    /// Fetches only when upstream's hash changed; true when new rows were served.
    pub async fn refresh(&self, db: &Db, client: &reqwest::Client) -> bool {
        match self.try_refresh(db, client).await {
            Ok(changed) => changed,
            Err(e) => {
                tracing::warn!("drop data refresh failed: {e}");
                false
            }
        }
    }

    async fn try_refresh(&self, db: &Db, client: &reqwest::Client) -> Result<bool, String> {
        let get = |url: &'static str| async move {
            match net::fetch(client, url, None, MAX_BYTES).await? {
                net::Fetched::Modified { body, .. } => {
                    serde_json::from_slice::<Value>(&body).map_err(|e| e.to_string())
                }
                net::Fetched::NotModified => Err(format!("{url}: unexpected 304")),
            }
        };
        let info = get(INFO_URL).await?;
        let hash = str_of(&info, "hash").unwrap_or("").to_owned();
        if !hash.is_empty() && self.hash.read().unwrap().as_deref() == Some(&hash) {
            return Ok(false);
        }
        let rows = flatten(&get(ALL_URL).await?);
        let payload = CachePayload {
            version: CACHE_VERSION,
            hash: hash.clone(),
            rows,
        };
        if let Ok(text) = serde_json::to_string(&payload) {
            let _ = cache::kv_put(db, CACHE_KEY, &text);
        }
        self.set(payload.rows, &hash);
        tracing::info!(
            "drop data refreshed (hash {})",
            hash.get(..8).unwrap_or(&hash)
        );
        Ok(true)
    }

    fn with_rows<T>(&self, f: impl FnOnce(&[DropRow]) -> T) -> T {
        f(&self.rows.read().unwrap())
    }
}

#[tauri::command]
#[specta::specta]
pub fn drop_search(
    query: String,
    mode: Option<DropSearchMode>,
    limit: Option<u32>,
    store: tauri::State<DropStore>,
) -> DropSearchResult {
    store.with_rows(|rows| {
        search(
            rows,
            &query,
            mode.unwrap_or_default(),
            limit.map_or(300, |l| l as usize),
        )
    })
}

#[tauri::command]
#[specta::specta]
pub fn drop_item_sources(name: String, store: tauri::State<DropStore>) -> Vec<DropRow> {
    store.with_rows(|rows| for_item(rows, &name))
}

/// Serves the cached rows right away, then refreshes from upstream in the background.
pub fn setup(app: &tauri::App) {
    use tauri::Manager;
    app.manage(DropStore::default());
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let (store, db) = (handle.state::<DropStore>(), handle.state::<Db>());
        if !store.load_cached(&db) {
            store.set(vec![], "");
        }
        if let Ok(client) = net::client() {
            store.refresh(&db, &client).await;
        }
    });
}

#[cfg(test)]
mod tests;
