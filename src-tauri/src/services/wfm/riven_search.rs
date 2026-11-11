//! Riven auctions on warframe.market (port of WFHelper `services/wfmRivenSearch.ts` and the family
//! list of `ipc/rivensIpc.ts`): the cached similar-riven search, and create / update / close of the
//! account's own auctions. Writes are user actions only. Every route is in `legacy_v1`.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::State;

use super::client::WfmClient;
use super::contracts::{WfmContract, normalize_all};
use super::errors::WfmError;
use super::legacy_v1::{self, urlencode};
use super::session::Session;
use super::validators::{self, RivenAuctionCreate, RivenAuctionUpdate};
use crate::services::rivens::data::{RivenData, has_variant_affix};

const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_CACHE_ENTRIES: usize = 50;
const DEFAULT_LIMIT: usize = 6;
/// WFM caps a search page at 500; a page this full means there is probably more.
const FULL_PAGE: usize = 490;

#[derive(Default)]
pub struct SearchCache(Mutex<HashMap<String, (Instant, Vec<WfmContract>)>>);

impl SearchCache {
    fn get(&self, key: &str) -> Option<Vec<WfmContract>> {
        let map = self.0.lock().unwrap();
        map.get(key)
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, v)| v.clone())
    }

    fn put(&self, key: String, listings: Vec<WfmContract>) {
        let mut map = self.0.lock().unwrap();
        map.insert(key, (Instant::now(), listings));
        while map.len() > MAX_CACHE_ENTRIES {
            let Some(oldest) = map
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            map.remove(&oldest);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RivenSearchOptions {
    /// How many listings to return (default 6).
    pub limit: Option<u32>,
    pub positive_stats: Option<Vec<String>>,
    pub negative_stats: Option<Vec<String>>,
    /// Fan out into polarity x sort queries (up to 7 requests) when the pool is capped.
    pub full_coverage: Option<bool>,
}

/// The stat tail of a riven auction search query. Measured on rubico 2026-09-01: WFM honours only the
/// FIRST of a repeated positive_stats/negative_stats key, so a comma list in one key is the AND the
/// picked stats mean.
pub fn stat_search_params(positive: &[String], negative: &[String]) -> String {
    let mut out = String::new();
    for (key, stats) in [("positive_stats", positive), ("negative_stats", negative)] {
        if !stats.is_empty() {
            out.push_str(&format!("&{key}={}", urlencode(&stats.join(","))));
        }
    }
    out
}

/// The first `count` listings plus the cheapest direct sales among the rest, so a consumer that hides
/// bidding auctions still has `count`. The search appends its price_desc page unsorted, hence the sort.
pub fn similar_listing_pool(listings: &[WfmContract], count: usize) -> Vec<WfmContract> {
    let first = &listings[..count.min(listings.len())];
    let have = first.iter().filter(|l| l.is_direct_sell).count();
    let price = |l: &WfmContract| l.buyout_platinum.unwrap_or(l.platinum);
    let mut direct: Vec<_> = listings[first.len()..]
        .iter()
        .filter(|l| l.is_direct_sell)
        .collect();
    direct.sort_by_key(|l| price(l));
    first
        .iter()
        .chain(direct.into_iter().take(count.saturating_sub(have)))
        .cloned()
        .collect()
}

fn auctions(body: &Value) -> Vec<WfmContract> {
    // Foreign rows without attributes are not rivens worth comparing.
    let payload = body.get("payload").unwrap_or(body);
    let rows: Vec<Value> = payload
        .get("auctions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|a| a.pointer("/item/attributes").is_some_and(Value::is_array))
        .cloned()
        .collect();
    normalize_all(&json!({ "auctions": rows }))
}

/// Similar rivens for sale, cheapest first. An error is returned (WFHelper swallowed it into `[]`) so
/// the caller can tell "no listings" from "WFM unreachable".
pub async fn search_similar(
    client: &WfmClient,
    cache: &SearchCache,
    weapon_slug: &str,
    opts: &RivenSearchOptions,
) -> Result<Vec<WfmContract>, WfmError> {
    let slug = validators::weapon_slug(weapon_slug)?;
    let limit = opts.limit.map_or(DEFAULT_LIMIT, |l| l as usize);
    let (mut pos, mut neg) = (
        opts.positive_stats.clone().unwrap_or_default(),
        opts.negative_stats.clone().unwrap_or_default(),
    );
    pos.sort();
    neg.sort();
    let full = opts.full_coverage == Some(true);
    // Coverage is part of the key so a quick result can't satisfy a full one.
    let key = format!(
        "{slug},{},{}|{}",
        if full { "full" } else { "quick" },
        pos.join(","),
        neg.join(",")
    );
    if let Some(hit) = cache.get(&key) {
        return Ok(hit.into_iter().take(limit).collect());
    }

    let stats = stat_search_params(&pos, &neg);
    let mut seen = HashSet::new();
    let mut all: Vec<WfmContract> = vec![];
    let mut add = |rows: Vec<WfmContract>| {
        for r in rows {
            if seen.insert(r.id.clone()) {
                all.push(r);
            }
        }
    };

    let quick = auctions(
        &legacy_v1::auction_search(client, &slug, &format!("{stats}&sort_by=price_asc")).await?,
    );
    let quick_len = quick.len();
    add(quick);
    if full && quick_len >= FULL_PAGE {
        // Likely more than 500 total: split by polarity and sort for full coverage.
        for pol in validators::RIVEN_POLARITIES {
            for sort in ["price_asc", "price_desc"] {
                let tail = format!("&polarity={pol}{stats}&sort_by={sort}");
                add(auctions(
                    &legacy_v1::auction_search(client, &slug, &tail).await?,
                ));
            }
        }
    } else if quick_len > 0 {
        // Small pool: price_desc too, in case it is cheap (already under 500).
        let tail = format!("{stats}&sort_by=price_desc");
        add(auctions(
            &legacy_v1::auction_search(client, &slug, &tail).await?,
        ));
    }
    tracing::info!(weapon = %slug, found = all.len(), "wfm riven search");
    cache.put(key, all.clone());
    Ok(all.into_iter().take(limit).collect())
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct RivenAuctionAck {
    /// `None` when WFM answered without an id.
    pub id: Option<String>,
}

fn authed(client: &WfmClient) -> Result<(), WfmError> {
    if client.token().is_some() {
        Ok(())
    } else {
        Err(WfmError::Unauthorized {
            detail: "Not logged in to Warframe.market.".into(),
        })
    }
}

fn create_body(a: &RivenAuctionCreate) -> Value {
    let attributes: Vec<_> = a
        .attributes
        .iter()
        .map(|x| json!({"url_name": x.url_name, "value": x.value, "positive": x.positive}))
        .collect();
    let mut body = json!({
        "item": {
            "type": "riven",
            "name": a.riven_name,
            "weapon_url_name": a.weapon_slug,
            "attributes": attributes,
            "re_rolls": a.rerolls,
            "mastery_level": a.mastery_level,
            "polarity": a.polarity,
            "mod_rank": a.mod_rank,
        },
        "starting_price": a.starting_price,
        "minimal_reputation": a.min_reputation,
        "private": a.is_private,
        // WFM requires the key; explicit null means "no buyout". Omitting it fails with
        // buyout_price: app.form.field_required.
        "buyout_price": a.buyout_price.filter(|p| *p > 0),
    });
    if !a.description.is_empty() {
        body["note"] = json!(a.description);
    }
    body
}

fn update_body(u: &RivenAuctionUpdate) -> Value {
    let mut body = json!({
        "minimal_reputation": u.min_reputation,
        // An explicit visible wins over the private flag.
        "visible": u.visible.unwrap_or(u.is_private != Some(true)),
        "note": u.description,
        "buyout_price": u.buyout_price.filter(|p| *p > 0),
    });
    // A direct sell has no opening bid. Sending the buyout in its place is what WFM reads as
    // converting the listing into a priced auction, so leave it out.
    if let Some(p) = u.starting_price {
        body["starting_price"] = json!(p);
    }
    body
}

pub async fn create_auction(
    client: &WfmClient,
    auction: RivenAuctionCreate,
) -> Result<RivenAuctionAck, WfmError> {
    authed(client)?;
    let a = auction.validated()?;
    let res = legacy_v1::auction_create(client, &create_body(&a)).await?;
    let id = res
        .pointer("/payload/auction/id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    tracing::info!(weapon = %a.weapon_slug, "wfm riven auction created");
    Ok(RivenAuctionAck { id })
}

pub async fn update_auction(
    client: &WfmClient,
    auction_id: &str,
    update: RivenAuctionUpdate,
) -> Result<RivenAuctionAck, WfmError> {
    authed(client)?;
    let id = validators::auction_id(auction_id)?;
    let u = update.validated()?;
    let res = legacy_v1::auction_update(client, &id, &update_body(&u)).await?;
    let id = res
        .pointer("/payload/auction/id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map_or(id, str::to_owned);
    Ok(RivenAuctionAck { id: Some(id) })
}

/// WFM has no auction delete: its own remove button PUTs the close route. A failure is reported as
/// is, because a 404 means the auction is already gone or is not ours, and probing a second route
/// would fire another account-mutating call for nothing.
pub async fn close_auction(client: &WfmClient, auction_id: &str) -> Result<(), WfmError> {
    authed(client)?;
    let id = validators::auction_id(auction_id)?;
    legacy_v1::auction_close(client, &id).await?;
    tracing::info!("wfm riven auction closed");
    Ok(())
}

/// Weapon names that have a riven market, one per WFM family: a variant folds onto its base weapon,
/// and a family whose base was never made keeps its affixed name. `None` families (list unavailable)
/// leaves the unfiltered list.
pub fn market_weapon_names(data: &RivenData, families: Option<&HashSet<String>>) -> Vec<String> {
    let names = data.all_weapon_names();
    let Some(families) = families else {
        return names;
    };
    let mut by_family: HashMap<String, String> = HashMap::new();
    for name in names {
        let slug = data.family_slug(&name);
        if slug.is_empty() || !families.contains(&slug) {
            continue;
        }
        match by_family.get(&slug) {
            Some(chosen) if !has_variant_affix(chosen) || has_variant_affix(&name) => {}
            _ => {
                by_family.insert(slug, name);
            }
        }
    }
    let mut out: Vec<String> = by_family.into_values().collect();
    out.sort_by_key(|n| n.to_lowercase());
    out
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_riven_search(
    session: State<'_, Session>,
    cache: State<'_, SearchCache>,
    weapon_slug: String,
    options: Option<RivenSearchOptions>,
) -> Result<Vec<WfmContract>, WfmError> {
    search_similar(
        session.client(),
        &cache,
        &weapon_slug,
        &options.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_riven_auction_create(
    session: State<'_, Session>,
    auction: RivenAuctionCreate,
) -> Result<RivenAuctionAck, WfmError> {
    create_auction(session.client(), auction).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_riven_auction_update(
    session: State<'_, Session>,
    auction_id: String,
    update: RivenAuctionUpdate,
) -> Result<RivenAuctionAck, WfmError> {
    update_auction(session.client(), &auction_id, update).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_riven_auction_close(
    session: State<'_, Session>,
    auction_id: String,
) -> Result<(), WfmError> {
    close_auction(session.client(), &auction_id).await
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::super::mock::{Mock, Reply, reply, spawn};
    use super::*;
    use crate::services::bundled_data::BundledData;

    const AID: &str = "aaaaaaaaaaaaaaaaaaaaaaaa";

    fn mock_client(script: Vec<Reply>) -> (Mock, WfmClient) {
        let m = spawn(script);
        let c = WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        (m, c)
    }

    fn body_of(req: &str) -> Value {
        serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap()).unwrap()
    }

    fn empty() -> Reply {
        reply(200, r#"{"payload":{"auctions":[]}}"#)
    }

    fn base() -> RivenAuctionCreate {
        serde_json::from_value(json!({
            "weaponSlug": "rubico", "rivenName": "Croni-Visican",
            "attributes": [
                {"urlName": "critical_chance", "value": 16, "positive": true},
                {"urlName": "recoil", "value": -9.9, "positive": true},
                {"urlName": "recoil", "value": 7.2, "positive": false}
            ],
            "rerolls": 3, "masteryLevel": 14, "polarity": "madurai", "modRank": 0,
            "buyoutPrice": null, "startingPrice": 100, "minReputation": 0,
            "isPrivate": false, "description": ""
        }))
        .unwrap()
    }

    // ---- search ----

    #[tokio::test]
    async fn every_picked_stat_travels_in_one_comma_list_per_polarity() {
        let (m, c) = mock_client(vec![empty()]);
        let opts = RivenSearchOptions {
            positive_stats: Some(vec!["critical_chance".into(), "damage".into()]),
            negative_stats: Some(vec!["zoom".into()]),
            ..Default::default()
        };
        search_similar(&c, &SearchCache::default(), "rubico", &opts)
            .await
            .unwrap();
        let path = &m.requests()[0];
        assert!(
            path.contains("positive_stats=critical_chance%2cdamage"),
            "{path}"
        );
        assert!(path.contains("negative_stats=zoom"));
        assert_eq!(path.matches("positive_stats=").count(), 1);
        assert_eq!(path.matches("negative_stats=").count(), 1);
    }

    #[tokio::test]
    async fn no_stat_keys_when_nothing_is_picked() {
        let (m, c) = mock_client(vec![empty()]);
        search_similar(
            &c,
            &SearchCache::default(),
            "rubico-prime",
            &Default::default(),
        )
        .await
        .unwrap();
        let path = &m.requests()[0];
        assert!(!path.contains("positive_stats") && !path.contains("negative_stats"));
        assert!(path.starts_with(
            "get /v1/auctions/search?type=riven&weapon_url_name=rubico-prime&sort_by=price_asc "
        ));
    }

    fn row(id: &str, start: u32, direct: bool) -> String {
        let buy = if direct {
            r#","is_direct_sell":true,"buyout_price":"#
        } else {
            r#","buyout_price":null,"x":"#
        };
        format!(
            r#"{{"id":"{id}","starting_price":{start}{buy}{start},"owner":{{"ingame_name":"S","status":"online"}},"item":{{"weapon_url_name":"rubico","re_rolls":1,"attributes":[{{"url_name":"critical_chance","value":9,"positive":true}}]}}}}"#
        )
    }

    #[tokio::test]
    async fn quick_search_adds_price_desc_dedups_caches_and_limits() {
        let asc = format!(
            r#"{{"payload":{{"auctions":[{},{}]}}}}"#,
            row("a", 10, false),
            row("b", 20, false)
        );
        let desc = format!(
            r#"{{"payload":{{"auctions":[{},{}]}}}}"#,
            row("b", 20, false),
            row("c", 30, false)
        );
        let (m, c) = mock_client(vec![reply(200, &asc), reply(200, &desc)]);
        let cache = SearchCache::default();
        let opts = RivenSearchOptions {
            limit: Some(2),
            ..Default::default()
        };
        let got = search_similar(&c, &cache, "rubico", &opts).await.unwrap();
        assert_eq!(
            got.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(
            (got[0].seller.as_deref(), got[0].rerolls),
            (Some("S"), Some(1))
        );
        assert!(m.requests()[1].contains("sort_by=price_desc"));
        // Second call is served from the cache with a bigger limit than the first.
        let again = search_similar(&c, &cache, "rubico", &Default::default())
            .await
            .unwrap();
        assert_eq!(again.len(), 3);
        assert_eq!(m.requests().len(), 2);
    }

    #[tokio::test]
    async fn an_empty_quick_page_stops_and_a_failure_is_reported() {
        let (m, c) = mock_client(vec![empty()]);
        assert!(
            search_similar(&c, &SearchCache::default(), "rubico", &Default::default())
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(m.requests().len(), 1);
        let (_, c) = mock_client(vec![reply(500, "{}")]);
        assert!(
            search_similar(&c, &SearchCache::default(), "rubico", &Default::default())
                .await
                .is_err()
        );
        let (m, c) = mock_client(vec![]);
        assert!(
            search_similar(&c, &SearchCache::default(), "a/b", &Default::default())
                .await
                .is_err()
        );
        assert!(
            m.requests().is_empty(),
            "a slug that changes the route never leaves"
        );
    }

    #[test]
    fn auctions_without_attributes_are_skipped() {
        let body = json!({"payload": {"auctions": [{"id": "x", "item": {}}, {"id": "y", "item": {"attributes": []}}]}});
        assert_eq!(
            auctions(&body)
                .iter()
                .map(|l| l.id.as_str())
                .collect::<Vec<_>>(),
            ["y"]
        );
    }

    fn listing(id: u32, direct: bool) -> WfmContract {
        let body = json!({"data": [{
            "id": id.to_string(), "starting_price": if direct { Value::Null } else { json!(id) },
            "buyout_price": if direct { json!(id) } else { Value::Null }, "is_direct_sell": direct
        }]});
        normalize_all(&body).remove(0)
    }

    #[test]
    fn pool_keeps_the_cheapest_and_tops_up_direct_sales_hidden_behind_auctions() {
        let all: Vec<_> = (0..30)
            .map(|i| listing(i + 1, i % 10 == 0))
            .chain((0..40).map(|i| listing(101 + i, true)))
            .collect();
        let pool = similar_listing_pool(&all, 30);
        assert_eq!(&pool[..30], &all[..30]);
        assert_eq!(pool.iter().filter(|l| l.is_direct_sell).count(), 30);
        assert_eq!(pool.len(), 57);
    }

    #[test]
    fn pool_tops_up_with_the_cheapest_direct_sales_even_in_descending_order() {
        let all: Vec<_> = (0..30)
            .map(|i| listing(i + 1, i != 0))
            .chain([listing(1000, true), listing(999, true)])
            .collect();
        let pool = similar_listing_pool(&all, 30);
        assert_eq!(
            pool[30..].iter().map(|l| l.platinum).collect::<Vec<_>>(),
            [999]
        );
    }

    // ---- create / update / close ----

    #[tokio::test]
    async fn create_always_sends_buyout_price_null_when_unset() {
        let (m, c) = mock_client(vec![reply(
            200,
            r#"{"payload":{"auction":{"id":"abc123"}}}"#,
        )]);
        let ack = create_auction(&c, base()).await.unwrap();
        assert_eq!(ack.id.as_deref(), Some("abc123"));
        let req = &m.requests()[0];
        assert!(req.starts_with("post /v1/auctions/create "));
        let body = body_of(req);
        assert_eq!(body.get("buyout_price"), Some(&Value::Null));
        assert_eq!(body["item"]["name"], "croni-visican");
        assert_eq!(body["item"]["weapon_url_name"], "rubico");
        assert!(body.get("note").is_none());
    }

    #[tokio::test]
    async fn create_passes_a_buyout_the_signed_values_and_the_minimum_reputation() {
        let (m, c) = mock_client(vec![reply(200, "{}")]);
        let a = RivenAuctionCreate {
            buyout_price: Some(150),
            min_reputation: 25,
            description: " hi ".into(),
            ..base()
        };
        assert_eq!(create_auction(&c, a).await.unwrap().id, None);
        let body = body_of(&m.requests()[0]);
        assert_eq!(body["buyout_price"], 150);
        assert_eq!(body["minimal_reputation"], 25);
        assert_eq!(body["note"], "hi");
        let values: Vec<f64> = body["item"]["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["value"].as_f64().unwrap())
            .collect();
        assert_eq!(values, [16.0, -9.9, 7.2]);
    }

    #[tokio::test]
    async fn create_validates_before_the_network() {
        let (m, c) = mock_client(vec![]);
        for a in [
            RivenAuctionCreate {
                starting_price: 0,
                ..base()
            },
            RivenAuctionCreate {
                polarity: "zenurik".into(),
                ..base()
            },
            RivenAuctionCreate {
                attributes: vec![],
                ..base()
            },
            RivenAuctionCreate {
                weapon_slug: "a/b".into(),
                ..base()
            },
            RivenAuctionCreate {
                mastery_level: 7,
                ..base()
            },
            RivenAuctionCreate {
                mod_rank: 9,
                ..base()
            },
        ] {
            assert!(matches!(
                create_auction(&c, a).await,
                Err(WfmError::Invalid { .. })
            ));
        }
        c.set_token(None);
        assert!(matches!(
            create_auction(&c, base()).await,
            Err(WfmError::Unauthorized { .. })
        ));
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn update_keeps_a_direct_sale_direct_and_lets_visible_beat_private() {
        let (m, c) = mock_client(vec![
            reply(
                200,
                r#"{"payload":{"auction":{"id":"zzzzzzzzzzzzzzzzzzzzzzzz"}}}"#,
            ),
            reply(200, "{}"),
        ]);
        let direct = RivenAuctionUpdate {
            buyout_price: Some(200),
            starting_price: None,
            min_reputation: 0,
            is_private: Some(true),
            description: " n ".into(),
            visible: None,
        };
        let ack = update_auction(&c, AID, direct).await.unwrap();
        assert_eq!(ack.id.as_deref(), Some("zzzzzzzzzzzzzzzzzzzzzzzz"));
        let req = &m.requests()[0];
        assert!(req.starts_with(&format!("put /v1/auctions/entry/{AID} ")));
        let body = body_of(req);
        assert!(body.get("starting_price").is_none());
        assert_eq!(
            (
                body["buyout_price"].clone(),
                body["visible"].clone(),
                body["note"].clone()
            ),
            (json!(200), json!(false), json!("n"))
        );
        let both = RivenAuctionUpdate {
            buyout_price: Some(0),
            starting_price: Some(50),
            min_reputation: 3,
            is_private: Some(true),
            description: String::new(),
            visible: Some(true),
        };
        assert_eq!(
            update_auction(&c, AID, both).await.unwrap().id.as_deref(),
            Some(AID)
        );
        let body = body_of(&m.requests()[1]);
        assert_eq!(
            (
                body["starting_price"].clone(),
                body["buyout_price"].clone(),
                body["visible"].clone()
            ),
            (json!(50), Value::Null, json!(true))
        );
    }

    #[tokio::test]
    async fn close_puts_the_close_route_once_and_reports_failures_as_is() {
        let (m, c) = mock_client(vec![
            reply(200, ""),
            reply(404, r#"{"error":"Not found"}"#),
            reply(405, r#"{"error":"Method not allowed: PUT"}"#),
            reply(403, r#"{"error":"app.form.not_authorized"}"#),
        ]);
        close_auction(&c, AID).await.unwrap();
        assert!(m.requests()[0].starts_with(&format!("put /v1/auctions/entry/{AID}/close ")));
        for (status, text) in [
            (404, "Not found"),
            (405, "Method not allowed"),
            (403, "app.form.not_authorized"),
        ] {
            match close_auction(&c, AID).await.unwrap_err() {
                WfmError::Api { status: s, detail } => {
                    assert!(s == status && detail.contains(text), "{s} {detail}")
                }
                e => panic!("{e}"),
            }
        }
        assert_eq!(m.requests().len(), 4, "no second mutating call");
        for bad in ["a/2", "a1", "../x"] {
            assert!(
                matches!(close_auction(&c, bad).await, Err(WfmError::Invalid { .. })),
                "{bad}"
            );
        }
        assert_eq!(m.requests().len(), 4);
    }

    // ---- market weapon names (needs `pnpm game-data`) ----

    fn data() -> Option<&'static RivenData> {
        static DATA: OnceLock<Option<RivenData>> = OnceLock::new();
        DATA.get_or_init(|| {
            crate::services::bundled_data::test_root()
                .map(|root| RivenData::load(&BundledData::new(root)))
        })
        .as_ref()
    }

    /// (slug, gameRef) of every weapon in `GET /v2/riven/weapons`, captured from the real API.
    fn wfm_weapons() -> Vec<(String, String)> {
        let v: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/wfm_riven_family_slugs.json"
        ))
        .unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["slug"].as_str().unwrap().to_owned(),
                    e["gameRef"].as_str().unwrap_or("").to_owned(),
                )
            })
            .collect()
    }

    fn families() -> HashSet<String> {
        wfm_weapons().into_iter().map(|(s, _)| s).collect()
    }

    #[test]
    fn one_option_per_wfm_family_with_variants_folded_onto_the_base() {
        let Some(d) = data() else { return };
        let fam = families();
        let all = d.all_weapon_names();
        let names = market_weapon_names(d, Some(&fam));
        let slugs: Vec<_> = names.iter().map(|n| d.family_slug(n)).collect();
        assert_eq!(slugs.iter().collect::<HashSet<_>>().len(), names.len());
        assert!(slugs.iter().all(|s| fam.contains(s)));
        assert!(names.len() < all.len());
        for kept in ["Boar", "Marelok"] {
            assert!(names.contains(&kept.to_owned()), "{kept}");
        }
        for gone in ["Boar Prime", "Vaykor Marelok", "MK1-Braton"] {
            assert!(!names.contains(&gone.to_owned()), "{gone}");
        }
        // Weapons with a disposition but no riven market are dropped.
        for dropped in ["Artemis Bow", "Iron Staff", "Whipclaw"] {
            assert!(
                all.contains(&dropped.to_owned()) && !names.contains(&dropped.to_owned()),
                "{dropped}"
            );
        }
        assert!(all.iter().any(|n| n.contains("<ARCHWING>")));
        assert!(!names.iter().any(|n| n.contains("<ARCHWING>")));
        // A family whose base weapon was never made keeps its affix.
        for kept in ["Kuva Bramma", "Tenet Livia", "Coda Motovore"] {
            assert!(
                names.contains(&kept.to_owned()) && fam.contains(&d.family_slug(kept)),
                "{kept}"
            );
        }
    }

    #[test]
    fn the_unfiltered_list_comes_back_when_the_family_list_is_unavailable() {
        let Some(d) = data() else { return };
        assert_eq!(market_weapon_names(d, None), d.all_weapon_names());
    }

    #[test]
    fn the_derived_family_slug_is_the_one_wfm_uses_for_every_resolvable_weapon() {
        let Some(d) = data() else { return };
        // WFM keys this weapon's auctions under a slug the base name cannot reproduce.
        let diverges = ["dark_split_sword_(dual_swords)"];
        let wrong: Vec<_> = wfm_weapons()
            .into_iter()
            .filter(|(slug, _)| !diverges.contains(&slug.as_str()))
            .filter_map(|(slug, game_ref)| {
                let name = d.weapon_name_by_unique_name(&game_ref)?;
                let derived = d.family_slug(name);
                (derived != slug).then(|| format!("{name}: derived {derived}, WFM {slug}"))
            })
            .collect();
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[test]
    fn the_ampersand_is_spelled_the_way_wfm_does() {
        let Some(d) = data() else { return };
        let fam = families();
        for (name, slug) in [
            ("Silva & Aegis", "silva_and_aegis"),
            ("Silva & Aegis Prime", "silva_and_aegis"),
            ("Ack & Brunt", "ack_and_brunt"),
            ("Cobra & Crane Prime", "cobra_and_crane"),
        ] {
            assert_eq!(d.family_slug(name), slug);
            assert!(fam.contains(slug));
        }
        assert!(!fam.contains("silva_aegis"));
    }
}
