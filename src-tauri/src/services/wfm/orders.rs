//! warframe.market v2 orders (port of WFHelper `services/wfmOrders.ts` + the order half of `wfmIpc.ts`):
//! my orders, create / update / delete / close, bulk visibility and the public order book.
//!
//! Creating, changing or closing an order is **always a user action** (WFM rules, `wfm-auth.md` §4):
//! these commands are only called from a click, never from a timer, alert or tracker. Every entry point
//! validates its input again (`validators`), so no caller can skip the trust boundary. No retries on
//! 401/429/5xx: only the three documented 400 compatibility retries in `create`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::State;
use zeroize::Zeroizing;

use super::catalog::{self, path_slug};
use super::client::{Api, WfmClient};
use super::errors::{Class, WfmError};
use super::scheduler::{self, Priority};
use super::session::Session;
use super::types::{CatalogItem, asset_url};
use super::validators::{self, CreateOrder, OrderUpdates};
use crate::db::Db;

/// api 0.25 rejects `perTrade`, 0.24 required it. Remember which the server accepts.
pub(crate) static SEND_PER_TRADE: AtomicBool = AtomicBool::new(false);

/// One of my orders, with the item resolved through the catalog.
#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MyOrder {
    pub id: String,
    /// `sell` or `buy`.
    pub order_type: String,
    pub platinum: u32,
    pub quantity: u32,
    pub per_trade: u32,
    pub visible: bool,
    pub mod_rank: Option<u32>,
    pub subtype: Option<String>,
    pub item_id: Option<String>,
    pub item_name: String,
    pub item_slug: Option<String>,
    pub item_thumb: Option<String>,
    /// ISO timestamps from the v2 order; absent on older replies.
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, specta::Type)]
pub struct MyOrders {
    pub sell: Vec<MyOrder>,
    pub buy: Vec<MyOrder>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct OrderAck {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
pub struct VisibilityResult {
    pub id: String,
    pub order: Option<MyOrder>,
    pub error: Option<String>,
}

/// A public order-book row, shaped like the v2 `Order` so the shared TS normaliser reads it as is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BookOrder {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type")]
    pub order_type: String,
    #[serde(default)]
    pub platinum: u32,
    #[serde(default)]
    pub quantity: u32,
    pub per_trade: Option<u32>,
    pub rank: Option<u32>,
    pub subtype: Option<String>,
    pub visible: Option<bool>,
    pub amber_stars: Option<u32>,
    pub cyan_stars: Option<u32>,
    pub user: Option<BookUser>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BookUser {
    pub ingame_name: Option<String>,
    pub status: Option<String>,
    pub platform: Option<String>,
    pub avatar: Option<String>,
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

fn body(v: Value) -> Option<Zeroizing<String>> {
    Some(Zeroizing::new(v.to_string()))
}

fn data(v: Value) -> Option<Value> {
    match v {
        Value::Object(mut o) => o.remove("data").filter(|d| !d.is_null()),
        _ => None,
    }
}

fn u32_of(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
}

fn str_of(raw: &Value, key: &str) -> Option<String> {
    raw.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// Raw v2 order -> `MyOrder`. Item details come from the catalog (the compact order carries `itemId`).
fn normalise(raw: &Value, catalog: &HashMap<&str, &CatalogItem>) -> Option<MyOrder> {
    let id = raw.get("id")?.as_str()?.to_owned();
    let item_id = raw.get("itemId").and_then(Value::as_str).map(str::to_owned);
    let item = item_id.as_deref().and_then(|i| catalog.get(i));
    let quantity = u32_of(raw.get("quantity")).filter(|q| *q > 0).unwrap_or(1);
    // WFM clamps perTrade into [1, quantity].
    let per_trade = u32_of(raw.get("perTrade").or(raw.get("per_trade")))
        .filter(|p| *p > 0)
        .map_or(1, |p| p.min(quantity));
    Some(MyOrder {
        id,
        order_type: raw
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("sell")
            .to_owned(),
        platinum: u32_of(raw.get("platinum")).unwrap_or(0),
        quantity,
        per_trade,
        visible: raw.get("visible").and_then(Value::as_bool).unwrap_or(true),
        mod_rank: u32_of(raw.get("rank")),
        subtype: raw
            .get("subtype")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        item_id,
        item_name: item.map_or_else(|| "(unknown)".into(), |i| i.name.clone()),
        item_slug: item.map(|i| i.slug.clone()),
        item_thumb: item.and_then(|i| i.thumb.as_deref()).and_then(asset_url),
        created_at: str_of(raw, "createdAt"),
        updated_at: str_of(raw, "updatedAt"),
    })
}

/// `data.order` or `data` of a create/update response.
fn mutation_order(v: Value) -> Result<MyOrder, WfmError> {
    let d = data(v).ok_or_else(|| WfmError::invalid("unusable order response"))?;
    let raw = d.get("order").unwrap_or(&d);
    normalise(raw, &HashMap::new()).ok_or_else(|| WfmError::invalid("unusable order response"))
}

pub async fn my_orders(client: &WfmClient, db: &Db) -> Result<MyOrders, WfmError> {
    authed(client)?;
    let t = Instant::now();
    let v = client
        .request(Api::V2, Method::GET, "/orders/my", None)
        .await?;
    let http_ms = t.elapsed().as_millis();
    let raw = data(v)
        .and_then(|d| d.as_array().cloned())
        .unwrap_or_default();
    // Without the catalog the list still renders, just with "(unknown)" names.
    let t = Instant::now();
    let items = if raw.is_empty() {
        Default::default()
    } else {
        catalog::items(client, db).await.unwrap_or_default()
    };
    let catalog_ms = t.elapsed().as_millis();
    let by_id: HashMap<&str, &CatalogItem> = items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut out = MyOrders::default();
    for o in raw.iter().filter_map(|r| normalise(r, &by_id)) {
        match o.order_type.as_str() {
            "buy" => out.buy.push(o),
            _ => out.sell.push(o),
        }
    }
    tracing::info!(
        "timing: my_orders orders={} http_ms={http_ms} catalog_ms={catalog_ms} catalog_items={}",
        out.buy.len() + out.sell.len(),
        items.len()
    );
    Ok(out)
}

/// `\bword\b` on lowercase text.
fn has_word(s: &str, word: &str) -> bool {
    s.match_indices(word).any(|(i, _)| {
        let edge = |c: Option<char>| !c.is_some_and(char::is_alphanumeric);
        edge(s[..i].chars().next_back()) && edge(s[i + word.len()..].chars().next())
    })
}

/// Creates an order. Each compatibility retry applies once: `perTrade` flip, `rank: 0`, and the
/// `regular` subtype. The item's subtypes come from the catalog when the server asks for one and the
/// caller gave none; anything but `regular` is never picked for the user.
pub async fn create(
    client: &WfmClient,
    db: &Db,
    params: CreateOrder,
    send_per_trade: &AtomicBool,
) -> Result<MyOrder, WfmError> {
    authed(client)?;
    let p = params.validated()?;
    let mut send = send_per_trade.load(Ordering::SeqCst);
    let (mut flipped, mut resolved) = (false, false);
    let mut rank = p.mod_rank;
    let mut subtype = p.subtype.clone();
    loop {
        let mut b = json!({
            "itemId": p.item_id,
            "type": p.order_type,
            "platinum": p.platinum,
            "quantity": p.quantity,
            "visible": p.visible.unwrap_or(true),
        });
        if send {
            b["perTrade"] = json!(p.per_trade.unwrap_or(1).clamp(1, p.quantity));
        }
        if let Some(r) = rank {
            b["rank"] = json!(r);
        }
        if let Some(s) = &subtype {
            b["subtype"] = json!(s);
        }
        let err = match client
            .request(Api::V2, Method::POST, "/order", body(b))
            .await
        {
            Ok(v) => return mutation_order(v),
            Err(e) => e,
        };
        let msg = err.to_string().to_lowercase();
        let wants = if send { "notallowed" } else { "required" };
        if !flipped && msg.contains("pertrade") && msg.contains(wants) {
            (flipped, send) = (true, !send);
            send_per_trade.store(send, Ordering::SeqCst);
        } else if rank.is_none() && has_word(&msg, "rank") && msg.contains("required") {
            // Stale catalog entries can omit the required rank.
            rank = Some(0);
        } else if subtype.is_none()
            && !resolved
            && msg.contains("subtype")
            && msg.contains("required")
        {
            resolved = true;
            let list = subtype_choices(client, db, &p.item_id).await;
            if list.iter().any(|s| s == "regular") {
                subtype = Some("regular".into());
            } else if !list.is_empty() {
                return Err(WfmError::SubtypeRequired { subtypes: list });
            } else {
                return Err(err);
            }
        } else {
            return Err(err);
        }
    }
}

pub async fn update(
    client: &WfmClient,
    order_id: &str,
    updates: OrderUpdates,
) -> Result<MyOrder, WfmError> {
    authed(client)?;
    let id = validators::order_id(order_id)?;
    let u = updates.validated()?;
    if u.is_empty() {
        return Err(WfmError::invalid("updateOrder: no fields to update."));
    }
    let mut b = serde_json::Map::new();
    if let Some(v) = u.platinum {
        b.insert("platinum".into(), v.into());
    }
    if let Some(v) = u.quantity {
        b.insert("quantity".into(), v.into());
    }
    if let Some(v) = u.visible {
        b.insert("visible".into(), v.into());
    }
    if let Some(v) = u.mod_rank {
        b.insert("rank".into(), v.into());
    }
    if let Some(v) = u.subtype {
        b.insert("subtype".into(), v.into());
    }
    let v = client
        .request(
            Api::V2,
            Method::PATCH,
            &format!("/order/{id}"),
            body(Value::Object(b)),
        )
        .await?;
    mutation_order(v)
}

pub async fn delete(client: &WfmClient, order_id: &str) -> Result<OrderAck, WfmError> {
    authed(client)?;
    let id = validators::order_id(order_id)?;
    client
        .request(Api::V2, Method::DELETE, &format!("/order/{id}"), None)
        .await?;
    Ok(OrderAck { id })
}

/// The API does not say what is left; callers re-fetch the orders.
pub async fn close(
    client: &WfmClient,
    order_id: &str,
    quantity: u32,
    per_trade: Option<u32>,
) -> Result<OrderAck, WfmError> {
    authed(client)?;
    let id = validators::order_id(order_id)?;
    let quantity = validators::close_quantity(quantity, per_trade)?;
    client
        .request(
            Api::V2,
            Method::POST,
            &format!("/order/{id}/close"),
            body(json!({ "quantity": quantity })),
        )
        .await?;
    Ok(OrderAck { id })
}

/// One PATCH per order, in order; a failure is reported on its row and does not stop the rest.
pub async fn set_visible(
    client: &WfmClient,
    ids: &[String],
    visible: bool,
) -> Result<Vec<VisibilityResult>, WfmError> {
    let ids = validators::bulk_ids(ids)?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let updates = OrderUpdates {
            visible: Some(visible),
            ..Default::default()
        };
        out.push(
            match scheduler::scoped(Priority::Job, update(client, &id, updates)).await {
                Ok(order) => VisibilityResult {
                    id,
                    order: Some(order),
                    error: None,
                },
                Err(e) => VisibilityResult {
                    id,
                    order: None,
                    error: Some(e.to_string()),
                },
            },
        );
    }
    Ok(out)
}

/// Public orders of one item. `top` asks for the best 5 per side from online users and may filter by
/// `rank`/`subtype`; leave `rank` unset for veiled rivens (they have none, PR #57). 400/404 = no
/// orders; any other failure is an error, never an empty book.
pub async fn orders_item(
    client: &WfmClient,
    slug: &str,
    top: bool,
    rank: Option<u32>,
    subtype: Option<&str>,
) -> Result<Vec<BookOrder>, WfmError> {
    let mut path = format!("/orders/item/{}", path_slug(slug)?);
    if top {
        path.push_str("/top");
        let mut q = vec![];
        if let Some(r) = rank {
            q.push(format!("rank={}", validators::range_rank(r)?));
        }
        if let Some(s) = subtype {
            q.push(format!("subtype={}", validators::subtype(s)?));
        }
        if !q.is_empty() {
            path = format!("{path}?{}", q.join("&"));
        }
    }
    let v = match client.request(Api::V2, Method::GET, &path, None).await {
        Ok(v) => v,
        Err(e) if e.class() == Class::NoData => return Ok(vec![]),
        Err(e) => return Err(e),
    };
    let d = data(v).ok_or_else(|| WfmError::invalid("unusable orders payload"))?;
    let rows: Vec<&Value> = match &d {
        Value::Array(a) => a.iter().collect(),
        Value::Object(o) => ["sell", "buy"]
            .iter()
            .filter_map(|k| o.get(*k).and_then(Value::as_array))
            .flatten()
            .collect(),
        _ => return Err(WfmError::invalid("unusable orders payload")),
    };
    Ok(rows
        .into_iter()
        .filter_map(|r| serde_json::from_value::<BookOrder>(r.clone()).ok())
        .collect())
}

async fn subtype_choices(client: &WfmClient, db: &Db, item_id: &str) -> Vec<String> {
    let Ok(items) =
        catalog::items_containing(client, db, |l| l.iter().any(|i| i.id == item_id)).await
    else {
        return vec![];
    };
    let Some(slug) = items.iter().find(|i| i.id == item_id).map(|i| &i.slug) else {
        return vec![];
    };
    // The list omits Atragraph choices; the live item has them.
    match catalog::item(client, slug).await {
        Ok(Some(item)) => item.subtypes,
        _ => vec![],
    }
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_my(
    session: State<'_, Session>,
    db: State<'_, Db>,
) -> Result<MyOrders, WfmError> {
    my_orders(session.client(), &db).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_create(
    session: State<'_, Session>,
    db: State<'_, Db>,
    order: CreateOrder,
) -> Result<MyOrder, WfmError> {
    let client = session.client();
    create(client, &db, order, &SEND_PER_TRADE).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_update(
    session: State<'_, Session>,
    order_id: String,
    updates: OrderUpdates,
) -> Result<MyOrder, WfmError> {
    update(session.client(), &order_id, updates).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_delete(
    session: State<'_, Session>,
    order_id: String,
) -> Result<OrderAck, WfmError> {
    delete(session.client(), &order_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_order_close(
    session: State<'_, Session>,
    order_id: String,
    quantity: u32,
    per_trade: Option<u32>,
) -> Result<OrderAck, WfmError> {
    close(session.client(), &order_id, quantity, per_trade).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_set_visible(
    session: State<'_, Session>,
    order_ids: Vec<String>,
    visible: bool,
) -> Result<Vec<VisibilityResult>, WfmError> {
    set_visible(session.client(), &order_ids, visible).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_orders_item(
    session: State<'_, Session>,
    slug: String,
    top: bool,
    rank: Option<u32>,
    subtype: Option<String>,
) -> Result<Vec<BookOrder>, WfmError> {
    orders_item(session.client(), &slug, top, rank, subtype.as_deref()).await
}

#[cfg(test)]
mod tests {
    use crate::db::mem_db;
    use std::sync::{Arc, Mutex};

    use super::super::mock::{Mock, reply, spawn};
    use super::*;

    const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaa";
    const ITEM: &str = "bbbbbbbbbbbbbbbbbbbbbbbb";

    type Events = Arc<Mutex<Vec<Option<String>>>>;

    fn client(m: &Mock) -> (WfmClient, Events) {
        let events: Events = Arc::default();
        let log = events.clone();
        let c = WfmClient::new(
            "http://x/v1",
            &format!("{}/v2", m.url),
            Box::new(move |t| log.lock().unwrap().push(t.map(str::to_owned))),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        (c, events)
    }

    fn order_json(extra: &str) -> String {
        format!(
            r#"{{"data":{{"id":"{ID}","type":"sell","platinum":10,"quantity":1,"visible":true,"itemId":"{ITEM}"{extra}}}}}"#
        )
    }

    fn new_order(extra: Value) -> CreateOrder {
        let mut v = json!({"itemId": ITEM, "orderType": "sell", "platinum": 10, "quantity": 1});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        serde_json::from_value(v).unwrap()
    }

    /// The replies `subtype_choices` consumes: versions, item list, live item.
    fn catalog_replies(subtypes: &str) -> Vec<super::super::mock::Reply> {
        vec![
            reply(200, r#"{"data":{"collections":{"items":"v1"}}}"#),
            reply(
                200,
                &format!(r#"{{"data":[{{"id":"{ITEM}","slug":"x_item"}}]}}"#),
            ),
            reply(
                200,
                &format!(r#"{{"data":{{"id":"{ITEM}","slug":"x_item","subtypes":{subtypes}}}}}"#),
            ),
        ]
    }

    #[tokio::test]
    async fn my_orders_split_by_side_and_resolve_items() {
        let orders = format!(
            r#"{{"data":[
                {{"id":"{ID}","type":"sell","platinum":50,"quantity":6,"perTrade":9,"rank":3,"itemId":"{ITEM}","createdAt":"2026-03-04T10:00:00.000Z"}},
                {{"id":"{ITEM}","type":"buy","platinum":5,"quantity":0,"visible":false,"subtype":"unrevealed","itemId":"zz"}},
                {{"type":"sell"}}
            ]}}"#
        );
        let m = spawn(vec![
            reply(200, &orders),
            reply(200, r#"{"data":{"collections":{"items":"v1"}}}"#),
            reply(
                200,
                &format!(
                    r#"{{"data":[{{"id":"{ITEM}","slug":"soma_prime_set","i18n":{{"en":{{"name":"Soma Prime Set","thumb":"items/a.png"}}}}}}]}}"#
                ),
            ),
        ]);
        let (c, _) = client(&m);
        let got = my_orders(&c, &mem_db()).await.unwrap();
        assert_eq!(got.sell.len(), 1);
        let s = &got.sell[0];
        assert_eq!(
            (s.item_name.as_str(), s.item_slug.as_deref()),
            ("Soma Prime Set", Some("soma_prime_set"))
        );
        assert_eq!((s.per_trade, s.mod_rank, s.quantity), (6, Some(3), 6));
        assert!(s.item_thumb.as_deref().unwrap().ends_with("items/a.png"));
        assert_eq!(s.created_at.as_deref(), Some("2026-03-04T10:00:00.000Z"));
        let b = &got.buy[0];
        assert_eq!(b.created_at, None);
        assert_eq!(b.item_name, "(unknown)");
        // quantity 0 is read as 1; the veiled riven keeps its subtype and no rank.
        assert_eq!(
            (b.quantity, b.subtype.as_deref(), b.mod_rank, b.visible),
            (1, Some("unrevealed"), None, false)
        );
        assert!(m.requests()[0].starts_with("get /v2/orders/my "));
    }

    #[tokio::test]
    async fn my_orders_survive_a_catalog_failure() {
        let m = spawn(vec![
            reply(
                200,
                &format!(
                    r#"{{"data":[{{"id":"{ID}","type":"sell","platinum":5,"quantity":1,"itemId":"{ITEM}"}}]}}"#
                ),
            ),
            reply(500, "{}"),
            reply(500, "{}"),
        ]);
        let (c, _) = client(&m);
        let got = my_orders(&c, &mem_db()).await.unwrap();
        assert_eq!(got.sell[0].item_name, "(unknown)");
    }

    #[tokio::test]
    async fn signed_out_calls_never_reach_the_network() {
        let m = spawn(vec![]);
        let (c, _) = client(&m);
        c.set_token(None);
        assert!(matches!(
            my_orders(&c, &mem_db()).await,
            Err(WfmError::Unauthorized { .. })
        ));
        assert!(matches!(
            delete(&c, ID).await,
            Err(WfmError::Unauthorized { .. })
        ));
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn unauthorized_expires_the_session_without_retry() {
        let m = spawn(vec![reply(401, r#"{"error":"app.errors.unauthorized"}"#)]);
        let (c, events) = client(&m);
        let err = my_orders(&c, &mem_db()).await.unwrap_err();
        assert!(matches!(err, WfmError::Unauthorized { .. }));
        assert_eq!(m.requests().len(), 1);
        assert_eq!(c.token(), None);
        assert_eq!(*events.lock().unwrap(), [None]);
    }

    #[tokio::test]
    async fn create_sends_v2_fields_and_a_veiled_riven_subtype() {
        let m = spawn(vec![reply(200, &order_json(r#","subtype":"unrevealed""#))]);
        let (c, _) = client(&m);
        let o = create(
            &c,
            &mem_db(),
            new_order(json!({"orderType": "SELL", "subtype": "Unrevealed"})),
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(o.subtype.as_deref(), Some("unrevealed"));
        let r = &m.requests()[0];
        assert!(r.starts_with("post /v2/order "));
        assert!(r.contains(r#""subtype":"unrevealed""#) && r.contains(r#""type":"sell""#));
        assert!(!r.contains("rank") && !r.contains("pertrade"), "{r}");
        assert!(r.contains("authorization: bearer tok"));
    }

    #[tokio::test]
    async fn create_rejects_bad_input_before_the_network() {
        let m = spawn(vec![]);
        let (c, _) = client(&m);
        for bad in [
            json!({"platinum": 900_001}),
            json!({"subtype": "shiny"}),
            json!({"itemId": "nope"}),
        ] {
            let err = create(&c, &mem_db(), new_order(bad), &AtomicBool::new(false)).await;
            assert!(matches!(err, Err(WfmError::Invalid { .. })));
        }
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn create_flips_per_trade_once_and_remembers() {
        let m = spawn(vec![
            reply(
                400,
                r#"{"error":{"inputs":{"perTrade":"app.field.required"}}}"#,
            ),
            reply(200, &order_json("")),
        ]);
        let (c, _) = client(&m);
        let pref = AtomicBool::new(false);
        create(
            &c,
            &mem_db(),
            new_order(json!({"quantity": 4, "perTrade": 2})),
            &pref,
        )
        .await
        .unwrap();
        let reqs = m.requests();
        assert!(
            !reqs[0].contains("pertrade") && reqs[1].contains(r#""pertrade":2"#),
            "{reqs:?}"
        );
        assert!(pref.load(Ordering::SeqCst));

        let m = spawn(vec![
            reply(
                400,
                r#"{"error":{"inputs":{"perTrade":"app.field.notAllowed"}}}"#,
            ),
            reply(
                400,
                r#"{"error":{"inputs":{"perTrade":"app.field.required"}}}"#,
            ),
        ]);
        let (c, _) = client(&m);
        let pref = AtomicBool::new(true);
        // Flipped back once, then the second 400 surfaces: never a loop.
        assert!(
            create(&c, &mem_db(), new_order(json!({})), &pref)
                .await
                .is_err()
        );
        assert_eq!(m.requests().len(), 2);
    }

    #[tokio::test]
    async fn create_retries_rank_zero_and_the_regular_subtype() {
        let mut script = vec![
            reply(400, r#"{"error":{"inputs":{"rank":"app.field.required"}}}"#),
            reply(
                400,
                r#"{"error":{"inputs":{"subtype":"app.field.required"}}}"#,
            ),
        ];
        script.extend(catalog_replies(r#"["regular","atragraph"]"#));
        script.push(reply(200, &order_json("")));
        let m = spawn(script);
        let (c, _) = client(&m);
        create(&c, &mem_db(), new_order(json!({})), &AtomicBool::new(false))
            .await
            .unwrap();
        let last = m.requests().pop().unwrap();
        assert!(
            last.contains(r#""rank":0"#) && last.contains(r#""subtype":"regular""#),
            "{last}"
        );
        assert!(m.requests()[4].starts_with("get /v2/item/x_item "));
    }

    #[tokio::test]
    async fn create_never_guesses_a_non_regular_subtype() {
        let mut script = vec![reply(
            400,
            r#"{"error":{"inputs":{"subtype":"app.field.required"}}}"#,
        )];
        script.extend(catalog_replies(r#"["intact","radiant"]"#));
        let m = spawn(script);
        let (c, _) = client(&m);
        let err = create(&c, &mem_db(), new_order(json!({})), &AtomicBool::new(false))
            .await
            .unwrap_err();
        assert_eq!(
            err,
            WfmError::SubtypeRequired {
                subtypes: vec!["intact".into(), "radiant".into()]
            }
        );
        assert_eq!(m.requests().len(), 4, "no second order is posted");
    }

    #[tokio::test]
    async fn update_patches_only_what_changed_including_riven_subtype() {
        let m = spawn(vec![reply(200, &order_json(r#","subtype":"revealed""#))]);
        let (c, _) = client(&m);
        let u = OrderUpdates {
            subtype: Some("Revealed".into()),
            platinum: Some(120),
            ..Default::default()
        };
        let o = update(&c, ID, u).await.unwrap();
        assert_eq!(o.subtype.as_deref(), Some("revealed"));
        let r = &m.requests()[0];
        assert!(r.starts_with(&format!("patch /v2/order/{ID} ")));
        assert!(r.contains(r#""subtype":"revealed""#) && r.contains(r#""platinum":120"#));
        assert!(!r.contains("quantity") && !r.contains("visible"), "{r}");
    }

    #[tokio::test]
    async fn update_refuses_empty_or_out_of_range_changes() {
        let m = spawn(vec![]);
        let (c, _) = client(&m);
        assert!(update(&c, ID, OrderUpdates::default()).await.is_err());
        let big = OrderUpdates {
            quantity: Some(10_000),
            ..Default::default()
        };
        assert!(update(&c, ID, big).await.is_err());
        assert!(
            update(
                &c,
                "../x",
                OrderUpdates {
                    visible: Some(true),
                    ..Default::default()
                }
            )
            .await
            .is_err()
        );
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn delete_and_close_hit_the_documented_routes() {
        let m = spawn(vec![reply(200, "{}"), reply(200, "{}")]);
        let (c, _) = client(&m);
        assert_eq!(delete(&c, ID).await.unwrap().id, ID);
        assert_eq!(close(&c, ID, 6, Some(3)).await.unwrap().id, ID);
        let reqs = m.requests();
        assert!(reqs[0].starts_with(&format!("delete /v2/order/{ID} ")));
        assert!(reqs[1].starts_with(&format!("post /v2/order/{ID}/close ")));
        assert!(reqs[1].contains(r#"{"quantity":6}"#));
    }

    #[tokio::test]
    async fn close_enforces_quantity_rules_locally() {
        let m = spawn(vec![]);
        let (c, _) = client(&m);
        for (q, pt) in [(0, None), (10_000, None), (5, Some(3))] {
            assert!(close(&c, ID, q, pt).await.is_err(), "{q} {pt:?}");
        }
        assert!(close(&c, "not-an-id", 1, None).await.is_err());
        assert!(m.requests().is_empty());
    }

    #[tokio::test]
    async fn close_failures_surface_the_wfm_error() {
        let m = spawn(vec![reply(404, r#"{"error":"app.order.notFound"}"#)]);
        let (c, _) = client(&m);
        let err = close(&c, ID, 1, None).await.unwrap_err();
        assert!(matches!(err, WfmError::Api { status: 404, .. }));
    }

    #[tokio::test]
    async fn set_visible_reports_each_order() {
        let m = spawn(vec![
            reply(200, &order_json(r#","visible":false"#)),
            reply(404, r#"{"error":"app.order.notFound"}"#),
        ]);
        let (c, _) = client(&m);
        let other = "cccccccccccccccccccccccc".to_owned();
        let got = set_visible(&c, &[ID.into(), other.clone(), "bad".into()], false)
            .await
            .unwrap();
        assert_eq!(got.len(), 2);
        assert!(got[0].order.is_some() && got[0].error.is_none());
        assert_eq!(
            (got[1].id.as_str(), got[1].order.is_none()),
            (other.as_str(), true)
        );
        assert!(got[1].error.is_some());
        assert!(m.requests()[0].contains(r#""visible":false"#));
        assert!(set_visible(&c, &["bad".into()], true).await.is_err());
    }

    const BOOK: &str = r#"{"data":[
        {"id":"a","type":"sell","platinum":40,"quantity":2,"perTrade":2,"rank":0,"subtype":"intact","visible":true,"user":{"ingameName":"Seller","status":"ingame","platform":"pc","avatar":"x.png"}},
        {"type":"buy","platinum":10,"quantity":1,"user":{"ingameName":"Buyer"}},
        {"platinum":10}
    ]}"#;

    #[tokio::test]
    async fn order_book_rows_keep_the_shape_the_shared_normaliser_reads() {
        let m = spawn(vec![reply(200, BOOK)]);
        let (c, _) = client(&m);
        c.set_token(None);
        let rows = orders_item(&c, "soma_prime_set", false, None, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2, "the row without a type is dropped");
        let json = serde_json::to_value(&rows[0]).unwrap();
        assert_eq!(json["type"], "sell");
        assert_eq!(json["user"]["ingameName"], "Seller");
        assert_eq!(json["perTrade"], 2);
        assert!(m.requests()[0].starts_with("get /v2/orders/item/soma_prime_set "));
        assert!(!m.requests()[0].contains("authorization"));
    }

    #[tokio::test]
    async fn veiled_rivens_are_queried_without_rank_zero() {
        let m = spawn(vec![
            reply(
                200,
                r#"{"data":{"sell":[{"type":"sell","platinum":9,"quantity":1}],"buy":[{"type":"buy","platinum":3,"quantity":1}]}}"#,
            ),
            reply(200, r#"{"data":{"sell":[],"buy":[]}}"#),
        ]);
        let (c, _) = client(&m);
        let rows = orders_item(&c, "kuva_bramma_riven", true, None, Some("Unrevealed"))
            .await
            .unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.order_type.as_str())
                .collect::<Vec<_>>(),
            ["sell", "buy"]
        );
        orders_item(&c, "soma_prime", true, Some(10), None)
            .await
            .unwrap();
        let reqs = m.requests();
        assert!(
            reqs[0].starts_with("get /v2/orders/item/kuva_bramma_riven/top?subtype=unrevealed "),
            "{}",
            reqs[0]
        );
        assert!(!reqs[0].contains("rank"));
        assert!(reqs[1].contains("/top?rank=10 "));
    }

    #[tokio::test]
    async fn order_book_distinguishes_no_data_from_failure() {
        let m = spawn(vec![
            reply(404, r#"{"error":"app.item.notFound"}"#),
            reply(429, "{}"),
            reply(503, "{}"),
        ]);
        let (c, _) = client(&m);
        assert!(
            orders_item(&c, "nope", false, None, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            orders_item(&c, "x", false, None, None).await,
            Err(WfmError::RateLimited { .. })
        ));
        c.lift_gate();
        assert!(matches!(
            orders_item(&c, "x", false, None, None).await,
            Err(WfmError::Api { status: 503, .. })
        ));
        assert!(orders_item(&c, "../etc", false, None, None).await.is_err());
        assert!(orders_item(&c, "x", true, Some(99), None).await.is_err());
    }

    #[test]
    fn has_word_respects_boundaries() {
        assert!(has_word("rank: app.field.required", "rank"));
        assert!(!has_word("modrank required", "rank"));
        assert!(!has_word("ranked", "rank"));
    }
}
