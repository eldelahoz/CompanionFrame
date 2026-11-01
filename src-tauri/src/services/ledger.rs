//! Trade ledger in SQLite (migration 7). Port of WFHelper's tradeLedgerStore; the full event
//! lives in `json`, the other columns only serve range queries and batch undo.
use chrono::{DateTime, Local};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::gdpr_import::{
    ApplyOutcome, GdprStaging, ImportPreview, LedgerErrorCode, LedgerSink, NoCatalog, TradeEvent,
    TradeType,
};
use crate::db::Db;

const MAX_LIMIT: u32 = 200;

#[derive(Clone, Debug, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerQuery {
    /// "YYYY-MM-DD", inclusive, local calendar day.
    pub from: Option<String>,
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<TradeType>,
    pub text: Option<String>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerPage {
    pub events: Vec<TradeEvent>,
    /// Rows matching the query, ignoring offset/limit.
    pub total: u32,
}

/// Absent field leaves the value alone; `credits`/`tradeTax` take a number to set it.
// shortcut: no way to clear credits/tradeTax, add a double-option when the edit UI needs it.
#[derive(Clone, Debug, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerPatch {
    pub plat_change: Option<f64>,
    pub partner: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<TradeType>,
    pub credits: Option<f64>,
    pub trade_tax: Option<f64>,
}

/// Local calendar day of an ISO timestamp; unparsable dates fall back to their date prefix.
fn day_key(date: &str) -> String {
    DateTime::parse_from_rfc3339(date).map_or_else(
        |_| date.chars().take(10).collect(),
        |t| t.with_timezone(&Local).format("%Y-%m-%d").to_string(),
    )
}

fn kind_str(k: TradeType) -> &'static str {
    match k {
        TradeType::Sale => "sale",
        TradeType::Purchase => "purchase",
        TradeType::Trade => "trade",
    }
}

fn direction(plat_change: i64) -> &'static str {
    if plat_change >= 0 {
        "received"
    } else {
        "given"
    }
}

fn insert(conn: &Connection, e: &TradeEvent) -> rusqlite::Result<bool> {
    let json =
        serde_json::to_string(e).map_err(|x| rusqlite::Error::ToSqlConversionFailure(x.into()))?;
    Ok(conn.execute(
        "INSERT OR IGNORE INTO trade_events (id, ts, day_key, kind, direction, partner, platinum, source, batch_id, json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            e.id,
            e.date,
            day_key(&e.date),
            kind_str(e.kind),
            direction(e.plat_change),
            e.partner,
            e.plat_change,
            e.source.map(|s| format!("{s:?}").to_lowercase()),
            e.import_batch_id,
            json
        ],
    )? == 1)
}

/// Insert events in one transaction; ids already present are skipped, never overwritten.
/// This is the plain insert API importers (GDPR) call.
pub fn add_events(conn: &mut Connection, events: &[TradeEvent]) -> rusqlite::Result<ApplyOutcome> {
    let tx = conn.transaction()?;
    let mut out = ApplyOutcome {
        applied: 0,
        skipped_duplicates: 0,
    };
    for e in events {
        if insert(&tx, e)? {
            out.applied += 1;
        } else {
            out.skipped_duplicates += 1;
        }
    }
    tx.commit()?;
    Ok(out)
}

