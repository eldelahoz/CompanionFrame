//! SQLite persistence of the imported snapshot and the deltas applied on top of it
//! (migration `inventory_snapshot`). A new snapshot replaces the old one and drops its deltas.

use rusqlite::{Connection, OptionalExtension, params};

use super::delta::{Delta, DeltaKind};

pub struct Row {
    pub source: String,
    pub imported_at: f64,
    pub file_modified_at: Option<f64>,
    pub path: Option<String>,
    pub hash: String,
    pub json: String,
}

pub fn save_snapshot(conn: &mut Connection, row: &Row) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM inventory_deltas", [])?;
    tx.execute(
        "INSERT OR REPLACE INTO inventory_snapshot (id, source, imported_at, file_modified_at, path, hash, json) \
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
        params![row.source, row.imported_at, row.file_modified_at, row.path, row.hash, row.json],
    )?;
    tx.commit()
}

pub fn load_snapshot(conn: &Connection) -> rusqlite::Result<Option<Row>> {
    conn.query_row(
        "SELECT source, imported_at, file_modified_at, path, hash, json FROM inventory_snapshot WHERE id = 1",
        [],
        |r| {
            Ok(Row {
                source: r.get(0)?,
                imported_at: r.get(1)?,
                file_modified_at: r.get(2)?,
                path: r.get(3)?,
                hash: r.get(4)?,
                json: r.get(5)?,
            })
        },
    )
    .optional()
}

pub fn clear(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM inventory_deltas", [])?;
    tx.execute("DELETE FROM inventory_snapshot", [])?;
    tx.commit()
}

pub fn insert_deltas(conn: &mut Connection, deltas: &[Delta]) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    {
        let mut ins = tx.prepare(
            "INSERT INTO inventory_deltas (at, kind, group_id, unique_name, label, count, platinum, estimated) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for d in deltas {
            ins.execute(params![
                d.at,
                d.kind.as_str(),
                d.group,
                d.unique_name,
                d.label,
                d.count,
                d.platinum,
                d.estimated
            ])?;
        }
    }
    tx.commit()
}

pub fn load_deltas(conn: &Connection) -> rusqlite::Result<Vec<Delta>> {
    let mut stmt = conn.prepare(
        "SELECT at, kind, group_id, unique_name, label, count, platinum, estimated \
         FROM inventory_deltas ORDER BY seq",
    )?;
    stmt.query_map([], |r| {
        Ok(Delta {
            at: r.get(0)?,
            kind: DeltaKind::parse(&r.get::<_, String>(1)?),
            group: r.get(2)?,
            unique_name: r.get(3)?,
            label: r.get(4)?,
            count: r.get(5)?,
            platinum: r.get(6)?,
            estimated: r.get(7)?,
        })
    })?
    .collect()
}
