//! Codex scan counts from the player's public profile (`getProfileViewingData.php`).
//! The account id is a public 24-hex id the user pastes (or a profile link containing it). In
//! game-memory mode it is also kept from a working helper fetch (the nonce never leaves
//! `api_helper`). Refreshed shortly after launch, every few hours, and on demand.

use crate::services::clock::now_ms;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::db::Db;
use crate::services::atomic::write_atomic;
use crate::services::cache::{kv_get, kv_put};
use crate::services::net;

pub mod personal;
#[cfg(test)]
mod tests;

const ACCOUNT_FILE: &str = "codex-profile.json";
const PROFILE_URL: &str = "https://api.warframe.com/cdn/getProfileViewingData.php?playerId=";
/// Floor between manual refreshes; it just absorbs double-clicks.
const REFRESH_MIN_MS: i64 = 60_000;
const MAX_PROFILE_BYTES: usize = 40_000_000;
const MAX_SCANS: usize = 10_000;

/// (account, attempt time, failed) of the last manual refresh.
static ATTEMPT: Mutex<Option<(String, i64, bool)>> = Mutex::new(None);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct CodexScanEntry {
    /// Enemy type path as the profile reports it.
    #[serde(rename = "type")]
    pub kind: String,
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CodexScansResult {
    pub fetched_at: Option<f64>,
    pub scans: Option<Vec<CodexScanEntry>>,
    /// `no-account`, `fetch-failed` or `no-data`.
    pub error: Option<String>,
    pub next_refresh_at: f64,
}

pub fn valid_account_id(s: &str) -> bool {
    s.len() == 24 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The first 24-hex run in what the user pasted: a bare id, or a profile link containing it.
pub fn account_id_from_input(input: &str) -> Option<String> {
    let lower = input.trim().to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|w| valid_account_id(w))
        .map(str::to_owned)
}

/// The account id inside `?accountId=<24 hex>&nonce=..`.
pub fn account_id_from_authz(authz: &str) -> Option<&str> {
    let rest = authz
        .split(['?', '&'])
        .find_map(|p| p.strip_prefix("accountId="))?;
    valid_account_id(rest).then_some(rest)
}

/// Remember the account behind a working inventory fetch (memory mode; `data` = app data dir).
pub fn note_authz(data: &Path, authz: &str) {
    let Some(id) = account_id_from_authz(authz) else {
        return;
    };
    if load_account(data).as_deref() != Some(id) && save_account(data, id).is_ok() {
        tracing::info!("codex: account id captured for profile fetches");
    }
}

/// Stores the account id for profile fetches (`data` = app data dir).
pub fn save_account(data: &Path, id: &str) -> std::io::Result<()> {
    let body = serde_json::json!({ "accountId": id }).to_string();
    write_atomic(&data.join(ACCOUNT_FILE), body.as_bytes())
}

pub fn load_account(data: &Path) -> Option<String> {
    let raw: Value = serde_json::from_slice(&std::fs::read(data.join(ACCOUNT_FILE)).ok()?).ok()?;
    let id = raw.get("accountId")?.as_str()?;
    valid_account_id(id).then(|| id.to_owned())
}

/// The scans array moved between `Stats` and `Results[0].Stats` historically; duplicates keep the max.
pub fn parse_profile_scans(payload: &Value) -> Option<Vec<CodexScanEntry>> {
    let raw = payload
        .pointer("/Stats/Scans")
        .or_else(|| payload.pointer("/Results/0/Stats/Scans"))?
        .as_array()?;
    let mut out: BTreeMap<String, u32> = BTreeMap::new();
    for e in raw.iter().take(MAX_SCANS) {
        let (Some(t), Some(n)) = (
            e.get("type").and_then(Value::as_str),
            e.get("scans").and_then(Value::as_u64),
        ) else {
            continue;
        };
        if t.is_empty() || t.len() > 512 || n > u64::from(u32::MAX) {
            continue;
        }
        let slot = out.entry(t.to_owned()).or_insert(0);
        *slot = (*slot).max(n as u32);
    }
    Some(
        out.into_iter()
            .map(|(kind, count)| CodexScanEntry { kind, count })
            .collect(),
    )
}

fn at_key(account: &str) -> String {
    format!("codex_scans_at:{account}")
}

type Cached = (i64, Vec<CodexScanEntry>);

fn read_cache(db: &Db, account: &str) -> Option<Cached> {
    let at: i64 = kv_get(db, &at_key(account)).ok()??.parse().ok()?;
    let conn = db.lock();
    let mut stmt = conn
        .prepare("SELECT entry, scans FROM codex_scans WHERE account = ?1 ORDER BY entry")
        .ok()?;
    let scans = stmt
        .query_map([account], |r| {
            Ok(CodexScanEntry {
                kind: r.get(0)?,
                count: r.get(1)?,
            })
        })
        .ok()?
        .flatten()
        .collect();
    Some((at, scans))
}

fn write_cache(db: &Db, account: &str, at: i64, scans: &[CodexScanEntry]) -> rusqlite::Result<()> {
    {
        let mut conn = db.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM codex_scans WHERE account = ?1", [account])?;
        {
            let mut ins =
                tx.prepare("INSERT INTO codex_scans (account, entry, scans) VALUES (?1, ?2, ?3)")?;
            for s in scans {
                ins.execute(rusqlite::params![account, s.kind, s.count])?;
            }
        }
        tx.commit()?;
    }
    kv_put(db, &at_key(account), &at.to_string())
}

fn result(account: &str, cached: Option<&Cached>) -> CodexScansResult {
    let attempt = ATTEMPT
        .lock()
        .unwrap()
        .clone()
        .filter(|(a, _, _)| a == account);
    let failed = attempt
        .as_ref()
        .is_some_and(|(_, at, f)| *f && cached.is_none_or(|(c, _)| at >= c));
    let next = cached
        .map_or(0, |(c, _)| c + REFRESH_MIN_MS)
        .max(attempt.map_or(0, |(_, at, _)| at + REFRESH_MIN_MS));
    CodexScansResult {
        fetched_at: cached.map(|(c, _)| *c as f64),
        scans: cached.map(|(_, s)| s.clone()),
        error: if failed {
            Some("fetch-failed".into())
        } else if cached.is_none() {
            Some("no-data".into())
        } else {
            None
        },
        next_refresh_at: next as f64,
    }
}

async fn fetch_profile_json(account: &str) -> Result<Value, String> {
    fetch_profile_if_changed(account, None)
        .await?
        .map(|(json, _)| json)
        .ok_or_else(|| "unexpected 304".into())
}

/// `None` when the server answers 304 to `etag`. Errors never carry the URL (it holds the id).
async fn fetch_profile_if_changed(
    account: &str,
    etag: Option<&str>,
) -> Result<Option<(Value, Option<String>)>, String> {
    let client = net::https_client(true).map_err(|e| e.to_string())?;
    let url = format!("{PROFILE_URL}{account}");
    let fetched = net::fetch(&client, &url, etag, MAX_PROFILE_BYTES)
        .await
        .map_err(|e| e.replace(&url, "profile").replace(account, "<id>"))?;
    match fetched {
        net::Fetched::Modified { body, etag } => serde_json::from_slice(&body)
            .map(|json| Some((json, etag)))
            .map_err(|_| "profile response is not valid JSON".into()),
        net::Fetched::NotModified => Ok(None),
    }
}

/// Keys of the player block that the mastery calculation reads, besides `LoadOutInventory`.
const PROFILE_INVENTORY_KEYS: [&str; 4] = ["XPInfo", "PlayerLevel", "Missions", "PlayerSkills"];

/// The inventory-shaped part of a public profile: equipped gear and XP per item (`XPInfo`),
/// mastery rank and node completion. No resources, blueprints or counts. `None` without XPInfo.
pub fn parse_profile_inventory(payload: &Value) -> Option<Value> {
    let player = payload.pointer("/Results/0")?.as_object()?;
    let mut inv = player
        .get("LoadOutInventory")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in PROFILE_INVENTORY_KEYS {
        if let (false, Some(v)) = (inv.contains_key(key), player.get(key)) {
            inv.insert(key.to_owned(), v.clone());
        }
    }
    inv.get("XPInfo")?.as_array()?;
    Some(Value::Object(inv))
}

fn inventory_key(account: &str) -> String {
    format!("profile_inventory:{account}")
}

/// Caches the profile's inventory part for the mastery view, so any profile fetch refreshes it.
pub(crate) fn remember_profile_inventory(app: &AppHandle, account: &str, payload: &Value) {
    let Some(inv) = parse_profile_inventory(payload) else {
        return;
    };
    if kv_put(
        &app.state::<Db>(),
        &inventory_key(account),
        &inv.to_string(),
    )
    .is_ok()
    {
        use tauri::Emitter;
        let _ = app.emit("profile-inventory-updated", ());
    }
}

/// The cached profile inventory of the saved account, when one was fetched.
pub fn profile_inventory(app: &AppHandle) -> Option<Value> {
    let account = load_account(&app.path().app_data_dir().ok()?)?;
    let raw = kv_get(&app.state::<Db>(), &inventory_key(&account)).ok()??;
    serde_json::from_str(&raw).ok()
}

fn etag_key(account: &str) -> String {
    format!("profile_etag:{account}")
}

/// Fetches the profile and caches its scans and inventory part (mastery). A 304 to the stored
/// ETag only stamps the cache as fresh. Records the attempt, so the UI can show a failure.
async fn refresh_profile(app: &AppHandle, account: &str) -> bool {
    let db = app.state::<Db>();
    let at = now_ms();
    // The ETag is only trusted while the scans it describes are still cached.
    let etag =
        read_cache(&db, account).and_then(|_| kv_get(&db, &etag_key(account)).ok().flatten());
    let ok = match fetch_profile_if_changed(account, etag.as_deref()).await {
        Ok(None) => kv_put(&db, &at_key(account), &at.to_string()).is_ok(),
        Ok(Some((json, etag))) => {
            remember_profile_inventory(app, account, &json);
            match parse_profile_scans(&json) {
                Some(scans) => {
                    let ok = write_cache(&db, account, at, &scans).is_ok();
                    if let (true, Some(e)) = (ok, etag) {
                        let _ = kv_put(&db, &etag_key(account), &e);
                    }
                    ok
                }
                None => {
                    tracing::warn!("profile fetch: no scans array in the response");
                    false
                }
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "profile fetch failed");
            false
        }
    };
    if ok {
        tracing::info!("profile fetch: ok");
    }
    *ATTEMPT.lock().unwrap() = Some((account.to_owned(), at, !ok));
    ok
}

/// First automatic fetch after launch, kept off the startup path.
const AUTO_FIRST: std::time::Duration = std::time::Duration::from_secs(20);
/// Then every few hours while the app runs (cheap with the ETag).
const AUTO_EVERY: std::time::Duration = std::time::Duration::from_secs(3 * 3600);

/// Refreshes the public profile in the background when an id is saved: shortly after launch,
/// then every `AUTO_EVERY`. Saving an id fetches right away (`profile_set_account`).
pub fn start_auto_refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTO_FIRST).await;
        loop {
            if let Some(account) = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|d| load_account(&d))
            {
                refresh_profile(&app, &account).await;
            }
            tokio::time::sleep(AUTO_EVERY).await;
        }
    });
}

