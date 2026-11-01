//! Generic run persistence (migrations 10-11): one `runs` row per run with the feature's record as
//! JSON, plus the gzipped raw log capture in `run_events`. Replaces WFHelper's index + `.log.gz` files.
use std::io::{Read, Write};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use rusqlite::{Connection, OptionalExtension, params};

const MAX_TAGS: usize = 12;
const MAX_TAG_LEN: usize = 32;
const MAX_NOTES_LEN: usize = 2000;

/// Tags trimmed, whitespace-collapsed, capped and de-duplicated case-insensitively.
pub fn normalize_tags(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in raw {
        let collapsed = entry.split_whitespace().collect::<Vec<_>>().join(" ");
        let tag = collapsed.chars().take(MAX_TAG_LEN).collect::<String>();
        let tag = tag.trim();
        if tag.is_empty() || out.iter().any(|t| t.to_lowercase() == tag.to_lowercase()) {
            continue;
        }
        out.push(tag.to_string());
        if out.len() >= MAX_TAGS {
            break;
        }
    }
    out
}

/// Notes without control characters (tab and newline stay), CRLF folded, capped.
pub fn normalize_notes(raw: &str) -> String {
    raw.replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|&c| (c >= ' ' || c == '\t' || c == '\n') && c != '\u{7f}')
        .take(MAX_NOTES_LEN)
        .collect::<String>()
        .trim()
        .to_string()
}

pub struct RunRow {
    pub id: String,
    pub kind: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub end_reason: String,
    pub source: String,
    /// The feature's full record as JSON.
    pub summary: String,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub duplicate_of: Option<String>,
    /// Compressed size of the raw capture; 0 once it is deleted.
    pub log_size: i64,
}

const COLS: &str = "id, kind, started_at, ended_at, end_reason, source, summary, notes, tags, duplicate_of, log_size";

fn row(r: &rusqlite::Row) -> rusqlite::Result<RunRow> {
    let tags: String = r.get(8)?;
    Ok(RunRow {
        id: r.get(0)?,
        kind: r.get(1)?,
        started_at: r.get(2)?,
        ended_at: r.get(3)?,
        end_reason: r.get(4)?,
        source: r.get(5)?,
        summary: r.get(6)?,
        notes: r.get(7)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        duplicate_of: r.get(9)?,
        log_size: r.get(10)?,
    })
}

fn tags_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

pub fn gzip(lines: &[String]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    for l in lines {
        // Writing into a Vec cannot fail.
        let _ = enc.write_all(l.as_bytes());
        let _ = enc.write_all(b"\n");
    }
    enc.finish().unwrap_or_default()
}

pub fn gunzip(blob: &[u8]) -> Option<String> {
    let mut out = String::new();
    GzDecoder::new(blob).read_to_string(&mut out).ok()?;
    Some(out)
}

/// False when the id already exists (nothing is written).
pub fn insert(conn: &mut Connection, r: &RunRow, log_gz: Option<&[u8]>) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    let n = tx.execute(
        "INSERT OR IGNORE INTO runs (id, kind, started_at, ended_at, end_reason, source, summary, notes, tags, duplicate_of, log_size)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            r.id,
            r.kind,
            r.started_at,
            r.ended_at,
            r.end_reason,
            r.source,
            r.summary,
            r.notes,
            tags_json(&r.tags),
            r.duplicate_of,
            r.log_size
        ],
    )?;
    if n == 1
        && let Some(blob) = log_gz
    {
        tx.execute(
            "INSERT INTO run_events (run_id, log) VALUES (?1, ?2)",
            params![r.id, blob],
        )?;
    }
    tx.commit()?;
    Ok(n == 1)
}

pub fn exists(conn: &Connection, id: &str) -> bool {
    conn.query_row("SELECT 1 FROM runs WHERE id = ?1", [id], |_| Ok(()))
        .optional()
        .is_ok_and(|r| r.is_some())
}

/// Newest first.
pub fn list(conn: &Connection, kind: &str) -> rusqlite::Result<Vec<RunRow>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM runs WHERE kind = ?1 ORDER BY started_at DESC, id DESC"
    ))?;
    st.query_map([kind], row)?.collect()
}

pub fn get(conn: &Connection, id: &str) -> rusqlite::Result<Option<RunRow>> {
    conn.query_row(&format!("SELECT {COLS} FROM runs WHERE id = ?1"), [id], row)
        .optional()
}

pub fn disk_usage(conn: &Connection, kind: &str) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(log_size), 0) FROM runs WHERE kind = ?1",
        [kind],
        |r| r.get(0),
    )
}

/// Rewrites everything a feature may change after the insert.
pub fn update(conn: &Connection, r: &RunRow) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE runs SET summary = ?2, notes = ?3, tags = ?4, duplicate_of = ?5 WHERE id = ?1",
        params![r.id, r.summary, r.notes, tags_json(&r.tags), r.duplicate_of],
    )?;
    Ok(())
}

