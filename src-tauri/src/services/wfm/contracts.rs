//! Riven contracts (port of WFHelper `services/wfmContracts.ts`): the account's own riven auctions
//! and the normaliser every auction shape goes through, other users' search rows included, so own
//! and foreign listings are the same `WfmContract`.
//!
//! The routes live in `legacy_v1` and may vanish: when every route is gone the result is an empty
//! page with an `unavailable` reason instead of an error, so the order views keep working.

use std::sync::Mutex;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use super::client::WfmClient;
use super::errors::WfmError;
use super::legacy_v1::{self, CANDIDATES};
use super::profile_slug::{self, SlugCache};
use super::session::Session;
use super::stats_v1::num;
use super::types::asset_url;

const DEFAULT_PAGE: u32 = 1;
const DEFAULT_LIMIT: u32 = 40;
const MAX_LIMIT: u32 = 100;
/// 403 is here too (not in WFHelper): a revoked v1 must degrade, not break the view.
const SKIPPABLE: [u16; 7] = [301, 302, 303, 400, 403, 404, 405];

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(untagged)]
pub enum AttributeValue {
    Number(f64),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WfmContractAttribute {
    pub url_name: String,
    pub label: String,
    pub value: Option<AttributeValue>,
    pub positive: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WfmContract {
    pub id: String,
    pub item_name: String,
    pub item_id: Option<String>,
    pub item_url_name: Option<String>,
    pub weapon_url_name: Option<String>,
    pub riven_suffix: Option<String>,
    pub item_thumb: Option<String>,
    pub platinum: u32,
    pub buyout_platinum: Option<u32>,
    pub starting_platinum: Option<u32>,
    pub quantity: u32,
    pub visible: bool,
    pub mod_rank: Option<u32>,
    pub rerolls: Option<u32>,
    pub mastery_level: Option<u32>,
    pub polarity: Option<String>,
    pub minimal_reputation: Option<u32>,
    pub is_direct_sell: bool,
    pub listed_at: Option<String>,
    pub updated_at: Option<String>,
    pub note: Option<String>,
    pub stats: Vec<WfmContractAttribute>,
    pub listing_url: String,
    pub source_type: Option<String>,
    /// Another user's auction only (`/auctions/search`); own contracts carry none.
    pub seller: Option<String>,
    pub seller_status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WfmContractsPage {
    pub contracts: Vec<WfmContract>,
    pub page: u32,
    pub total_pages: Option<u32>,
    pub has_more: bool,
    /// Why there are no contracts when WFM no longer serves any route for them.
    pub unavailable: Option<String>,
}

/// Which route answered last time (tried first next time) and the account slug.
#[derive(Default)]
pub struct Contracts {
    resolved: Mutex<Option<&'static str>>,
    slug: SlugCache,
}

impl Contracts {}

/// First value that is neither missing nor null (JS `??`).
fn coalesce<'a>(values: &[Option<&'a Value>]) -> Option<&'a Value> {
    values.iter().flatten().copied().find(|v| !v.is_null())
}

/// First non-empty string (WFHelper `firstNonEmpty`).
fn first_str(values: &[Option<&Value>]) -> Option<String> {
    values.iter().flatten().find_map(|v| {
        let s = v.as_str()?.trim();
        (!s.is_empty()).then(|| s.to_owned())
    })
}

fn rounded(v: f64) -> u32 {
    v.abs().round().min(f64::from(u32::MAX)) as u32
}

fn count(values: &[Option<&Value>]) -> Option<u32> {
    coalesce(values)
        .and_then(num)
        .filter(|n| *n >= 0.0)
        .map(rounded)
}

fn title_from_slug(slug: &str) -> String {
    let mut out = String::new();
    let mut prev_word = false;
    for c in slug.replace('_', " ").chars() {
        out.push(if !prev_word && c.is_ascii_lowercase() {
            c.to_ascii_uppercase()
        } else {
            c
        });
        prev_word = c.is_alphanumeric();
    }
    out
}

fn iso(v: Option<&Value>) -> Option<String> {
    let s = first_str(&[v])?;
    DateTime::parse_from_rfc3339(&s).ok().map(|d| {
        d.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    })
}

