//! Recorded missions (tables `mission_rewards`, `mission_reward_items`), newest first on read.
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, params, params_from_iter, types::Value as Sql};

use super::{
    MissionRewardItem, MissionRewardSummary, MissionRewardsPage, MissionRewardsQuery,
    MissionRewardsStatus, MissionRewardsTotals,
};

pub const RECENT_LIMIT: u32 = 10;
pub const MAX_PAGE_SIZE: u32 = 200;
const MAX_QUERY_NAMES: usize = 5_000;
const MAX_MISSIONS: i64 = 10_000;
const MAX_ITEMS_PER_SUMMARY: usize = 5_000;
const MAX_UNIQUE_NAME_CHARS: usize = 256;
const MAX_QUERY_OFFSET: i64 = 10_000_000;

static NODE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_]{1,64}$").unwrap());
static MISSION_TYPE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^MT_[A-Z_]{1,40}$").unwrap());

fn is_unique_name(n: &str) -> bool {
    n.starts_with('/') && n.chars().count() <= MAX_UNIQUE_NAME_CHARS
}

/// The checks the store applies, so a restart shows a mission exactly as recorded.
fn validate(s: &MissionRewardSummary) -> Option<MissionRewardSummary> {
    let ok = |n: f64| n.is_finite();
    if s.id.is_empty() || s.id.chars().count() > 64 || s.mission_count == 0 {
        return None;
    }
    if !ok(s.ended_at) || !ok(s.read_at) || !ok(s.credits) || !ok(s.endo) {
        return None;
    }
    if s.credits < 0.0 || s.endo < 0.0 {
        return None;
    }
    Some(MissionRewardSummary {
        mission_type: s
            .mission_type
            .clone()
            .filter(|t| MISSION_TYPE_ID.is_match(t)),
        node: s.node.clone().filter(|n| NODE_ID.is_match(n)),
        baseline_at: s.baseline_at.filter(|b| ok(*b) && *b > 0.0),
        items: s
            .items
            .iter()
            .take(MAX_ITEMS_PER_SUMMARY)
            .filter(|i| i.count > 0 && is_unique_name(&i.unique_name))
            .cloned()
            .collect(),
        ..s.clone()
    })
}

/// False when the summary fails validation or its id is already recorded (the first one stays).
pub fn append(conn: &Connection, summary: &MissionRewardSummary) -> rusqlite::Result<bool> {
    let Some(s) = validate(summary) else {
        tracing::warn!(
            "[MissionRewards] {} not recorded: its summary fails validation",
            summary.id
        );
        return Ok(false);
    };
    let added = conn.execute(
        "INSERT OR IGNORE INTO mission_rewards (id, ended_at, read_at, mission_count, mission_type, node, credits, endo, baseline_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![s.id, s.ended_at, s.read_at, s.mission_count, s.mission_type, s.node, s.credits, s.endo, s.baseline_at],
    )?;
    if added == 0 {
        return Ok(false);
    }
    let mut ins = conn.prepare_cached(
        "INSERT INTO mission_reward_items (mission_id, unique_name, count) VALUES (?1, ?2, ?3)
         ON CONFLICT (mission_id, unique_name) DO UPDATE SET count = count + excluded.count",
    )?;
    for i in &s.items {
        ins.execute(params![s.id, i.unique_name, i.count])?;
    }
    let dropped = conn.execute(
        "DELETE FROM mission_rewards WHERE seq <= (SELECT seq FROM mission_rewards ORDER BY seq DESC LIMIT 1 OFFSET ?1)",
        [MAX_MISSIONS],
    )?;
    if dropped > 0 {
        conn.execute(
            "DELETE FROM mission_reward_items WHERE mission_id NOT IN (SELECT id FROM mission_rewards)",
            [],
        )?;
    }
    Ok(true)
}

fn items_of(conn: &Connection, id: &str) -> rusqlite::Result<Vec<MissionRewardItem>> {
    let mut st = conn.prepare_cached(
        "SELECT unique_name, count FROM mission_reward_items WHERE mission_id = ?1 ORDER BY unique_name",
    )?;
    st.query_map([id], |r| {
        Ok(MissionRewardItem {
            unique_name: r.get(0)?,
            count: r.get(1)?,
        })
    })?
    .collect()
}