pub fn query(conn: &Connection, q: &LedgerQuery) -> rusqlite::Result<LedgerPage> {
    let mut wh = vec!["1=1".to_string()];
    let mut args: Vec<String> = vec![];
    let mut cond = |sql: &str, v: String| {
        args.push(v);
        wh.push(sql.replace('?', &format!("?{}", args.len())));
    };
    if let Some(v) = &q.from {
        cond("day_key >= ?", v.clone());
    }
    if let Some(v) = &q.to {
        cond("day_key <= ?", v.clone());
    }
    if let Some(k) = q.kind {
        cond("kind = ?", kind_str(k).into());
    }
    if let Some(t) = q.text.as_deref().filter(|t| !t.trim().is_empty()) {
        let esc = t
            .trim()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        cond("json LIKE ? ESCAPE '\\'", format!("%{esc}%"));
    }
    let wh = wh.join(" AND ");
    let total: u32 = conn.query_row(
        &format!("SELECT count(*) FROM trade_events WHERE {wh}"),
        rusqlite::params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    let limit = q.limit.unwrap_or(MAX_LIMIT).min(MAX_LIMIT);
    let mut stmt = conn.prepare(&format!(
        "SELECT json FROM trade_events WHERE {wh} ORDER BY ts DESC, id DESC LIMIT {limit} OFFSET {}",
        q.offset.unwrap_or(0)
    ))?;
    let events = stmt
        .query_map(rusqlite::params_from_iter(args.iter()), |r| {
            r.get::<_, String>(0)
        })?
        .filter_map(|j| serde_json::from_str(&j.ok()?).ok())
        .collect();
    Ok(LedgerPage { events, total })
}

fn get(conn: &Connection, id: &str) -> Option<TradeEvent> {
    let json: String = conn
        .query_row("SELECT json FROM trade_events WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .ok()?;
    serde_json::from_str(&json).ok()
}

pub fn edit(conn: &Connection, id: &str, p: &LedgerPatch) -> Result<(), LedgerErrorCode> {
    let mut e = get(conn, id).ok_or(LedgerErrorCode::RowGone)?;
    let num = |v: f64| {
        v.is_finite()
            .then_some(v.round() as i64)
            .ok_or(LedgerErrorCode::InvalidPatch)
    };
    if let Some(v) = p.plat_change {
        e.plat_change = num(v)?;
    }
    if let Some(v) = &p.partner {
        e.partner = Some(v.trim().to_string()).filter(|s| !s.is_empty());
    }
    if let Some(k) = p.kind {
        e.kind = k;
    }
    if let Some(v) = p.credits {
        e.credits = Some(num(v)?);
    }
    if let Some(v) = p.trade_tax {
        e.trade_tax = Some(num(v)?);
    }
    let json = serde_json::to_string(&e).map_err(|_| LedgerErrorCode::SaveFailed)?;
    conn.execute(
        "UPDATE trade_events SET kind=?2, direction=?3, partner=?4, platinum=?5, json=?6 WHERE id=?1",
        params![
            id,
            kind_str(e.kind),
            direction(e.plat_change),
            e.partner,
            e.plat_change,
            json
        ],
    )
    .map_err(|_| LedgerErrorCode::SaveFailed)?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM trade_events WHERE id = ?1", [id])? == 1)
}

/// Undo an import: remove every row stamped with the batch id.
pub fn delete_batch(conn: &Connection, batch_id: &str) -> rusqlite::Result<u32> {
    Ok(conn.execute("DELETE FROM trade_events WHERE batch_id = ?1", [batch_id])? as u32)
}

/// Remember that the row's warframe.market listing was closed, so the UI stops offering it.
pub fn mark_wfm_closed(conn: &Connection, id: &str) -> Result<(), LedgerErrorCode> {
    let mut e = get(conn, id).ok_or(LedgerErrorCode::RowGone)?;
    e.wfm_closed = Some(true);
    let json = serde_json::to_string(&e).map_err(|_| LedgerErrorCode::SaveFailed)?;
    conn.execute(
        "UPDATE trade_events SET json=?2 WHERE id=?1",
        params![id, json],
    )
    .map_err(|_| LedgerErrorCode::SaveFailed)?;
    Ok(())
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LedgerBatch {
    pub batch_id: String,
    pub source: Option<String>,
    pub rows: u32,
    /// Earliest and latest trade date in the batch (the import time is not stored).
    pub from: String,
    pub to: String,
}

/// Imported batches still in the ledger, most recently imported first.
pub fn list_batches(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<LedgerBatch>> {
    let mut stmt = conn.prepare(
        "SELECT batch_id, max(source), count(*), min(ts), max(ts) FROM trade_events
         WHERE batch_id IS NOT NULL GROUP BY batch_id ORDER BY max(rowid) DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(LedgerBatch {
            batch_id: r.get(0)?,
            source: r.get(1)?,
            rows: r.get(2)?,
            from: r.get(3)?,
            to: r.get(4)?,
        })
    })?;
    rows.collect()
}

/// All stored events, for matching a GDPR import against live rows.
fn all(conn: &Connection) -> rusqlite::Result<Vec<TradeEvent>> {
    let mut stmt = conn.prepare("SELECT json FROM trade_events")?;
    Ok(stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|j| serde_json::from_str(&j.ok()?).ok())
        .collect())
}

pub struct SqlSink<'a>(pub &'a Db);
impl LedgerSink for SqlSink<'_> {
    fn add_events(&self, events: Vec<TradeEvent>) -> Result<ApplyOutcome, LedgerErrorCode> {
        let mut conn = self.0.0.lock().unwrap();
        add_events(&mut conn, &events).map_err(|e| {
            tracing::warn!("[Ledger] write failed: {e}");
            LedgerErrorCode::ImportWriteFailed
        })
    }
}

fn read_err(e: rusqlite::Error) -> LedgerErrorCode {
    tracing::warn!("[Ledger] read failed: {e}");
    LedgerErrorCode::LedgerReadFailed
}