fn normalize_attribute(raw: &Value) -> Option<WfmContractAttribute> {
    let attr = raw.as_object()?;
    let get = |k: &str| attr.get(k);
    let url_name = first_str(&[get("url_name"), get("urlName"), get("name")])
        .unwrap_or_else(|| "unknown".into());
    let label = first_str(&[get("display_name"), get("displayName")])
        .unwrap_or_else(|| title_from_slug(&url_name));
    let value = get("value").and_then(|v| match num(v) {
        Some(n) => Some(AttributeValue::Number(n)),
        None => first_str(&[Some(v)]).map(AttributeValue::Text),
    });
    let positive = ["positive", "is_positive"]
        .iter()
        .find_map(|k| get(k).and_then(Value::as_bool));
    Some(WfmContractAttribute {
        url_name,
        label,
        value,
        positive,
    })
}

/// One auction/contract row of any known shape -> `WfmContract`. `None` without an id.
pub fn normalize_contract(raw: &Value) -> Option<WfmContract> {
    let r = raw.as_object()?;
    let rv = |k: &str| r.get(k);
    let empty = Value::Null;
    let item = rv("item").filter(|v| v.is_object()).unwrap_or(&empty);
    let iv = |k: &str| item.get(k);
    let en = item.get("i18n").and_then(|i| i.get("en")).unwrap_or(&empty);
    let ev = |k: &str| en.get(k);

    let item_slug = first_str(&[iv("url_name"), rv("item_url_name")]);
    let weapon_slug = first_str(&[
        iv("weapon_url_name"),
        iv("weaponUrlName"),
        rv("weapon_url_name"),
        rv("weaponUrlName"),
    ]);
    let item_name = first_str(&[
        ev("item_name"),
        ev("itemName"),
        iv("item_name"),
        iv("itemName"),
        iv("weapon_name"),
        iv("weaponName"),
        rv("item_name"),
        rv("itemName"),
    ])
    .or_else(|| {
        weapon_slug
            .as_deref()
            .map(|w| format!("{} Riven", title_from_slug(w)))
    })
    .unwrap_or_else(|| "Riven Contract".into());
    let item_thumb = first_str(&[
        iv("thumb"),
        iv("icon"),
        iv("image"),
        rv("thumb"),
        rv("icon"),
    ])
    .as_deref()
    .and_then(asset_url);

    let buyout = coalesce(&[rv("buyout_price"), rv("buyoutPrice")]).and_then(num);
    let starting = coalesce(&[rv("starting_price"), rv("startingPrice")]).and_then(num);
    let listed = rv("platinum")
        .and_then(num)
        .or_else(|| rv("price").and_then(num))
        .or(buyout)
        .or(starting)
        .unwrap_or(0.0);

    let attributes = [iv("attributes"), rv("attributes")]
        .iter()
        .flatten()
        .find_map(|v| v.as_array())
        .map_or(&[][..], Vec::as_slice);

    let id = first_str(&[rv("id"), rv("_id"), rv("contract_id"), rv("contractId")])?;
    let direct_sell = rv("is_direct_sell") == Some(&Value::Bool(true))
        || rv("isDirectSell") == Some(&Value::Bool(true))
        || (buyout.is_some_and(|b| b > 0.0) && starting.is_none_or(|s| s <= 0.0));
    let owner = rv("owner");
    let owner_str = |k: &str| owner.and_then(|o| o.get(k));

    Some(WfmContract {
        listing_url: format!(
            "https://warframe.market/auction/{}",
            legacy_v1::urlencode(&id)
        ),
        id,
        item_name,
        item_id: first_str(&[iv("id"), rv("itemId")]),
        item_url_name: item_slug.or_else(|| weapon_slug.clone()),
        riven_suffix: weapon_slug
            .as_ref()
            .and_then(|_| first_str(&[iv("name"), rv("riven_name")])),
        weapon_url_name: weapon_slug,
        item_thumb,
        platinum: rounded(listed),
        buyout_platinum: buyout.map(rounded),
        starting_platinum: starting.map(rounded),
        quantity: rv("quantity").and_then(num).map_or(1, rounded).max(1),
        visible: rv("visible") != Some(&Value::Bool(false)),
        mod_rank: count(&[iv("mod_rank"), iv("rank"), rv("mod_rank"), rv("rank")]),
        rerolls: count(&[iv("re_rolls"), iv("reRolls"), rv("re_rolls"), rv("reRolls")]),
        mastery_level: count(&[
            iv("mastery_level"),
            iv("masteryLevel"),
            rv("mastery_level"),
            rv("masteryLevel"),
        ]),
        polarity: first_str(&[iv("polarity"), rv("polarity"), iv("mod_polarity")]),
        minimal_reputation: count(&[rv("minimal_reputation"), rv("minimalReputation")]),
        is_direct_sell: direct_sell,
        listed_at: iso(coalesce(&[rv("created_at"), rv("createdAt")])),
        updated_at: iso(coalesce(&[rv("updated_at"), rv("updatedAt")])),
        note: first_str(&[rv("note")]),
        stats: attributes.iter().filter_map(normalize_attribute).collect(),
        source_type: first_str(&[rv("type"), rv("contract_type"), rv("contractType")]),
        seller: first_str(&[owner_str("ingame_name")]),
        seller_status: first_str(&[owner_str("status")]).map(|s| s.to_lowercase()),
    })
}

