//! warframe.market v2 catalog: item list, single item, set membership. The list is cached in
//! `kv_cache` and refreshed when `/v2/versions` reports a new collection version.
//!
//! Only successes are cached. A 401/403/408/429/5xx (see `errors::Class`) never turns into "no data":
//! the stale list is served, or the error surfaces. Only 400/404 on a single item mean "not there".

use reqwest::Method;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tauri::State;

use super::client::{Api, WfmClient};
use super::errors::{Class, WfmError};
use super::session::Session;
use super::types::{CatalogItem, Item};
use crate::db::Db;
use crate::services::cache::{kv_get, kv_put};

const ITEMS_KEY: &str = "wfm:v2:items";

#[derive(Serialize, Deserialize)]
struct Stored<T> {
    version: Option<String>,
    items: Vec<T>,
}

fn unusable(path: &str) -> WfmError {
    WfmError::invalid(format!("unusable {path} payload"))
}

fn data(v: Value) -> Option<Value> {
    match v {
        Value::Object(mut o) => o.remove("data").filter(|d| !d.is_null()),
        _ => None,
    }
}

/// Cached list for `path`, re-fetched when `/versions` `collections.<collection>` changed.
/// `parse` yields the rows from `data`; an empty result is an error, never a cached empty list.
pub(super) async fn cached_collection<T: Serialize + DeserializeOwned>(
    client: &WfmClient,
    db: &Db,
    key: &str,
    path: &str,
    collection: &str,
    parse: fn(&Value) -> Vec<T>,
) -> Result<Vec<T>, WfmError> {
    let cached = kv_get(db, key)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Stored<T>>(&s).ok())
        .filter(|s| !s.items.is_empty());
    let version = client
        .request(Api::V2, Method::GET, "/versions", None)
        .await
        .ok()
        .and_then(|v| {
            v.pointer(&format!("/data/collections/{collection}"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    // Unknown current version (offline, rate limited): the cached copy beats nothing.
    if cached
        .as_ref()
        .is_some_and(|c| version.is_none() || c.version == version)
    {
        return Ok(cached.unwrap().items);
    }
    let fresh = match client.request(Api::V2, Method::GET, path, None).await {
        Ok(body) => data(body).map(|d| parse(&d)).filter(|r| !r.is_empty()),
        Err(e) if cached.is_none() => return Err(e),
        Err(_) => None,
    };
    match (fresh, cached) {
        (Some(items), _) => {
            let stored = Stored { version, items };
            if let Err(e) = serde_json::to_string(&stored)
                .map_err(|e| e.to_string())
                .and_then(|json| kv_put(db, key, &json).map_err(|e| e.to_string()))
            {
                tracing::warn!(error = %e, key, "wfm cache write failed");
            }
            Ok(stored.items)
        }
        (None, Some(c)) => Ok(c.items),
        (None, None) => Err(unusable(path)),
    }
}

fn parse_items(d: &Value) -> Vec<CatalogItem> {
    d.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| serde_json::from_value::<Item>(e.clone()).ok())
        .filter(|i| !i.slug.is_empty())
        .map(|i| CatalogItem::from(&i))
        .collect()
}

pub async fn items(client: &WfmClient, db: &Db) -> Result<Vec<CatalogItem>, WfmError> {
    cached_collection(client, db, ITEMS_KEY, "/items", "items", parse_items).await
}

/// A slug goes into the URL path: refuse anything that could change the route.
pub(super) fn path_slug(slug: &str) -> Result<&str, WfmError> {
    let bad = |c: char| c.is_whitespace() || c.is_control() || "/\\?#%".contains(c);
    if slug.is_empty() || slug.len() > 128 || slug.contains("..") || slug.chars().any(bad) {
        return Err(WfmError::invalid("invalid item slug"));
    }
    Ok(slug)
}

/// `Ok(None)`: 400/404, the item is not on the market. Any other failure is an error, never "none".
async fn get_or_none(client: &WfmClient, path: &str) -> Result<Option<Value>, WfmError> {
    match client.request(Api::V2, Method::GET, path, None).await {
        Ok(v) => data(v).map(Some).ok_or_else(|| unusable(path)),
        Err(e) if e.class() == Class::NoData => Ok(None),
        Err(e) => Err(e),
    }
}

/// Live details of one item. The cached list can predate newly added variants.
pub async fn item(client: &WfmClient, slug: &str) -> Result<Option<Item>, WfmError> {
    let path = format!("/item/{}", path_slug(slug)?);
    let Some(d) = get_or_none(client, &path).await? else {
        return Ok(None);
    };
    let mut item: Item = serde_json::from_value(d).map_err(|_| unusable(&path))?;
    let mut seen = std::collections::HashSet::new();
    item.subtypes
        .retain(|s| !s.trim().is_empty() && seen.insert(s.clone()));
    Ok(Some(item))
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetPart {
    pub slug: String,
    pub quantity_in_set: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetLookup {
    #[serde(rename_all = "camelCase")]
    Set {
        set_slug: String,
        parts: Vec<SetPart>,
    },
    NotSet,
}

/// One root, at least two parts, every part with a positive quantity, and `slug` among them.
/// Anything else is an error (unavailable), never a guess.
fn parse_set(slug: &str, d: &Value) -> Option<SetLookup> {
    let items = d.get("items")?.as_array()?;
    let (mut roots, mut parts) = (vec![], vec![]);
    for raw in items {
        let it: Item = serde_json::from_value(raw.clone()).ok()?;
        if it.slug.is_empty() {
            return None;
        }
        if it.set_root == Some(true) {
            roots.push(it.slug);
        } else {
            let quantity_in_set = it.quantity_in_set.filter(|q| *q >= 1)?;
            parts.push(SetPart {
                slug: it.slug,
                quantity_in_set,
            });
        }
    }
    let [set_slug] = roots.as_slice() else {
        return None;
    };
    if parts.len() < 2 || (slug != set_slug && !parts.iter().any(|p| p.slug == slug)) {
        return None;
    }
    Some(SetLookup::Set {
        set_slug: set_slug.clone(),
        parts,
    })
}

pub async fn item_set(client: &WfmClient, slug: &str) -> Result<SetLookup, WfmError> {
    let path = format!("/item/{}/set", path_slug(slug)?);
    match get_or_none(client, &path).await? {
        None => Ok(SetLookup::NotSet),
        Some(d) => parse_set(slug, &d).ok_or_else(|| unusable(&path)),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_wfm_items(
    session: State<'_, Session>,
    db: State<'_, Db>,
) -> Result<Vec<CatalogItem>, WfmError> {
    items(session.client(), &db).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_item(session: State<'_, Session>, slug: String) -> Result<Option<Item>, WfmError> {
    item(session.client(), &slug).await
}

#[cfg(test)]
mod tests {
    use super::super::mock::{Mock, reply, spawn};
    use super::*;
    use crate::db::mem_db;

    fn client(m: &Mock) -> WfmClient {
        WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap()
    }

    const ITEMS: &str = include_str!("../../../tests/fixtures/wfm_items.json");
    const SET: &str = include_str!("../../../tests/fixtures/wfm_item_set.json");

    fn versions(items: &str) -> String {
        format!(r#"{{"data":{{"collections":{{"items":"{items}","rivens":"r1"}}}}}}"#)
    }

    fn count(m: &Mock, path: &str) -> usize {
        let line = format!("get /v2{path} ");
        m.requests().iter().filter(|r| r.starts_with(&line)).count()
    }

    #[tokio::test]
    async fn fetches_normalises_and_caches_by_version() {
        let m = spawn(vec![
            reply(200, &versions("v1")),
            reply(200, ITEMS),
            reply(200, &versions("v1")),
        ]);
        let (c, db) = (client(&m), mem_db());
        let list = items(&c, &db).await.unwrap();
        assert_eq!(list.len(), 3);
        let serration = list.iter().find(|i| i.slug == "serration").unwrap();
        assert_eq!(serration.name, "Serration");
        assert_eq!(serration.max_rank, Some(10));
        assert_eq!(
            serration.thumb.as_deref(),
            Some(
                "https://warframe.market/static/assets/items/images/en/thumbs/serration.711b19665b2acbac445c68c9e8f8b550.128x128.png"
            )
        );
        // Same version: no second /items request.
        assert_eq!(items(&c, &db).await.unwrap(), list);
        assert_eq!(count(&m, "/items"), 1);
    }

    #[tokio::test]
    async fn refetches_when_the_version_changes() {
        let m = spawn(vec![
            reply(200, &versions("v1")),
            reply(200, ITEMS),
            reply(200, &versions("v2")),
            reply(200, ITEMS),
        ]);
        let (c, db) = (client(&m), mem_db());
        items(&c, &db).await.unwrap();
        items(&c, &db).await.unwrap();
        assert_eq!(count(&m, "/items"), 2);
    }

    #[tokio::test]
    async fn transient_failure_serves_stale_and_never_caches_the_failure() {
        for status in [401, 403, 408, 429, 500, 503] {
            let m = spawn(vec![
                reply(200, &versions("v1")),
                reply(200, ITEMS),
                reply(200, &versions("v2")),
                reply(status, "{}"),
                reply(200, &versions("v2")),
                reply(200, ITEMS),
            ]);
            let (c, db) = (client(&m), mem_db());
            items(&c, &db).await.unwrap();
            assert_eq!(items(&c, &db).await.unwrap().len(), 3, "stale on {status}");
            // The failure left no marker: the next call fetches again.
            items(&c, &db).await.unwrap();
            assert_eq!(count(&m, "/items"), 3, "retried after {status}");
        }
    }

    #[tokio::test]
    async fn cold_cache_errors_surface_and_are_not_cached() {
        let m = spawn(vec![
            reply(500, "{}"),
            reply(503, "{}"),
            reply(200, &versions("v1")),
            reply(200, ITEMS),
        ]);
        let (c, db) = (client(&m), mem_db());
        assert!(items(&c, &db).await.is_err());
        assert_eq!(items(&c, &db).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn malformed_or_empty_list_is_an_error_and_not_cached() {
        for body in [
            r#"{"data":"nope"}"#,
            r#"{"data":[]}"#,
            r#"{"data":[{"x":1}]}"#,
            "[]",
        ] {
            let m = spawn(vec![reply(200, &versions("v1")), reply(200, body)]);
            let (c, db) = (client(&m), mem_db());
            assert!(items(&c, &db).await.is_err(), "{body}");
            assert_eq!(kv_get(&db, ITEMS_KEY).unwrap(), None);
        }
    }

    #[tokio::test]
    async fn unusable_cache_is_ignored_and_refetched() {
        let m = spawn(vec![reply(200, &versions("v1")), reply(200, ITEMS)]);
        let (c, db) = (client(&m), mem_db());
        kv_put(&db, ITEMS_KEY, "{broken").unwrap();
        assert_eq!(items(&c, &db).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn item_distinguishes_no_data_from_transient() {
        let body = r#"{"data":{"id":"a","slug":"live_wire","maxRank":0,"subtypes":["regular","regular",""]}}"#;
        let m = spawn(vec![
            reply(200, body),
            reply(404, "{}"),
            reply(400, "{}"),
            reply(429, "{}"),
            reply(503, "{}"),
        ]);
        let c = client(&m);
        let it = item(&c, "live_wire").await.unwrap().unwrap();
        assert_eq!(it.subtypes, ["regular"]);
        assert_eq!(item(&c, "gone").await.unwrap(), None);
        assert_eq!(item(&c, "gone").await.unwrap(), None);
        assert!(item(&c, "x").await.is_err());
        assert!(item(&c, "x").await.is_err());
    }

    #[tokio::test]
    async fn item_refuses_slugs_that_change_the_route() {
        let m = spawn(vec![]);
        let c = client(&m);
        for s in ["", "../me", "a/b", "a?b", "a b", "a%2fb"] {
            assert!(item(&c, s).await.is_err(), "{s:?}");
        }
        assert!(m.requests().is_empty());
        for s in ["zid-an-asheir", "melee_riven_mod_(veiled)"] {
            assert!(path_slug(s).is_ok());
        }
    }

    #[tokio::test]
    async fn set_valid_member_and_root() {
        let m = spawn(vec![reply(200, SET), reply(200, SET)]);
        let c = client(&m);
        for slug in ["ash_prime_set", "ash_prime_chassis_blueprint"] {
            let SetLookup::Set { set_slug, parts } = item_set(&c, slug).await.unwrap() else {
                panic!("not a set");
            };
            assert_eq!(set_slug, "ash_prime_set");
            assert_eq!(parts.len(), 4);
            assert_eq!(parts[0].quantity_in_set, 1);
        }
    }

    #[tokio::test]
    async fn set_rejects_malformed_quantities_and_shapes() {
        let bad = |f: fn(&mut Value)| {
            let mut v: Value = serde_json::from_str(SET).unwrap();
            f(&mut v["data"]);
            v.to_string()
        };
        let bodies = [
            bad(|d| d["items"][1]["quantityInSet"] = 0.into()),
            bad(|d| d["items"][1]["quantityInSet"] = 1.5.into()),
            bad(|d| d["items"][1]["quantityInSet"] = "2".into()),
            bad(|d| {
                d["items"][1]
                    .as_object_mut()
                    .unwrap()
                    .remove("quantityInSet");
            }),
            bad(|d| d["items"][1]["setRoot"] = true.into()),
            bad(|d| d["items"][0]["setRoot"] = false.into()),
            bad(|d| d["items"].as_array_mut().unwrap().truncate(2)),
            bad(|d| d["items"] = Value::Null),
        ];
        let m = spawn(bodies.iter().map(|b| reply(200, b)).collect());
        let c = client(&m);
        for (i, _) in bodies.iter().enumerate() {
            assert!(item_set(&c, "ash_prime_set").await.is_err(), "case {i}");
        }
    }

    #[tokio::test]
    async fn set_404_is_not_a_set_and_transient_is_an_error() {
        let m = spawn(vec![reply(404, "{}"), reply(503, "{}"), reply(429, "{}")]);
        let c = client(&m);
        assert_eq!(item_set(&c, "serration").await.unwrap(), SetLookup::NotSet);
        assert!(item_set(&c, "serration").await.is_err());
        assert!(item_set(&c, "serration").await.is_err());
    }

    #[test]
    fn set_must_contain_the_requested_slug() {
        let v: Value = serde_json::from_str(SET).unwrap();
        assert!(parse_set("somebody_else", &v["data"]).is_none());
    }
}
