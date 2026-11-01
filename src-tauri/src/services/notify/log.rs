//! Notification history in SQLite (`notification_log`, migration 13), capped at 200 rows.

use rusqlite::{Connection, params};
use serde::Serialize;

use super::channels::NotificationKind;

pub const MAX_ENTRIES: i64 = 200;
const MAX_TITLE_CHARS: usize = 200;
const MAX_BODY_CHARS: usize = 500;

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct NotificationEntry {
    pub id: String,
    /// ISO timestamp; the UI formats it so a language switch repaints it.
    pub at: String,
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

pub fn record(
    conn: &Connection,
    kind: NotificationKind,
    title: &str,
    body: &str,
) -> rusqlite::Result<NotificationEntry> {
    let entry = NotificationEntry {
        id: uuid::Uuid::new_v4().simple().to_string(),
        at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        kind,
        title: clip(title, MAX_TITLE_CHARS),
        body: clip(body, MAX_BODY_CHARS),
    };
    conn.execute(
        "INSERT INTO notification_log (id, at, kind, title, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            entry.id,
            entry.at,
            entry.kind.as_str(),
            entry.title,
            entry.body
        ],
    )?;
    // The cap trims the oldest rows first.
    conn.execute(
        "DELETE FROM notification_log WHERE seq <= (SELECT max(seq) FROM notification_log) - ?1",
        [MAX_ENTRIES],
    )?;
    Ok(entry)
}

/// Newest first, the order the history list renders. Rows with an unknown kind are skipped and
/// text is re-clipped: the database is user-writable, so a stored row is not bounded by what
/// `record` enforced when it was written.
pub fn get_all(conn: &Connection) -> rusqlite::Result<Vec<NotificationEntry>> {
    let mut stmt =
        conn.prepare("SELECT id, at, kind, title, body FROM notification_log ORDER BY seq DESC")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, at, kind, title, body) = row?;
        let Some(kind) = NotificationKind::parse(&kind) else {
            continue;
        };
        out.push(NotificationEntry {
            id,
            at,
            kind,
            title: clip(&title, MAX_TITLE_CHARS),
            body: clip(&body, MAX_BODY_CHARS),
        });
    }
    out.truncate(MAX_ENTRIES as usize);
    Ok(out)
}

/// Says whether the entry was there.
pub fn remove(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM notification_log WHERE id = ?1", [id])? > 0)
}

pub fn clear(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM notification_log", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_conn;

    fn titles(conn: &Connection) -> Vec<String> {
        get_all(conn)
            .unwrap()
            .into_iter()
            .map(|e| e.title)
            .collect()
    }

    #[test]
    fn returns_the_newest_entry_first() {
        let c = mem_conn();
        record(&c, NotificationKind::World, "First", "one").unwrap();
        record(&c, NotificationKind::Trade, "Second", "two").unwrap();
        assert_eq!(titles(&c), ["Second", "First"]);
    }

    #[test]
    fn gives_every_entry_its_own_id() {
        let c = mem_conn();
        let a = record(&c, NotificationKind::App, "A", "a").unwrap();
        let b = record(&c, NotificationKind::App, "B", "b").unwrap();
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn keeps_200_entries_and_drops_the_oldest_past_the_cap() {
        let c = mem_conn();
        for i in 0..200 {
            record(&c, NotificationKind::App, &format!("n{i}"), "").unwrap();
        }
        let all = get_all(&c).unwrap();
        assert_eq!(all.len(), 200);
        assert_eq!(all.last().unwrap().title, "n0");

        record(&c, NotificationKind::App, "n200", "").unwrap();
        let all = get_all(&c).unwrap();
        assert_eq!(all.len(), 200);
        assert_eq!(all[0].title, "n200");
        assert_eq!(all.last().unwrap().title, "n1");
    }

    #[test]
    fn skips_rows_with_an_unknown_kind() {
        let c = mem_conn();
        record(&c, NotificationKind::Message, "Persisted", "from Tenno").unwrap();
        c.execute(
            "INSERT INTO notification_log (id, at, kind, title, body) VALUES ('bad', '2026-01-01T00:00:00.000Z', 'nope', 'x', '')",
            [],
        )
        .unwrap();
        let all = get_all(&c).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kind, NotificationKind::Message);
        assert_eq!(all[0].body, "from Tenno");
    }

    // A hand-written kind guard would drop every entry of a kind added to the enum.
    #[test]
    fn reloads_an_entry_of_every_declared_kind() {
        let c = mem_conn();
        for kind in NotificationKind::ALL {
            record(&c, kind, &format!("title-{}", kind.as_str()), "").unwrap();
        }
        assert_eq!(get_all(&c).unwrap().len(), NotificationKind::ALL.len());
    }

    #[test]
    fn trims_over_long_text_on_write_and_on_read() {
        let c = mem_conn();
        let e = record(
            &c,
            NotificationKind::App,
            &"t".repeat(400),
            &"b".repeat(900),
        )
        .unwrap();
        assert_eq!((e.title.len(), e.body.len()), (200, 500));
        c.execute(
            "INSERT INTO notification_log (id, at, kind, title, body) VALUES ('a', 'x', 'app', ?1, ?2)",
            params!["t".repeat(400), "b".repeat(900)],
        )
        .unwrap();
        let first = &get_all(&c).unwrap()[0];
        assert_eq!((first.title.len(), first.body.len()), (200, 500));
    }

    #[test]
    fn drops_one_entry_and_keeps_the_rest() {
        let c = mem_conn();
        record(&c, NotificationKind::World, "First", "one").unwrap();
        let middle = record(&c, NotificationKind::App, "Test", "fired").unwrap();
        record(&c, NotificationKind::Trade, "Third", "three").unwrap();
        assert!(remove(&c, &middle.id).unwrap());
        assert_eq!(titles(&c), ["Third", "First"]);
    }

    #[test]
    fn removing_an_unknown_id_is_a_no_op() {
        let c = mem_conn();
        record(&c, NotificationKind::World, "First", "one").unwrap();
        assert!(!remove(&c, "nope").unwrap());
        assert_eq!(get_all(&c).unwrap().len(), 1);
    }

    #[test]
    fn clears_the_stored_history() {
        let c = mem_conn();
        record(
            &c,
            NotificationKind::Trade,
            "Listing Closed",
            "Ash Prime Chassis",
        )
        .unwrap();
        clear(&c).unwrap();
        assert!(get_all(&c).unwrap().is_empty());
    }

    #[test]
    fn a_failed_write_is_an_error_not_a_panic() {
        let c = mem_conn();
        c.execute("DROP TABLE notification_log", []).unwrap();
        assert!(record(&c, NotificationKind::App, "Still returned", "body").is_err());
    }
}
