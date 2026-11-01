//! Market alerts: user rules over warframe.market listings that notify through the real
//! [`Notifier`](crate::services::notify::Notifier). `rules` is pure, `store` is SQLite, `checker`
//! is the engine (the only network part).

pub mod checker;
pub mod rules;
pub mod store;

#[cfg(test)]
mod tests;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use self::checker::{Ctx, Engine, MarketAlertEngineStatus, MarketAlertTestFire};
use self::rules::{
    MAX_RULES, MarketAlertBinding, MarketAlertHit, MarketAlertRule, build_export, parse_import,
};
use crate::db::{Db, db_err};
use crate::services::clock::now_ms;
use crate::services::notify::Notifications;
use crate::services::wfm::session::Session;

const CHANGED_EVENT: &str = "market-alerts-changed";

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketAlertEntry {
    pub rule: MarketAlertRule,
    pub binding: MarketAlertBinding,
    /// Epoch ms the quiet window ends, while one is running.
    pub cooldown_until: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MarketAlertSave {
    pub rule: MarketAlertRule,
    pub binding: Option<MarketAlertBinding>,
    /// Owned count the UI knew at save time; the item rule's owned gates fall back on it.
    pub owned_count: Option<u32>,
}

/// Arms the engine: first tick after a short delay, then once a minute. The task reads its
/// dependencies from managed state each tick, so it only needs `Engine` managed here.
pub fn setup(app: &tauri::App) {
    app.manage(Engine::default());
    app.state::<Engine>().arm();
    let h = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(checker::INITIAL_DELAY_SECS)).await;
        let mut every = tokio::time::interval(Duration::from_secs(checker::TICK_SECS));
        loop {
            every.tick().await;
            let (engine, db, session, notif) = (
                h.state::<Engine>(),
                h.state::<Db>(),
                h.state::<Session>(),
                h.state::<Notifications>(),
            );
            let h2 = h.clone();
            let changed = move || {
                let _ = h2.emit(CHANGED_EVENT, ());
            };
            let cx = Ctx {
                client: session.client(),
                db: &db,
                notifier: &*notif,
                own_name: session.status().user_name,
                changed: &changed,
            };
            engine.tick(&cx, now_ms()).await;
        }
    });
}

fn emit_changed(app: &AppHandle) {
    let _ = app.emit(CHANGED_EVENT, ());
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_get(
    db: State<'_, Db>,
    engine: State<'_, Engine>,
) -> Result<Vec<MarketAlertEntry>, String> {
    let now = now_ms();
    let rows = store::load(&db.0.lock().unwrap()).map_err(db_err)?;
    Ok(rows
        .into_iter()
        .map(|(rule, binding)| MarketAlertEntry {
            cooldown_until: engine.cooldown_until(&rule.id, now).map(|t| t as f64),
            rule,
            binding,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_set(
    app: AppHandle,
    db: State<'_, Db>,
    engine: State<'_, Engine>,
    payload: MarketAlertSave,
) -> Result<MarketAlertRule, String> {
    let mut rule = payload.rule;
    rule.validate()?;
    let conn = db.0.lock().unwrap();
    let rule = store::save(&conn, rule, payload.binding.unwrap_or_default())?;
    if let (Some(item), Some(n)) = (&rule.item, payload.owned_count) {
        store::set_owned(&conn, &item.item_url_name, n).map_err(db_err)?;
    }
    drop(conn);
    // An edited rule re-evaluates at once instead of waiting out the old timers.
    engine.reset_rule(&rule.id);
    emit_changed(&app);
    Ok(rule)
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_delete(
    app: AppHandle,
    db: State<'_, Db>,
    engine: State<'_, Engine>,
    id: String,
) -> Result<bool, String> {
    let gone = store::delete(&db.0.lock().unwrap(), &id).map_err(db_err)?;
    engine.forget_rule(&id);
    emit_changed(&app);
    Ok(gone)
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_set_enabled(
    app: AppHandle,
    db: State<'_, Db>,
    engine: State<'_, Engine>,
    id: String,
    enabled: bool,
) -> Result<bool, String> {
    let found = store::set_enabled(&db.0.lock().unwrap(), &id, enabled).map_err(db_err)?;
    if enabled {
        engine.reset_rule(&id);
    } else {
        engine.forget_rule(&id);
    }
    emit_changed(&app);
    Ok(found)
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_clear_cooldown(
    db: State<'_, Db>,
    engine: State<'_, Engine>,
    id: String,
) -> Result<bool, String> {
    let found = store::get(&db.0.lock().unwrap(), &id)
        .map_err(db_err)?
        .is_some();
    if found {
        engine.reset_rule(&id);
    }
    Ok(found)
}

#[tauri::command]
#[specta::specta]
pub async fn market_alerts_test(
    db: State<'_, Db>,
    engine: State<'_, Engine>,
    session: State<'_, Session>,
    notif: State<'_, Notifications>,
    id: String,
) -> Result<MarketAlertTestFire, String> {
    let cx = Ctx {
        client: session.client(),
        db: &db,
        notifier: &*notif,
        own_name: session.status().user_name,
        changed: &|| {},
    };
    engine.test_fire(&cx, &id).await
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_hits_get(db: State<'_, Db>) -> Result<Vec<MarketAlertHit>, String> {
    store::hits(&db.0.lock().unwrap()).map_err(db_err)
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_hits_clear(app: AppHandle, db: State<'_, Db>) -> Result<(), String> {
    store::clear_hits(&db.0.lock().unwrap()).map_err(db_err)?;
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn market_alerts_status(
    db: State<'_, Db>,
    engine: State<'_, Engine>,
) -> MarketAlertEngineStatus {
    engine.status(&db)
}

/// JSON of the criteria only; `ids` picks rules, absent exports all, empty exports nothing.
#[tauri::command]
#[specta::specta]
pub fn market_alerts_export(db: State<'_, Db>, ids: Option<Vec<String>>) -> Result<String, String> {
    let rows = store::load(&db.0.lock().unwrap()).map_err(db_err)?;
    let picked: Vec<_> = rows
        .into_iter()
        .map(|(r, _)| r)
        .filter(|r| ids.as_ref().is_none_or(|i| i.contains(&r.id)))
        .collect();
    Ok(build_export(&picked))
}

/// Imported rules arrive disabled so a shared file never fires before the user has seen it.
#[tauri::command]
#[specta::specta]
pub fn market_alerts_import(
    app: AppHandle,
    db: State<'_, Db>,
    text: String,
) -> Result<u32, String> {
    let parsed = parse_import(&text)?;
    let conn = db.0.lock().unwrap();
    let have = store::load(&conn).map_err(db_err)?.len();
    if have + parsed.len() > MAX_RULES {
        return Err("rule limit reached".into());
    }
    let n = parsed.len() as u32;
    for mut rule in parsed {
        rule.enabled = false;
        store::save(&conn, rule, MarketAlertBinding::default())?;
    }
    drop(conn);
    emit_changed(&app);
    Ok(n)
}
