//! Changes applied on top of an imported snapshot: exact ones from EE.log trades, estimated ones
//! from relic rewards the user confirmed. `apply` is pure; the snapshot itself is never edited.

use serde_json::{Value, json};

use crate::services::gdpr_import::{Direction, TradeEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaKind {
    Trade,
    Relic,
}

impl DeltaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::Relic => "relic",
        }
    }

    pub fn parse(s: &str) -> Self {
        if s == "relic" {
            Self::Relic
        } else {
            Self::Trade
        }
    }
}

/// One row: an item count change (`unique_name` + signed `count`) or a platinum change.
#[derive(Clone, Debug, PartialEq)]
pub struct Delta {
    /// Epoch milliseconds.
    pub at: f64,
    pub kind: DeltaKind,
    /// Rows of one trade or pick share it.
    pub group: String,
    /// `None` when the item could not be resolved to a game path; kept for the record, not applied.
    pub unique_name: Option<String>,
    pub label: String,
    pub count: i64,
    pub platinum: i64,
    pub estimated: bool,
}

/// Rows for a trade: items by their resolved game path, platinum as its own row.
pub fn from_trade(e: &TradeEvent, at: f64) -> Vec<Delta> {
    let row = |label: &str, unique_name: Option<String>, count, platinum| Delta {
        at,
        kind: DeltaKind::Trade,
        group: e.id.clone(),
        unique_name,
        label: label.to_owned(),
        count,
        platinum,
        estimated: false,
    };
    let mut out: Vec<Delta> = e
        .items
        .iter()
        // Platinum travels as `plat_change`, never as an item.
        .filter(|i| !i.display_name.eq_ignore_ascii_case("platinum"))
        .map(|i| {
            let sign = if i.direction == Direction::Given {
                -1
            } else {
                1
            };
            let path = Some(i.internal_name.clone()).filter(|p| p.starts_with('/'));
            row(&i.display_name, path, sign * i64::from(i.count), 0)
        })
        .collect();
    if e.plat_change != 0 {
        out.push(row("Platinum", None, 0, e.plat_change));
    }
    out
}

/// The relic reward the user took: one estimated copy.
pub fn from_pick(unique_name: &str, label: &str, at: f64) -> Delta {
    Delta {
        at,
        kind: DeltaKind::Relic,
        group: format!("pick-{at}"),
        unique_name: Some(unique_name.to_owned()),
        label: label.to_owned(),
        count: 1,
        platinum: 0,
        estimated: true,
    }
}

const COUNTED: [&str; 3] = ["MiscItems", "Recipes", "RawUpgrades"];

fn count_of(row: &Value) -> i64 {
    row.get("ItemCount").and_then(Value::as_i64).unwrap_or(1)
}

fn bump(inv: &mut Value, name: &str, by: i64) {
    for key in COUNTED {
        let Some(rows) = inv.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        if let Some(i) = rows
            .iter()
            .position(|r| r.get("ItemType").and_then(Value::as_str) == Some(name))
        {
            let next = count_of(&rows[i]) + by;
            if next <= 0 {
                rows.remove(i);
            } else {
                rows[i]["ItemCount"] = json!(next);
            }
            return;
        }
    }
    if by <= 0 {
        return;
    }
    let key = if name.contains("/Types/Recipes/") {
        "Recipes"
    } else if name.contains("/Upgrades/") {
        "RawUpgrades"
    } else {
        "MiscItems"
    };
    if let Some(obj) = inv.as_object_mut() {
        let rows = obj.entry(key).or_insert_with(|| json!([]));
        if let Some(rows) = rows.as_array_mut() {
            rows.push(json!({"ItemType": name, "ItemCount": by}));
        }
    }
}

/// The snapshot with every resolvable delta applied (platinum floors at 0).
pub fn apply(snapshot: &Value, deltas: &[Delta]) -> Value {
    let mut inv = snapshot.clone();
    for d in deltas {
        if let Some(name) = d.unique_name.as_deref().filter(|_| d.count != 0) {
            bump(&mut inv, name, d.count);
        }
        if d.platinum != 0
            && let Some(plat) = inv.get("PremiumCredits").and_then(Value::as_i64)
        {
            inv["PremiumCredits"] = json!((plat + d.platinum).max(0));
        }
    }
    inv
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gdpr_import::{TradeItem, TradeType};

    fn item(name: &str, path: &str, count: u32, direction: Direction) -> TradeItem {
        TradeItem {
            internal_name: path.into(),
            display_name: name.into(),
            count,
            direction,
            wfm_slug: None,
            wfm_thumb: None,
        }
    }

    fn trade(items: Vec<TradeItem>, plat: i64) -> TradeEvent {
        TradeEvent {
            id: "t1".into(),
            date: String::new(),
            kind: TradeType::Trade,
            plat_change: plat,
            items,
            partner: None,
            schema_version: None,
            source: None,
            source_record_id: None,
            import_batch_id: None,
            wfm_closed: None,
            credits: None,
            trade_tax: None,
        }
    }

    #[test]
    fn a_trade_moves_counts_and_platinum_and_skips_unresolved_names() {
        let snap = json!({
            "PremiumCredits": 100,
            "MiscItems": [{"ItemType": "/Lotus/A", "ItemCount": 2}],
            "Recipes": [],
        });
        let e = trade(
            vec![
                item("A", "/Lotus/A", 2, Direction::Given),
                item("B", "/Lotus/Types/Recipes/B", 1, Direction::Received),
                item("Mystery", "", 3, Direction::Received),
            ],
            50,
        );
        let out = apply(&snap, &from_trade(&e, 1.0));
        assert_eq!(out["MiscItems"], json!([]));
        assert_eq!(
            out["Recipes"],
            json!([{"ItemType": "/Lotus/Types/Recipes/B", "ItemCount": 1}])
        );
        assert_eq!(out["PremiumCredits"], 150);
        assert_eq!(snap["PremiumCredits"], 100);
    }

    #[test]
    fn a_pick_adds_one_estimated_copy_and_counts_never_go_negative() {
        let snap = json!({"MiscItems": [{"ItemType": "/Lotus/X", "ItemCount": 1}]});
        let mut gone = from_pick("/Lotus/X", "X", 1.0);
        gone.count = -5;
        let out = apply(&snap, &[from_pick("/Lotus/Y", "Y", 1.0), gone]);
        assert_eq!(
            out["MiscItems"],
            json!([{"ItemType": "/Lotus/Y", "ItemCount": 1}])
        );
        assert!(from_pick("/Lotus/Y", "Y", 1.0).estimated);
    }
}