#[tauri::command]
#[specta::specta]
pub fn ledger_get_range(db: State<Db>, query: LedgerQuery) -> Result<LedgerPage, LedgerErrorCode> {
    self::query(&db.0.lock().unwrap(), &query).map_err(read_err)
}

#[tauri::command]
#[specta::specta]
pub fn ledger_add(db: State<Db>, event: TradeEvent) -> Result<(), LedgerErrorCode> {
    add_events(&mut db.0.lock().unwrap(), &[event])
        .map(drop)
        .map_err(|_| LedgerErrorCode::SaveFailed)
}

#[tauri::command]
#[specta::specta]
pub fn ledger_edit(db: State<Db>, id: String, patch: LedgerPatch) -> Result<(), LedgerErrorCode> {
    edit(&db.0.lock().unwrap(), &id, &patch)
}

#[tauri::command]
#[specta::specta]
pub fn ledger_undo_batch(db: State<Db>, batch_id: String) -> Result<u32, LedgerErrorCode> {
    delete_batch(&db.0.lock().unwrap(), &batch_id).map_err(|_| LedgerErrorCode::SaveFailed)
}

#[tauri::command]
#[specta::specta]
pub fn ledger_list_batches(db: State<Db>) -> Result<Vec<LedgerBatch>, LedgerErrorCode> {
    list_batches(&db.0.lock().unwrap(), 5).map_err(read_err)
}

#[tauri::command]
#[specta::specta]
pub fn gdpr_preview_file(
    path: String,
    staging: State<GdprStaging>,
    db: State<Db>,
) -> Result<ImportPreview, LedgerErrorCode> {
    let existing = all(&db.0.lock().unwrap()).map_err(read_err)?;
    staging.preview_file(std::path::Path::new(&path), &existing, &NoCatalog)
}

#[derive(Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub applied: u32,
    pub skipped_duplicates: u32,
}