/// The saved account id, if any.
#[tauri::command]
#[specta::specta]
pub fn profile_get_account(app: AppHandle) -> Option<String> {
    load_account(&app.path().app_data_dir().ok()?)
}

/// Saves the id found in `input` (empty clears it); errors with `invalid` when none is found.
#[tauri::command]
#[specta::specta]
pub fn profile_set_account(app: AppHandle, input: String) -> Result<Option<String>, String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    if input.trim().is_empty() {
        let _ = std::fs::remove_file(data.join(ACCOUNT_FILE));
        return Ok(None);
    }
    let id = account_id_from_input(&input).ok_or("invalid")?;
    save_account(&data, &id).map_err(|e| e.to_string())?;
    Ok(Some(id))
}

#[tauri::command]
#[specta::specta]
pub async fn get_codex_scans(app: AppHandle, refresh: bool) -> Result<CodexScansResult, String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let Some(account) = load_account(&data) else {
        return Ok(CodexScansResult {
            error: Some("no-account".into()),
            ..Default::default()
        });
    };
    let db = app.state::<Db>();
    let cached = read_cache(&db, &account);
    if !refresh || now_ms() < result(&account, cached.as_ref()).next_refresh_at as i64 {
        return Ok(result(&account, cached.as_ref()));
    }
    refresh_profile(&app, &account).await;
    let cached = read_cache(&db, &account);
    Ok(result(&account, cached.as_ref()))
}