struct Extracted {
    rows: Vec<Value>,
    page: u32,
    total_pages: Option<u32>,
    has_more: bool,
}

fn nonzero(values: &[Option<&Value>]) -> Option<u32> {
    values
        .iter()
        .flatten()
        .filter_map(|v| num(v))
        .find(|n| *n != 0.0)
        .map(rounded)
}

fn page_info(c: Option<&Value>, rows: Vec<Value>) -> Extracted {
    let get = |k: &str| c.and_then(|c| c.get(k));
    let page =
        nonzero(&[get("page"), get("current_page"), get("currentPage")]).unwrap_or(DEFAULT_PAGE);
    let total_pages = nonzero(&[
        get("total_pages"),
        get("totalPages"),
        get("last_page"),
        get("lastPage"),
    ]);
    let has_more = ["has_more", "hasMore"]
        .iter()
        .find_map(|k| get(k).and_then(Value::as_bool))
        .unwrap_or_else(|| total_pages.is_some_and(|t| page < t));
    Extracted {
        rows,
        page,
        total_pages,
        has_more,
    }
}

/// Rows and pagination from `{data|payload: …}` whichever of the known shapes it has.
fn extract_contracts(body: &Value) -> Extracted {
    let root = coalesce(&[body.get("data"), body.get("payload")]).unwrap_or(body);
    let truthy = |v: &&Value| !v.is_null() && v.as_bool() != Some(false);
    for candidate in [Some(root), root.get("data"), root.get("payload")]
        .into_iter()
        .flatten()
        .filter(truthy)
    {
        if let Some(rows) = candidate.as_array() {
            return page_info(Some(root), rows.clone());
        }
        for key in ["contracts", "auctions", "items", "results"] {
            if let Some(rows) = candidate.get(key).and_then(Value::as_array) {
                return page_info(Some(candidate), rows.clone());
            }
        }
    }
    page_info(None, vec![])
}

/// Every auction row of a search/own-contracts body, normalised.
pub fn normalize_all(body: &Value) -> Vec<WfmContract> {
    extract_contracts(body)
        .rows
        .iter()
        .filter_map(normalize_contract)
        .collect()
}