#[tauri::command]
#[specta::specta]
pub fn gdpr_apply(
    batch_id: String,
    staging: State<GdprStaging>,
    db: State<Db>,
) -> Result<ApplyResult, LedgerErrorCode> {
    let o = staging.apply(&batch_id, &SqlSink(&db))?;
    Ok(ApplyResult {
        applied: o.applied,
        skipped_duplicates: o.skipped_duplicates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_conn;
    use crate::services::gdpr_import::{Direction, EventSource, TradeItem};

    fn ev(id: &str, date: &str, plat: i64, batch: Option<&str>) -> TradeEvent {
        TradeEvent {
            id: id.into(),
            date: date.into(),
            kind: TradeType::Sale,
            plat_change: plat,
            items: vec![TradeItem {
                internal_name: "x".into(),
                display_name: "Ash Prime Chassis".into(),
                count: 1,
                direction: Direction::Given,
                wfm_slug: None,
                wfm_thumb: None,
            }],
            partner: Some("Kestrel".into()),
            schema_version: Some(2),
            source: Some(EventSource::Gdpr),
            source_record_id: None,
            import_batch_id: batch.map(Into::into),
            wfm_closed: None,
            credits: None,
            trade_tax: None,
        }
    }

    #[test]
    fn insert_twice_does_not_duplicate() {
        let mut c = mem_conn();
        let e = [ev("a", "2024-03-04T12:00:00Z", 45, None)];
        assert_eq!(add_events(&mut c, &e).unwrap().applied, 1);
        assert_eq!(add_events(&mut c, &e).unwrap().skipped_duplicates, 1);
        assert_eq!(query(&c, &LedgerQuery::default()).unwrap().total, 1);
    }

    #[test]
    fn ranges_text_and_paging() {
        let mut c = mem_conn();
        let evs: Vec<_> = (1..=5)
            .map(|d| {
                ev(
                    &format!("e{d}"),
                    &format!("2024-03-0{d}T12:00:00Z"),
                    d,
                    None,
                )
            })
            .collect();
        add_events(&mut c, &evs).unwrap();
        let q = |f: &dyn Fn(&mut LedgerQuery)| {
            let mut q = LedgerQuery::default();
            f(&mut q);
            query(&c, &q).unwrap()
        };
        let p = q(&|q| {
            q.from = Some("2024-03-02".into());
            q.to = Some("2024-03-04".into());
        });
        assert_eq!(p.total, 3);
        let p = q(&|q| {
            q.limit = Some(2);
            q.offset = Some(1);
        });
        assert_eq!(
            (p.total, p.events.len(), p.events[0].id.as_str()),
            (5, 2, "e4")
        );
        assert_eq!(q(&|q| q.text = Some("chassis".into())).total, 5);
        assert_eq!(q(&|q| q.text = Some("100%".into())).total, 0);
    }

    #[test]
    fn edit_delete_and_undo_batch() {
        let mut c = mem_conn();
        add_events(
            &mut c,
            &[
                ev("a", "2024-03-04T12:00:00Z", 45, Some("b1")),
                ev("b", "2024-03-05T12:00:00Z", 10, Some("b1")),
                ev("c", "2024-03-06T12:00:00Z", 10, None),
            ],
        )
        .unwrap();
        edit(
            &c,
            "c",
            &LedgerPatch {
                plat_change: Some(-7.0),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(get(&c, "c").unwrap().plat_change, -7);
        assert_eq!(
            edit(&c, "zz", &LedgerPatch::default()),
            Err(LedgerErrorCode::RowGone)
        );
        assert_eq!(delete_batch(&c, "b1").unwrap(), 2);
        assert!(delete(&c, "c").unwrap());
        assert_eq!(query(&c, &LedgerQuery::default()).unwrap().total, 0);
    }

    #[test]
    fn marking_wfm_closed_persists_and_misses_report_row_gone() {
        let mut c = mem_conn();
        add_events(&mut c, &[ev("a", "2024-03-04T12:00:00Z", 45, None)]).unwrap();
        assert_eq!(get(&c, "a").unwrap().wfm_closed, None);
        mark_wfm_closed(&c, "a").unwrap();
        assert_eq!(get(&c, "a").unwrap().wfm_closed, Some(true));
        assert_eq!(
            query(&c, &LedgerQuery::default()).unwrap().events[0].wfm_closed,
            Some(true)
        );
        assert_eq!(mark_wfm_closed(&c, "zz"), Err(LedgerErrorCode::RowGone));
    }

    #[test]
    fn batches_list_newest_first_and_vanish_when_undone() {
        let mut c = mem_conn();
        add_events(
            &mut c,
            &[
                ev("a", "2024-03-04T12:00:00Z", 45, Some("b1")),
                ev("b", "2024-03-09T12:00:00Z", 10, Some("b1")),
                ev("c", "2024-03-06T12:00:00Z", 10, Some("b2")),
                ev("d", "2024-03-07T12:00:00Z", 10, None),
            ],
        )
        .unwrap();
        let l = list_batches(&c, 5).unwrap();
        let got: Vec<_> = l.iter().map(|b| (b.batch_id.as_str(), b.rows)).collect();
        assert_eq!(got, [("b2", 1), ("b1", 2)]);
        assert_eq!(
            (l[1].source.as_deref(), l[1].from.as_str(), l[1].to.as_str()),
            (Some("gdpr"), "2024-03-04T12:00:00Z", "2024-03-09T12:00:00Z")
        );
        assert_eq!(list_batches(&c, 1).unwrap().len(), 1);
        delete_batch(&c, "b2").unwrap();
        assert_eq!(list_batches(&c, 5).unwrap().len(), 1);
    }

    /// parse -> apply -> query, twice: the second import adds nothing.
    #[test]
    fn gdpr_import_through_the_sql_sink_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("ledger-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("trades.csv");
        std::fs::write(
            &file,
            "Date,Type,Item,Platinum,Partner\n2024-03-04,Sale,Ash Prime Chassis,45,Kestrel\n2024-03-05,Purchase,Nikana Prime Blade,30,Vor\nnot a row\n",
        )
        .unwrap();
        let db = Db(std::sync::Mutex::new(mem_conn()));
        let staging = GdprStaging::default();
        let preview = staging.preview_file(&file, &[], &NoCatalog).unwrap();
        let out = staging.apply(&preview.batch_id, &SqlSink(&db)).unwrap();
        assert_eq!(out.applied, 2);
        assert!(preview.counts.rejected >= 1 || preview.counts.unresolved >= 1);

        let existing = all(&db.0.lock().unwrap()).unwrap();
        let again = staging.preview_file(&file, &existing, &NoCatalog).unwrap();
        assert_eq!(again.counts.parsed, 0);
        assert_eq!(again.counts.duplicates, 2);
        let out = staging.apply(&again.batch_id, &SqlSink(&db)).unwrap();
        assert_eq!(out.applied, 0);

        let conn = db.0.lock().unwrap();
        assert_eq!(query(&conn, &LedgerQuery::default()).unwrap().total, 2);
        assert_eq!(delete_batch(&conn, &preview.batch_id).unwrap(), 2);
        std::fs::remove_dir_all(dir).ok();
    }
}