/// Normalized tags as stored; None for an unknown id.
pub fn set_tags(
    conn: &Connection,
    id: &str,
    tags: &[String],
) -> rusqlite::Result<Option<Vec<String>>> {
    let clean = normalize_tags(tags);
    let n = conn.execute(
        "UPDATE runs SET tags = ?2 WHERE id = ?1",
        params![id, tags_json(&clean)],
    )?;
    Ok((n == 1).then_some(clean))
}

/// Normalized notes as stored ("" when cleared); None for an unknown id.
pub fn set_notes(conn: &Connection, id: &str, notes: &str) -> rusqlite::Result<Option<String>> {
    let clean = normalize_notes(notes);
    let value = (!clean.is_empty()).then_some(clean.as_str());
    let n = conn.execute(
        "UPDATE runs SET notes = ?2 WHERE id = ?1",
        params![id, value],
    )?;
    Ok((n == 1).then_some(clean))
}

pub fn log_gz(conn: &Connection, id: &str) -> Option<Vec<u8>> {
    conn.query_row("SELECT log FROM run_events WHERE run_id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()
    .ok()
    .flatten()
}

/// False when the run has no capture.
pub fn delete_log(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    let n = conn.execute("DELETE FROM run_events WHERE run_id = ?1", [id])?;
    conn.execute("UPDATE runs SET log_size = 0 WHERE id = ?1", [id])?;
    Ok(n > 0)
}

pub fn delete(conn: &mut Connection, id: &str) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM run_events WHERE run_id = ?1", [id])?;
    let n = tx.execute("DELETE FROM runs WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(n == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_conn;

    fn run(id: &str) -> RunRow {
        RunRow {
            id: id.into(),
            kind: "arbi".into(),
            started_at: 1,
            ended_at: 2,
            end_reason: "mission-end".into(),
            source: "live".into(),
            summary: "{}".into(),
            notes: None,
            tags: vec![],
            duplicate_of: None,
            log_size: 3,
        }
    }

    #[test]
    fn tags_are_trimmed_collapsed_capped_and_deduped() {
        let raw: Vec<String> = [
            "  boar   run ",
            "BOAR run",
            "",
            "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
        ]
        .map(String::from)
        .into();
        let t = normalize_tags(&raw);
        assert_eq!(t[0], "boar run");
        assert_eq!(t.len(), 2);
        assert_eq!(t[1].len(), MAX_TAG_LEN);
        let many: Vec<String> = (0..20).map(|i| format!("t{i}")).collect();
        assert_eq!(normalize_tags(&many).len(), MAX_TAGS);
    }

    #[test]
    fn notes_drop_control_chars_and_are_capped() {
        assert_eq!(normalize_notes(" a\r\nb\u{0}\tc\u{7f} "), "a\nb\tc");
        assert_eq!(normalize_notes(&"y".repeat(3000)).len(), MAX_NOTES_LEN);
    }

    #[test]
    fn insert_list_annotate_and_delete() {
        let mut c = mem_conn();
        let blob = gzip(&["one".into(), "two".into()]);
        assert!(insert(&mut c, &run("a"), Some(&blob)).unwrap());
        assert!(
            !insert(&mut c, &run("a"), Some(&blob)).unwrap(),
            "same id never overwrites"
        );
        assert_eq!(gunzip(&log_gz(&c, "a").unwrap()).unwrap(), "one\ntwo\n");
        assert_eq!(list(&c, "arbi").unwrap().len(), 1);
        assert!(list(&c, "pt").unwrap().is_empty());
        assert_eq!(disk_usage(&c, "arbi").unwrap(), 3);

        assert_eq!(
            set_tags(&c, "a", &["x".into()]).unwrap(),
            Some(vec!["x".into()])
        );
        assert_eq!(set_notes(&c, "a", " hi ").unwrap(), Some("hi".into()));
        assert_eq!(set_notes(&c, "nope", "x").unwrap(), None);
        let r = get(&c, "a").unwrap().unwrap();
        assert_eq!(
            (r.notes.as_deref(), r.tags),
            (Some("hi"), vec!["x".to_string()])
        );
        assert_eq!(set_notes(&c, "a", "  ").unwrap(), Some(String::new()));
        assert_eq!(get(&c, "a").unwrap().unwrap().notes, None);

        assert!(delete_log(&c, "a").unwrap());
        assert!(log_gz(&c, "a").is_none());
        assert_eq!(disk_usage(&c, "arbi").unwrap(), 0);
        assert!(delete(&mut c, "a").unwrap());
        assert!(!delete(&mut c, "a").unwrap());
    }
}
