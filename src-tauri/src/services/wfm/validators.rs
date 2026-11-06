//! IPC trust boundary for orders (port of WFHelper `ipc/wfmValidators.ts`). Every payload is a typed
//! struct that refuses unknown fields; `validated()` then enforces ranges (`docs/wfm-api.md` §5) and
//! normalises case. Riven subtypes `unrevealed`/`revealed` (PR #57) are accepted on create AND update.

use serde::Deserialize;

use super::errors::WfmError;

pub const MAX_PLATINUM: u32 = 900_000;
pub const MAX_QUANTITY: u32 = 9_999;
pub const MAX_RANK: u32 = 20;
pub const MAX_BULK_IDS: usize = 200;

pub const ORDER_SUBTYPES: [&str; 4] = ["intact", "exceptional", "flawless", "radiant"];
pub const MOD_VARIANTS: [&str; 2] = ["regular", "atragraph"];
pub const RIVEN_SUBTYPES: [&str; 2] = ["unrevealed", "revealed"];

fn bad(what: &str) -> WfmError {
    WfmError::invalid(format!("invalid order: {what}"))
}

/// 24 hex characters: a WFM object id. Also what keeps an id safe to put in a URL path.
pub fn valid_id(id: &str) -> bool {
    id.len() == 24 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn id(v: &str, field: &str) -> Result<String, WfmError> {
    let v = v.trim();
    if valid_id(v) {
        Ok(v.to_owned())
    } else {
        Err(bad(field))
    }
}

fn range(v: u32, min: u32, max: u32, field: &str) -> Result<u32, WfmError> {
    if (min..=max).contains(&v) {
        Ok(v)
    } else {
        Err(bad(&format!("{field} must be {min}-{max}")))
    }
}

pub fn subtype(v: &str) -> Result<String, WfmError> {
    let s = v.trim().to_lowercase();
    let known = ORDER_SUBTYPES
        .iter()
        .chain(&MOD_VARIANTS)
        .chain(&RIVEN_SUBTYPES)
        .any(|k| *k == s);
    if known { Ok(s) } else { Err(bad("subtype")) }
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateOrder {
    pub item_id: String,
    /// `sell` or `buy`, any case.
    pub order_type: String,
    pub platinum: u32,
    pub quantity: u32,
    pub visible: Option<bool>,
    pub mod_rank: Option<u32>,
    pub per_trade: Option<u32>,
    pub subtype: Option<String>,
}

impl CreateOrder {
    pub fn validated(self) -> Result<Self, WfmError> {
        let order_type = self.order_type.trim().to_lowercase();
        if order_type != "sell" && order_type != "buy" {
            return Err(bad("orderType"));
        }
        let quantity = range(self.quantity, 1, MAX_QUANTITY, "quantity")?;
        Ok(Self {
            item_id: id(&self.item_id, "itemId")?,
            order_type,
            platinum: range(self.platinum, 1, MAX_PLATINUM, "platinum")?,
            quantity,
            visible: self.visible,
            mod_rank: self
                .mod_rank
                .map(|r| range(r, 0, MAX_RANK, "modRank"))
                .transpose()?,
            per_trade: self
                .per_trade
                .map(|p| range(p, 1, quantity, "perTrade"))
                .transpose()?,
            subtype: self.subtype.as_deref().map(subtype).transpose()?,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrderUpdates {
    pub platinum: Option<u32>,
    pub quantity: Option<u32>,
    pub visible: Option<bool>,
    pub mod_rank: Option<u32>,
    pub subtype: Option<String>,
}

impl OrderUpdates {
    pub fn validated(self) -> Result<Self, WfmError> {
        Ok(Self {
            platinum: self
                .platinum
                .map(|v| range(v, 1, MAX_PLATINUM, "platinum"))
                .transpose()?,
            quantity: self
                .quantity
                .map(|v| range(v, 1, MAX_QUANTITY, "quantity"))
                .transpose()?,
            visible: self.visible,
            mod_rank: self
                .mod_rank
                .map(|v| range(v, 0, MAX_RANK, "modRank"))
                .transpose()?,
            subtype: self.subtype.as_deref().map(subtype).transpose()?,
        })
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

pub fn platinum(v: u32) -> Result<u32, WfmError> {
    range(v, 1, MAX_PLATINUM, "platinum")
}

pub fn range_rank(r: u32) -> Result<u32, WfmError> {
    range(r, 0, MAX_RANK, "rank")
}

pub fn order_id(v: &str) -> Result<String, WfmError> {
    id(v, "orderId")
}

/// Close: whole numbers only, and when the caller knows the listing's `perTrade` the quantity must be
/// a multiple of it (WFM rejects anything else).
pub fn close_quantity(quantity: u32, per_trade: Option<u32>) -> Result<u32, WfmError> {
    let q = range(quantity, 1, MAX_QUANTITY, "quantity")?;
    match per_trade {
        Some(p) if p == 0 || q % p != 0 => Err(bad("quantity must be a multiple of perTrade")),
        _ => Ok(q),
    }
}

/// Bulk visibility: valid ids only, at most `MAX_BULK_IDS`; none left is an error.
pub fn bulk_ids(ids: &[String]) -> Result<Vec<String>, WfmError> {
    let ids: Vec<String> = ids
        .iter()
        .filter_map(|i| id(i, "orderIds").ok())
        .take(MAX_BULK_IDS)
        .collect();
    if ids.is_empty() {
        Err(bad("orderIds"))
    } else {
        Ok(ids)
    }
}

// ---- Riven auctions (v1, see `legacy_v1`) ----

pub const RIVEN_POLARITIES: [&str; 3] = ["madurai", "naramon", "vazarin"];
pub const MAX_RIVEN_ATTRIBUTES: usize = 4;
pub const MAX_NOTE_CHARS: usize = 500;

fn bad_auction(what: &str) -> WfmError {
    WfmError::invalid(format!("invalid riven auction: {what}"))
}

/// A WFM auction id (24 hex), the same shape as an order id.
pub fn auction_id(v: &str) -> Result<String, WfmError> {
    let v = v.trim();
    if valid_id(v) {
        Ok(v.to_owned())
    } else {
        Err(bad_auction("auctionId"))
    }
}

/// A riven weapon slug: it goes into a query string, so only what the catalog holds
/// (`dark_split_sword_(dual_swords)` included) gets through.
pub fn weapon_slug(v: &str) -> Result<String, WfmError> {
    let s = v.trim().to_lowercase();
    let ok = !s.is_empty()
        && s.len() <= 120
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_()-".contains(c));
    if ok {
        Ok(s)
    } else {
        Err(bad_auction("weaponSlug"))
    }
}

fn note(v: &str) -> Result<String, WfmError> {
    let v = v.trim();
    if v.chars().count() > MAX_NOTE_CHARS || v.chars().any(char::is_control) {
        return Err(bad_auction("description"));
    }
    Ok(v.to_owned())
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RivenAttributeInput {
    /// WFM attribute slug, e.g. `critical_chance`.
    pub url_name: String,
    /// Signed as the game shows it; a curse is negative.
    pub value: f64,
    pub positive: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RivenAuctionCreate {
    pub weapon_slug: String,
    pub riven_name: String,
    pub attributes: Vec<RivenAttributeInput>,
    pub rerolls: u32,
    pub mastery_level: u32,
    pub polarity: String,
    pub mod_rank: u32,
    /// `None` or 0: no buyout.
    pub buyout_price: Option<u32>,
    pub starting_price: u32,
    pub min_reputation: u32,
    pub is_private: bool,
    pub description: String,
}

impl RivenAuctionCreate {
    pub fn validated(self) -> Result<Self, WfmError> {
        let name = self.riven_name.trim().to_lowercase();
        if name.is_empty() || name.chars().count() > 64 || name.chars().any(char::is_control) {
            return Err(bad_auction("rivenName"));
        }
        if self.attributes.is_empty() || self.attributes.len() > MAX_RIVEN_ATTRIBUTES {
            return Err(bad_auction("attributes"));
        }
        let mut attributes = vec![];
        for a in self.attributes {
            let slug = a.url_name.trim().to_lowercase();
            let ok = !slug.is_empty()
                && slug.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                && a.value.is_finite();
            if !ok {
                return Err(bad_auction("attributes"));
            }
            attributes.push(RivenAttributeInput {
                url_name: slug,
                ..a
            });
        }
        let polarity = self.polarity.trim().to_lowercase();
        if !RIVEN_POLARITIES.contains(&polarity.as_str()) {
            return Err(bad_auction("polarity"));
        }
        Ok(Self {
            weapon_slug: weapon_slug(&self.weapon_slug)?,
            riven_name: name,
            attributes,
            rerolls: range(self.rerolls, 0, 999, "rerolls").map_err(|_| bad_auction("rerolls"))?,
            mastery_level: range(self.mastery_level, 8, 16, "masteryLevel")
                .map_err(|_| bad_auction("masteryLevel"))?,
            polarity,
            mod_rank: range(self.mod_rank, 0, 8, "modRank").map_err(|_| bad_auction("modRank"))?,
            buyout_price: self
                .buyout_price
                .map(|p| range(p, 0, MAX_PLATINUM, "buyoutPrice"))
                .transpose()
                .map_err(|_| bad_auction("buyoutPrice"))?,
            starting_price: range(self.starting_price, 1, MAX_PLATINUM, "startingPrice")
                .map_err(|_| bad_auction("startingPrice"))?,
            min_reputation: range(self.min_reputation, 0, MAX_PLATINUM, "minReputation")
                .map_err(|_| bad_auction("minReputation"))?,
            is_private: self.is_private,
            description: note(&self.description)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RivenAuctionUpdate {
    /// `None` or 0: no buyout.
    pub buyout_price: Option<u32>,
    /// `None` for a direct sale: it has no opening bid.
    pub starting_price: Option<u32>,
    pub min_reputation: u32,
    pub is_private: Option<bool>,
    pub description: String,
    /// Wins over `is_private` when set: hiding a listing is not marking it private.
    pub visible: Option<bool>,
}

impl RivenAuctionUpdate {
    pub fn validated(self) -> Result<Self, WfmError> {
        Ok(Self {
            buyout_price: self
                .buyout_price
                .map(|p| range(p, 0, MAX_PLATINUM, "buyoutPrice"))
                .transpose()
                .map_err(|_| bad_auction("buyoutPrice"))?,
            starting_price: self
                .starting_price
                .map(|p| range(p, 1, MAX_PLATINUM, "startingPrice"))
                .transpose()
                .map_err(|_| bad_auction("startingPrice"))?,
            min_reputation: range(self.min_reputation, 0, MAX_PLATINUM, "minReputation")
                .map_err(|_| bad_auction("minReputation"))?,
            is_private: self.is_private,
            description: note(&self.description)?,
            visible: self.visible,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaa";

    fn create(v: Value) -> Result<CreateOrder, WfmError> {
        serde_json::from_value::<CreateOrder>(v)
            .map_err(WfmError::invalid)?
            .validated()
    }

    fn update(v: Value) -> Result<OrderUpdates, WfmError> {
        serde_json::from_value::<OrderUpdates>(v)
            .map_err(WfmError::invalid)?
            .validated()
    }

    fn base() -> Value {
        json!({"itemId": ID, "orderType": "sell", "platinum": 10, "quantity": 1})
    }

    fn with(extra: Value) -> Value {
        let mut b = base();
        b.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        b
    }

    #[test]
    fn rejects_malformed_create() {
        let bad_id =
            json!({"itemId": "bad-id", "orderType": "sell", "platinum": 10, "quantity": 1});
        assert!(create(bad_id).is_err());
        assert!(create(with(json!({"orderType": "trade"}))).is_err());
        assert!(create(json!({"itemId": ID})).is_err());
    }

    #[test]
    fn ranges_follow_the_wfm_docs() {
        for (f, v, ok) in [
            ("platinum", 0, false),
            ("platinum", 1, true),
            ("platinum", 900_000, true),
            ("platinum", 900_001, false),
            ("quantity", 0, false),
            ("quantity", 9_999, true),
            ("quantity", 10_000, false),
            ("modRank", 20, true),
            ("modRank", 21, false),
        ] {
            assert_eq!(create(with(json!({ f: v }))).is_ok(), ok, "{f}={v}");
        }
        // A fractional or negative number cannot even be deserialized.
        assert!(create(with(json!({"platinum": 1.5}))).is_err());
        assert!(create(with(json!({"quantity": -1}))).is_err());
    }

    #[test]
    fn fields_outside_the_contract_are_refused() {
        assert!(create(with(json!({"charges": 3}))).is_err());
        assert!(update(json!({"platinum": 5, "perTrade": 2})).is_err());
    }

    #[test]
    fn per_trade_is_capped_by_quantity() {
        let ok = create(with(json!({"quantity": 6, "perTrade": 6}))).unwrap();
        assert_eq!(ok.per_trade, Some(6));
        assert!(create(with(json!({"quantity": 6, "perTrade": 7}))).is_err());
        assert!(create(with(json!({"perTrade": 0}))).is_err());
    }

    #[test]
    fn subtypes_cover_relic_mod_and_riven_on_create_and_update() {
        let o = create(with(json!({"subtype": "Radiant"}))).unwrap();
        assert_eq!(o.subtype.as_deref(), Some("radiant"));
        assert!(create(with(json!({"subtype": "shiny"}))).is_err());
        for s in ["regular", "atragraph"] {
            let o = create(with(json!({"subtype": s, "modRank": 10}))).unwrap();
            assert_eq!((o.subtype.as_deref(), o.mod_rank), (Some(s), Some(10)));
            let u = update(json!({"subtype": s, "modRank": 10})).unwrap();
            assert_eq!((u.subtype.as_deref(), u.mod_rank), (Some(s), Some(10)));
        }
        // PR #57: veiled rivens are priced by reveal state and carry no rank.
        for s in ["unrevealed", "revealed"] {
            let o = create(with(json!({"subtype": s}))).unwrap();
            assert_eq!((o.subtype.as_deref(), o.mod_rank), (Some(s), None));
            let u = update(json!({"subtype": s})).unwrap();
            assert_eq!(u.subtype.as_deref(), Some(s));
        }
        assert_eq!(create(base()).unwrap().subtype, None);
        let flawless = update(json!({"subtype": "flawless"})).unwrap();
        assert_eq!(flawless.subtype.as_deref(), Some("flawless"));
        assert!(update(json!({"subtype": "x"})).is_err());
    }

    #[test]
    fn update_validates_each_present_field() {
        assert!(update(json!({})).unwrap().is_empty());
        assert!(update(json!({"platinum": 0})).is_err());
        assert!(update(json!({"quantity": 10_000})).is_err());
        assert!(update(json!({"modRank": 21})).is_err());
        assert_eq!(
            update(json!({"visible": false})).unwrap().visible,
            Some(false)
        );
    }

    #[test]
    fn order_ids_are_24_hex() {
        assert!(order_id(ID).is_ok());
        for bad in [
            "not-an-id",
            &"a".repeat(25),
            "",
            "../../etc/passwd/x/xxxxxx",
        ] {
            assert!(order_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn close_quantity_bounds_and_per_trade_multiple() {
        assert_eq!(close_quantity(6, None).unwrap(), 6);
        for q in [0, 10_000] {
            assert!(close_quantity(q, None).is_err());
        }
        assert_eq!(close_quantity(6, Some(3)).unwrap(), 6);
        assert!(close_quantity(5, Some(3)).is_err());
        assert!(close_quantity(5, Some(0)).is_err());
    }

    #[test]
    fn bulk_ids_drop_invalid_and_cap() {
        assert_eq!(bulk_ids(&[ID.into(), "nope".into()]).unwrap(), [ID]);
        assert!(bulk_ids(&["nope".into()]).is_err());
        assert!(bulk_ids(&[]).is_err());
        assert_eq!(
            bulk_ids(&vec![ID.to_owned(); 300]).unwrap().len(),
            MAX_BULK_IDS
        );
    }
}