/// The account's riven auctions. `user` is the signed-in in-game name.
pub async fn my_contracts(
    client: &WfmClient,
    state: &Contracts,
    user: Option<&str>,
    page: Option<u32>,
    limit: Option<u32>,
) -> Result<WfmContractsPage, WfmError> {
    let (Some(user), true) = (user, client.token().is_some()) else {
        return Err(WfmError::Unauthorized {
            detail: "Not logged in to Warframe.market.".into(),
        });
    };
    let page = page.filter(|p| *p > 0).unwrap_or(DEFAULT_PAGE);
    let limit = limit
        .filter(|l| *l > 0)
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT);

    let first = *state.resolved.lock().unwrap();
    let mut order: Vec<_> = CANDIDATES.iter().collect();
    order.sort_by_key(|c| Some(c.name) != first);

    let (mut last_error, mut skipped) = (None, vec![]);
    // Resolving the slug costs a request of its own, so it waits until a route that needs it.
    let mut slug: Option<Option<String>> = None;
    for c in order {
        let slug_for_path = if c.needs_slug() {
            if slug.is_none() {
                slug = Some(profile_slug::own_slug(client, &state.slug, user).await);
            }
            match slug.as_ref().and_then(Option::as_deref) {
                Some(s) => s,
                None => continue,
            }
        } else {
            ""
        };
        match legacy_v1::get_candidate(client, c, slug_for_path, page, limit).await {
            Ok(body) => {
                let e = extract_contracts(&body);
                if first != Some(c.name) {
                    tracing::info!(route = c.name, "wfm contracts route resolved");
                }
                *state.resolved.lock().unwrap() = Some(c.name);
                let contracts = e.rows.iter().filter_map(normalize_contract).collect();
                return Ok(WfmContractsPage {
                    contracts,
                    page: e.page,
                    total_pages: e.total_pages,
                    has_more: e.has_more,
                    unavailable: None,
                });
            }
            Err(e @ WfmError::Unauthorized { .. }) => return Err(e),
            Err(WfmError::Api { status, .. }) if SKIPPABLE.contains(&status) => {
                tracing::info!(route = c.name, status, "wfm contracts route unavailable");
                let mut r = state.resolved.lock().unwrap();
                if *r == Some(c.name) {
                    *r = None;
                }
                skipped.push(status);
            }
            Err(e) => {
                tracing::warn!(route = c.name, error = %e, "wfm contracts route failed");
                last_error = Some(e);
            }
        }
    }
    if let Some(e) = last_error {
        return Err(e);
    }
    let reason = if skipped.is_empty() {
        "no contracts route could be addressed".to_owned()
    } else {
        let codes: Vec<_> = skipped.iter().map(u16::to_string).collect();
        format!(
            "warframe.market no longer serves riven contracts (HTTP {})",
            codes.join(", ")
        )
    };
    Ok(WfmContractsPage {
        contracts: vec![],
        page,
        total_pages: None,
        has_more: false,
        unavailable: Some(reason),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_contracts_my(
    session: State<'_, Session>,
    contracts: State<'_, Contracts>,
    page: Option<u32>,
    limit: Option<u32>,
) -> Result<WfmContractsPage, WfmError> {
    let user = session.status().user_name;
    my_contracts(session.client(), &contracts, user.as_deref(), page, limit).await
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::mock::{Mock, Reply, reply, spawn};
    use super::*;

    #[test]
    fn normalizes_v2_contract_rows_and_pagination() {
        let body = json!({"data": {"page": 2, "total_pages": 4, "has_more": true, "contracts": [{
            "id": "65f6d1f9f8f87e16d88c7a11", "platinum": 451, "buyout_price": 800,
            "starting_price": 450, "quantity": 2, "visible": true,
            "created_at": "2026-03-04T10:00:00.000Z", "updated_at": "2026-03-04T10:30:00.000Z",
            "item": {
                "id": "65f6d1f9f8f87e16d88c7b99", "name": "visio-critatis", "url_name": "riven_mod",
                "weapon_url_name": "latron_prime",
                "thumb": "items/images/en/thumbs/latron_prime_riven_mod.png",
                "i18n": {"en": {"item_name": "Latron Prime Riven Mod"}},
                "attributes": [
                    {"url_name": "critical_chance", "display_name": "Critical Chance", "value": 88.7, "positive": true},
                    {"url_name": "weapon_recoil", "value": -52.1, "is_positive": false}
                ]
            }
        }]}});
        let e = extract_contracts(&body);
        assert_eq!(
            (e.page, e.total_pages, e.has_more, e.rows.len()),
            (2, Some(4), true, 1)
        );
        let c = &normalize_all(&body)[0];
        assert_eq!(
            (
                c.id.as_str(),
                c.item_name.as_str(),
                c.item_url_name.as_deref()
            ),
            (
                "65f6d1f9f8f87e16d88c7a11",
                "Latron Prime Riven Mod",
                Some("riven_mod")
            )
        );
        assert_eq!(c.item_id.as_deref(), Some("65f6d1f9f8f87e16d88c7b99"));
        assert_eq!(c.weapon_url_name.as_deref(), Some("latron_prime"));
        assert_eq!(c.riven_suffix.as_deref(), Some("visio-critatis"));
        assert_eq!(
            (
                c.platinum,
                c.buyout_platinum,
                c.starting_platinum,
                c.quantity
            ),
            (451, Some(800), Some(450), 2)
        );
        assert!(c.visible && !c.is_direct_sell);
        assert_eq!(
            c.listing_url,
            "https://warframe.market/auction/65f6d1f9f8f87e16d88c7a11"
        );
        assert_eq!(
            c.item_thumb,
            asset_url("items/images/en/thumbs/latron_prime_riven_mod.png")
        );
        assert_eq!(c.listed_at.as_deref(), Some("2026-03-04T10:00:00.000Z"));
        assert_eq!(c.updated_at.as_deref(), Some("2026-03-04T10:30:00.000Z"));
        let attr = |u: &str, l: &str, v: f64, p| WfmContractAttribute {
            url_name: u.into(),
            label: l.into(),
            value: Some(AttributeValue::Number(v)),
            positive: Some(p),
        };
        assert_eq!(
            c.stats,
            [
                attr("critical_chance", "Critical Chance", 88.7, true),
                attr("weapon_recoil", "Weapon Recoil", -52.1, false),
            ]
        );
    }

    #[test]
    fn normalizes_the_profile_auctions_shape_with_fallback_page_fields() {
        let body = json!({"payload": {"auctions": [{
            "_id": "65f6d1f9f8f87e16d88c7a22", "price": "600", "quantity": 1, "visible": false,
            "contract_type": "auction", "item_url_name": "riven_mod", "weapon_url_name": "boar_prime",
            "weapon_name": "Boar Prime Riven",
            "thumb": "https://warframe.market/static/assets/boar_prime_riven.png",
            "attributes": [{"name": "multishot", "value": "120"}]
        }], "current_page": 1, "last_page": 3}});
        let e = extract_contracts(&body);
        assert_eq!((e.page, e.total_pages, e.has_more), (1, Some(3), true));
        let c = &normalize_all(&body)[0];
        assert_eq!(c.id, "65f6d1f9f8f87e16d88c7a22");
        assert_eq!(c.item_name, "Boar Prime Riven");
        assert_eq!(c.source_type.as_deref(), Some("auction"));
        assert_eq!(c.item_url_name.as_deref(), Some("riven_mod"));
        assert_eq!(c.weapon_url_name.as_deref(), Some("boar_prime"));
        assert_eq!(
            (c.platinum, c.visible, c.is_direct_sell),
            (600, false, false)
        );
        assert_eq!(
            c.stats,
            [WfmContractAttribute {
                url_name: "multishot".into(),
                label: "Multishot".into(),
                value: Some(AttributeValue::Number(120.0)),
                positive: None
            }]
        );
    }

    #[test]
    fn unknown_payload_shapes_give_safe_defaults() {
        let e = extract_contracts(&json!({"data": {"unexpected": true}}));
        assert_eq!(
            (e.rows.len(), e.page, e.total_pages, e.has_more),
            (0, 1, None, false)
        );
        assert!(normalize_all(&json!(null)).is_empty());
        // No id, no contract.
        assert!(normalize_all(&json!({"data": [{"platinum": 5}]})).is_empty());
    }

    #[test]
    fn own_and_foreign_auctions_share_one_shape() {
        let foreign = json!({"payload": {"auctions": [{
            "id": "a1", "starting_price": 100, "buyout_price": 300, "is_direct_sell": false,
            "owner": {"ingame_name": "Seller", "status": "InGame"},
            "item": {"type": "riven", "name": "croni-visican", "weapon_url_name": "rubico",
                     "re_rolls": 3, "attributes": [{"url_name": "critical_chance", "value": 16, "positive": true}]}
        }]}});
        let c = &normalize_all(&foreign)[0];
        assert_eq!(
            (c.seller.as_deref(), c.seller_status.as_deref()),
            (Some("Seller"), Some("ingame"))
        );
        assert_eq!(
            (c.platinum, c.rerolls, c.item_name.as_str()),
            (300, Some(3), "Rubico Riven")
        );
        // A buyout with no opening bid is a direct sale.
        let direct = normalize_all(&json!({"data": [{"id": "d", "buyout_price": 90}]}));
        assert!(direct[0].is_direct_sell);
        assert_eq!(direct[0].seller, None);
    }

    fn mock_client(script: Vec<Reply>) -> (Mock, WfmClient) {
        let m = spawn(script);
        let c = WfmClient::new_unthrottled(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        (m, c)
    }

    const ROW: &str = r#"{"id":"c1","platinum":10,"item":{"weapon_url_name":"rubico"}}"#;

    #[tokio::test]
    async fn the_first_route_that_answers_wins_and_is_tried_first_next_time() {
        let page =
            format!(r#"{{"payload":{{"auctions":[{ROW}],"current_page":1,"last_page":1}}}}"#);
        let (m, c) = mock_client(vec![
            reply(404, ""),
            reply(200, r#"{"data":{"slug":"me-1"}}"#),
            reply(200, &page),
            reply(200, &page),
        ]);
        let state = Contracts::default();
        let got = my_contracts(&c, &state, Some("Me"), None, None)
            .await
            .unwrap();
        assert_eq!(got.contracts.len(), 1);
        let reqs = m.requests();
        assert!(
            reqs[0].starts_with("get /v1/profile/auctions?limit=40 "),
            "{}",
            reqs[0]
        );
        assert!(reqs[1].starts_with("get /v2/me "));
        assert!(reqs[2].starts_with("get /v1/profile/me-1/auctions?limit=40 "));
        my_contracts(&c, &state, Some("Me"), Some(2), None)
            .await
            .unwrap();
        assert!(m.requests()[3].starts_with("get /v1/profile/me-1/auctions?page=2&limit=40 "));
    }

    #[tokio::test]
    async fn the_account_implicit_route_needs_no_slug_lookup() {
        let (m, c) = mock_client(vec![reply(200, r#"{"payload":{"auctions":[]}}"#)]);
        let got = my_contracts(
            &c,
            &Contracts::default(),
            Some("Trade Partner"),
            None,
            Some(500),
        )
        .await
        .unwrap();
        assert!(got.contracts.is_empty() && got.unavailable.is_none());
        assert_eq!(m.requests().len(), 1);
        assert!(m.requests()[0].starts_with("get /v1/profile/auctions?limit=100 "));
    }

    #[tokio::test]
    async fn a_dead_v1_degrades_to_no_contracts_with_a_reason() {
        // 7 routes + the /v2/me lookup in between; 403/404/405 are all "gone".
        let dead: Vec<Reply> = [404, 403, 404, 405, 404, 404, 404, 403, 404]
            .map(|s| reply(s, ""))
            .into();
        let (_, c) = mock_client(dead);
        let got = my_contracts(&c, &Contracts::default(), Some("Me"), None, None)
            .await
            .unwrap();
        assert!(got.contracts.is_empty() && !got.has_more);
        assert!(
            got.unavailable
                .unwrap()
                .contains("no longer serves riven contracts")
        );
    }

    #[tokio::test]
    async fn transient_failures_and_expired_sessions_are_errors_not_degradation() {
        let mut script = vec![reply(503, "")];
        script.extend((0..8).map(|_| reply(404, "")));
        let (_, c) = mock_client(script);
        let err = my_contracts(&c, &Contracts::default(), Some("Me"), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, WfmError::Api { status: 503, .. }), "{err}");

        let (m, c) = mock_client(vec![reply(401, "{}")]);
        let err = my_contracts(&c, &Contracts::default(), Some("Me"), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, WfmError::Unauthorized { .. }));
        assert_eq!(
            m.requests().len(),
            1,
            "an expired session stops the probing"
        );
    }

    #[tokio::test]
    async fn signed_out_never_reaches_the_network() {
        let (m, c) = mock_client(vec![]);
        for user in [None, Some("Me")] {
            c.set_token(None);
            let r = my_contracts(&c, &Contracts::default(), user, None, None).await;
            assert!(matches!(r, Err(WfmError::Unauthorized { .. })));
        }
        assert!(m.requests().is_empty());
    }
}
