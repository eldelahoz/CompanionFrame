//! Live riven roll sessions (table `riven_sessions`): the log gives the weapon, the roll count and
//! the Kuva cost of each rolling-screen visit; card stats need OCR (Phase 6) and are not here.
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::services::eelog_parsers::riven::RivenEvent;
use crate::services::runs::store::normalize_notes;

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenRollSession {
    pub id: String,
    pub weapon: String,
    pub rolls: u32,
    pub kuva_spent: u32,
    pub started_at: f64,
    pub ended_at: f64,
    pub notes: Option<String>,
    pub pinned: bool,
}

/// Folds one rolling-screen visit out of the state machine's events.
#[derive(Default)]
pub struct Recorder {
    open: Option<Open>,
}

struct Open {
    started_at: f64,
    weapon: String,
    cost: u32,
    rolls: u32,
    kuva: u32,
}

impl Recorder {
    /// The finished session when `event` closes one that rolled at least once.
    pub fn feed(&mut self, event: &RivenEvent, now_ms: f64) -> Option<RivenRollSession> {
        let now = now_ms.floor();
        match event {
            RivenEvent::SessionOpen => {
                self.open = Some(Open {
                    started_at: now,
                    weapon: String::new(),
                    cost: 0,
                    rolls: 0,
                    kuva: 0,
                });
            }
            RivenEvent::RollPending {
                weapon,
                kuva_per_roll,
            } => {
                if let Some(o) = &mut self.open {
                    o.weapon.clone_from(weapon);
                    o.cost = *kuva_per_roll;
                }
            }
            RivenEvent::RollConfirmed => {
                if let Some(o) = &mut self.open {
                    o.rolls += 1;
                    o.kuva += o.cost;
                }
            }
            RivenEvent::SessionClose => {
                let o = self.open.take().filter(|o| o.rolls > 0)?;
                return Some(RivenRollSession {
                    id: format!("riven-{}", o.started_at as i64),
                    weapon: o.weapon,
                    rolls: o.rolls,
                    kuva_spent: o.kuva,
                    started_at: o.started_at,
                    ended_at: now,
                    notes: None,
                    pinned: false,
                });
            }
            _ => {}
        }
        None
    }

    pub fn reset(&mut self) {
        self.open = None;
    }
}

pub fn save(conn: &Connection, s: &RivenRollSession) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO riven_sessions (id, weapon, rolls, kuva_spent, started_at, ended_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![s.id, s.weapon, s.rolls, s.kuva_spent, s.started_at, s.ended_at],
    )?;
    Ok(())
}

/// Pinned first, then newest first.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<RivenRollSession>> {
    conn.prepare("SELECT id, weapon, rolls, kuva_spent, started_at, ended_at, notes, pinned FROM riven_sessions ORDER BY pinned DESC, started_at DESC LIMIT 500")?
        .query_map([], |r| {
            Ok(RivenRollSession {
                id: r.get(0)?,
                weapon: r.get(1)?,
                rolls: r.get(2)?,
                kuva_spent: r.get(3)?,
                started_at: r.get(4)?,
                ended_at: r.get(5)?,
                notes: r.get(6)?,
                pinned: r.get(7)?,
            })
        })?
        .collect()
}

/// The note as stored (None once cleared); the outer None means an unknown id.
pub fn set_note(
    conn: &Connection,
    id: &str,
    note: &str,
) -> rusqlite::Result<Option<Option<String>>> {
    let note = Some(normalize_notes(note)).filter(|n| !n.is_empty());
    let hit = conn.execute(
        "UPDATE riven_sessions SET notes = ?2 WHERE id = ?1",
        params![id, note],
    )?;
    Ok((hit > 0).then_some(note))
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool) -> rusqlite::Result<bool> {
    Ok(conn.execute(
        "UPDATE riven_sessions SET pinned = ?2 WHERE id = ?1",
        params![id, pinned],
    )? > 0)
}

pub fn delete(conn: &Connection, id: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute("DELETE FROM riven_sessions WHERE id = ?1", [id])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_conn;

    fn session(id: &str, at: i64) -> RivenRollSession {
        RivenRollSession {
            id: id.into(),
            weapon: "Braton Prime".into(),
            rolls: 3,
            kuva_spent: 3600,
            started_at: at as f64,
            ended_at: at as f64 + 5.0,
            notes: None,
            pinned: false,
        }
    }

    #[test]
    fn lists_pinned_first_then_newest_and_ignores_a_duplicate_save() {
        let c = mem_conn();
        for (id, at) in [("a", 1), ("b", 2), ("c", 3)] {
            save(&c, &session(id, at)).unwrap();
        }
        save(&c, &session("a", 1)).unwrap();
        assert!(set_pinned(&c, "a", true).unwrap());
        let ids: Vec<_> = list(&c).unwrap().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, ["a", "c", "b"]);
        assert!(!set_pinned(&c, "zzz", true).unwrap());
    }

    #[test]
    fn notes_are_normalized_cleared_and_unknown_ids_reported() {
        let c = mem_conn();
        save(&c, &session("a", 1)).unwrap();
        assert_eq!(
            set_note(&c, "a", "  keep\r\nthis ").unwrap(),
            Some(Some("keep\nthis".into()))
        );
        assert_eq!(set_note(&c, "a", "  ").unwrap(), Some(None));
        assert_eq!(set_note(&c, "nope", "x").unwrap(), None);
        assert!(delete(&c, "a").unwrap() && list(&c).unwrap().is_empty());
    }
}