fn summaries(
    conn: &Connection,
    clause: &str,
    args: Vec<Sql>,
) -> rusqlite::Result<Vec<MissionRewardSummary>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, ended_at, read_at, mission_count, mission_type, node, credits, endo, baseline_at
         FROM mission_rewards m {clause}"
    ))?;
    let rows = st
        .query_map(params_from_iter(args), |r| {
            Ok(MissionRewardSummary {
                id: r.get(0)?,
                ended_at: r.get(1)?,
                read_at: r.get(2)?,
                mission_count: r.get(3)?,
                mission_type: r.get(4)?,
                node: r.get(5)?,
                items: vec![],
                credits: r.get(6)?,
                endo: r.get(7)?,
                baseline_at: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|mut s| {
            s.items = items_of(conn, &s.id)?;
            Ok(s)
        })
        .collect()
}

/// Newest first.
pub fn recent(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<MissionRewardSummary>> {
    summaries(conn, "ORDER BY seq DESC LIMIT ?1", vec![Sql::from(limit)])
}

/// Untrusted renderer input to a bounded query.
pub fn normalize_query(q: MissionRewardsQuery) -> Result<MissionRewardsQuery, String> {
    if q.offset < 0.0 || q.offset.fract() != 0.0 || q.limit < 1.0 || q.limit.fract() != 0.0 {
        return Err("mission page query rejected".into());
    }
    Ok(MissionRewardsQuery {
        offset: q.offset.min(MAX_QUERY_OFFSET as f64),
        limit: q.limit.min(f64::from(MAX_PAGE_SIZE)),
        since: q.since.filter(|n| n.is_finite()),
        today_since: q.today_since.filter(|n| n.is_finite()),
        mission_type: q.mission_type.filter(|t| MISSION_TYPE_ID.is_match(t)),
        unique_names: q.unique_names.map(|names| {
            names
                .into_iter()
                .take(MAX_QUERY_NAMES)
                .filter(|n| is_unique_name(n))
                .collect()
        }),
    })
}

/// Newest first; totals cover every match, the summaries only the requested slice. The caller
/// supplies the status.
pub fn query(
    conn: &Connection,
    q: &MissionRewardsQuery,
    status: MissionRewardsStatus,
) -> rusqlite::Result<MissionRewardsPage> {
    let mut conds: Vec<String> = vec![];
    let mut args: Vec<Sql> = vec![];
    if let Some(since) = q.since {
        conds.push("m.ended_at >= ?".into());
        args.push(since.into());
    }
    if let Some(t) = &q.mission_type {
        conds.push("m.mission_type = ?".into());
        args.push(t.clone().into());
    }
    if let Some(names) = &q.unique_names {
        // A mission matches when it received any of these; no names match nothing.
        if names.is_empty() {
            conds.push("0".into());
        } else {
            let marks = vec!["?"; names.len()].join(",");
            conds.push(format!(
                "EXISTS (SELECT 1 FROM mission_reward_items i WHERE i.mission_id = m.id AND i.unique_name IN ({marks}))"
            ));
            args.extend(names.iter().map(|n| Sql::from(n.clone())));
        }
    }
    let wher = if conds.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conds.join(" AND "))
    };

    let (matched, missions, credits, endo) = conn.query_row(
        &format!(
            "SELECT count(*), coalesce(sum(mission_count), 0), coalesce(sum(credits), 0), coalesce(sum(endo), 0)
             FROM mission_rewards m {wher}"
        ),
        params_from_iter(args.clone()),
        |r| Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let items = conn
        .prepare(&format!(
            "SELECT i.unique_name, sum(i.count) FROM mission_reward_items i JOIN mission_rewards m ON m.id = i.mission_id
             {wher} GROUP BY i.unique_name ORDER BY 2 DESC, 1"
        ))?
        .query_map(params_from_iter(args.clone()), |r| {
            Ok(MissionRewardItem {
                unique_name: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut page_args = args;
    page_args.extend([Sql::from(q.limit as i64), Sql::from(q.offset as i64)]);
    let page = summaries(
        conn,
        &format!("{wher} ORDER BY seq DESC LIMIT ? OFFSET ?"),
        page_args,
    )?;

    let today = match q.today_since {
        Some(t) => conn.query_row(
            "SELECT coalesce(sum(mission_count), 0) FROM mission_rewards WHERE ended_at >= ?1",
            [t],
            |r| r.get(0),
        )?,
        None => 0,
    };
    let strings = |sql: &str| -> rusqlite::Result<Vec<String>> {
        conn.prepare(sql)?.query_map([], |r| r.get(0))?.collect()
    };
    Ok(MissionRewardsPage {
        summaries: page,
        matched,
        totals: MissionRewardsTotals {
            missions,
            credits,
            endo,
            items,
        },
        latest: recent(conn, 1)?.pop(),
        recorded: conn.query_row("SELECT count(*) FROM mission_rewards", [], |r| r.get(0))?,
        today,
        mission_types: strings(
            "SELECT DISTINCT mission_type FROM mission_rewards WHERE mission_type IS NOT NULL ORDER BY 1",
        )?,
        item_types: strings("SELECT DISTINCT unique_name FROM mission_reward_items ORDER BY 1")?,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::mem_conn;
    use crate::services::mission_rewards::{MissionRewardsPhase, MissionRewardsStatus};

    const A: &str = "/Lotus/Types/Items/MiscItems/Plastids";
    const B: &str = "/Lotus/Types/Game/Projections/T1VoidProjectionTestBronze";

    fn status() -> MissionRewardsStatus {
        MissionRewardsStatus {
            phase: MissionRewardsPhase::Idle,
            blocked: None,
            last_failure: None,
            pending_missions: 0,
        }
    }

    fn mission(n: u32, kind: Option<&str>, items: &[(&str, u32)]) -> MissionRewardSummary {
        MissionRewardSummary {
            id: format!("m{n}"),
            ended_at: f64::from(n) * 1000.0,
            read_at: f64::from(n) * 1000.0 + 3000.0,
            mission_count: 1,
            mission_type: kind.map(Into::into),
            node: kind.map(|_| "SolNode25".into()),
            items: items
                .iter()
                .map(|(u, c)| MissionRewardItem {
                    unique_name: (*u).into(),
                    count: *c,
                })
                .collect(),
            credits: f64::from(n) * 10.0,
            endo: 5.0,
            baseline_at: Some(1.0),
        }
    }

    fn q() -> MissionRewardsQuery {
        MissionRewardsQuery {
            offset: 0.0,
            limit: 50.0,
            since: None,
            today_since: None,
            mission_type: None,
            unique_names: None,
        }
    }

    fn ids(p: &MissionRewardsPage) -> Vec<&str> {
        p.summaries.iter().map(|s| s.id.as_str()).collect()
    }

    fn seed(c: &Connection) {
        append(c, &mission(1, Some("MT_SURVIVAL"), &[(A, 4)])).unwrap();
        append(c, &mission(2, Some("MT_DEFENSE"), &[(A, 1), (B, 1)])).unwrap();
        append(c, &mission(3, None, &[(B, 2)])).unwrap();
    }

    #[test]
    fn a_recorded_mission_reads_back_exactly_as_recorded() {
        let c = mem_conn();
        let m = mission(1, Some("MT_SURVIVAL"), &[(A, 4), (B, 1)]);
        assert!(append(&c, &m).unwrap());
        let mut want = m;
        want.items.sort_by(|a, b| a.unique_name.cmp(&b.unique_name));
        assert_eq!(recent(&c, 10).unwrap(), vec![want]);
    }

    #[test]
    fn keeps_the_first_of_two_missions_with_the_same_id() {
        let c = mem_conn();
        append(&c, &mission(1, None, &[(A, 4)])).unwrap();
        assert!(!append(&c, &mission(1, None, &[(A, 9)])).unwrap());
        let got = recent(&c, 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].items[0].count, 4);
    }

    #[test]
    fn drops_the_oldest_mission_past_the_cap_and_its_items() {
        let c = mem_conn();
        for n in 0..(MAX_MISSIONS as u32 + 2) {
            let item = format!("/Lotus/Only/{n}");
            append(&c, &mission(n + 1, None, &[(item.as_str(), 1)])).unwrap();
        }
        let p = query(&c, &q(), status()).unwrap();
        assert_eq!(p.recorded, MAX_MISSIONS as u32);
        assert_eq!(p.item_types.len(), MAX_MISSIONS as usize);
        assert!(!p.item_types.contains(&"/Lotus/Only/0".to_string()));
        assert_eq!(
            recent(&c, 1).unwrap()[0].id,
            format!("m{}", MAX_MISSIONS + 2)
        );
    }

    #[test]
    fn rejects_summaries_that_fail_validation_and_cleans_odd_fields() {
        let c = mem_conn();
        let bad = |f: fn(&mut MissionRewardSummary)| {
            let mut m = mission(1, None, &[(A, 1)]);
            f(&mut m);
            assert!(!append(&c, &m).unwrap());
        };
        bad(|m| m.id = String::new());
        bad(|m| m.id = "x".repeat(65));
        bad(|m| m.mission_count = 0);
        bad(|m| m.credits = -1.0);
        bad(|m| m.endo = f64::NAN);
        bad(|m| m.ended_at = f64::INFINITY);
        assert_eq!(query(&c, &q(), status()).unwrap().recorded, 0);

        let mut m = mission(2, Some("survival"), &[(A, 1), ("no-slash", 3), (B, 0)]);
        m.node = Some("Sol Node!".into());
        m.baseline_at = Some(0.0);
        assert!(append(&c, &m).unwrap());
        let got = &recent(&c, 1).unwrap()[0];
        assert_eq!(
            (got.mission_type.clone(), got.node.clone(), got.baseline_at),
            (None, None, None)
        );
        assert_eq!(got.items.len(), 1);
    }

    #[test]
    fn totals_every_match_and_pages_newest_first() {
        let c = mem_conn();
        seed(&c);
        let p = query(&c, &MissionRewardsQuery { limit: 2.0, ..q() }, status()).unwrap();
        assert_eq!((p.matched, p.recorded), (3, 3));
        assert_eq!(ids(&p), ["m3", "m2"]);
        assert_eq!(p.latest.as_ref().unwrap().id, "m3");
        assert_eq!(p.totals.missions, 3);
        assert_eq!(p.totals.credits, 60.0);
        assert_eq!(p.totals.endo, 15.0);
        assert_eq!(
            p.totals
                .items
                .iter()
                .map(|i| (i.unique_name.as_str(), i.count))
                .collect::<Vec<_>>(),
            [(A, 5), (B, 3)]
        );
        assert_eq!(p.mission_types, ["MT_DEFENSE", "MT_SURVIVAL"]);
        assert_eq!(p.item_types, [B, A]);
        let next = query(
            &c,
            &MissionRewardsQuery {
                offset: 2.0,
                limit: 2.0,
                ..q()
            },
            status(),
        )
        .unwrap();
        assert_eq!(ids(&next), ["m1"]);
    }

    #[test]
    fn filters_by_start_mission_type_and_received_items_leaving_the_latest_alone() {
        let c = mem_conn();
        seed(&c);
        let since = query(
            &c,
            &MissionRewardsQuery {
                since: Some(2000.0),
                ..q()
            },
            status(),
        )
        .unwrap();
        assert_eq!(ids(&since), ["m3", "m2"]);
        let kind = query(
            &c,
            &MissionRewardsQuery {
                mission_type: Some("MT_DEFENSE".into()),
                ..q()
            },
            status(),
        )
        .unwrap();
        assert_eq!(
            (ids(&kind), kind.latest.as_ref().unwrap().id.as_str()),
            (vec!["m2"], "m3")
        );
        let named = query(
            &c,
            &MissionRewardsQuery {
                unique_names: Some(vec![B.into()]),
                ..q()
            },
            status(),
        )
        .unwrap();
        assert_eq!(ids(&named), ["m3", "m2"]);
        let none = query(
            &c,
            &MissionRewardsQuery {
                unique_names: Some(vec![]),
                ..q()
            },
            status(),
        )
        .unwrap();
        assert_eq!((none.matched, none.recorded), (0, 3));
    }

    #[test]
    fn counts_todays_missions_whatever_the_other_filters_match() {
        let c = mem_conn();
        seed(&c);
        let mut carried = mission(4, None, &[]);
        carried.mission_count = 3;
        append(&c, &carried).unwrap();
        let p = query(
            &c,
            &MissionRewardsQuery {
                today_since: Some(3000.0),
                mission_type: Some("MT_DEFENSE".into()),
                ..q()
            },
            status(),
        )
        .unwrap();
        assert_eq!((p.today, p.matched), (4, 1));
        assert_eq!(query(&c, &q(), status()).unwrap().today, 0);
    }

    #[test]
    fn bounds_renderer_input_and_drops_what_it_cannot_use() {
        let n = normalize_query(MissionRewardsQuery {
            offset: 1e12,
            limit: 9999.0,
            since: Some(f64::NAN),
            mission_type: Some("survival".into()),
            unique_names: Some(vec![A.into(), "nope".into(), "/".repeat(300)]),
            ..q()
        })
        .unwrap();
        assert_eq!(
            (n.offset, n.limit, n.since, n.mission_type),
            (MAX_QUERY_OFFSET as f64, 200.0, None, None)
        );
        assert_eq!(n.unique_names, Some(vec![A.to_string()]));
        for (offset, limit) in [(-1.0, 5.0), (0.5, 5.0), (0.0, 0.0)] {
            assert!(
                normalize_query(MissionRewardsQuery {
                    offset,
                    limit,
                    ..q()
                })
                .is_err()
            );
        }
    }
}
