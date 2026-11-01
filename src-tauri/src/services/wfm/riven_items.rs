//! Tradable riven weapons (`/v2/riven/weapons`), cached like the item list and refreshed when
//! `/versions` `collections.rivens` changes. Callers must fail open on an error, not read it as
//! "not a riven weapon".

use serde_json::Value;
use tauri::State;

use super::catalog::cached_collection;
use super::client::WfmClient;
use super::errors::WfmError;
use super::session::Session;
use super::types::{RivenItem, RivenWeapon};
use crate::db::Db;

const KEY: &str = "wfm:v2:riven-weapons";

fn parse(d: &Value) -> Vec<RivenWeapon> {
    d.as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| serde_json::from_value::<RivenItem>(e.clone()).ok())
        .filter(|i| !i.slug.is_empty())
        .map(|i| RivenWeapon::from(&i))
        .collect()
}

pub async fn weapons(client: &WfmClient, db: &Db) -> Result<Vec<RivenWeapon>, WfmError> {
    cached_collection(client, db, KEY, "/riven/weapons", "rivens", parse).await
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_riven_weapons(
    session: State<'_, Session>,
    db: State<'_, Db>,
) -> Result<Vec<RivenWeapon>, WfmError> {
    weapons(session.client(), &db).await
}

#[cfg(test)]
mod tests {
    use super::super::mock::{reply, spawn};
    use super::*;
    use crate::db::mem_db;

    const RIVENS: &str = include_str!("../../../tests/fixtures/wfm_riven_weapons.json");

    fn versions(r: &str) -> String {
        format!(r#"{{"data":{{"collections":{{"items":"i1","rivens":"{r}"}}}}}}"#)
    }

    fn client(url: &str) -> WfmClient {
        WfmClient::new("http://x/v1", &format!("{url}/v2"), Box::new(|_| {})).unwrap()
    }

    #[tokio::test]
    async fn fetches_caches_and_refreshes_by_version() {
        let m = spawn(vec![
            reply(200, &versions("r1")),
            reply(200, RIVENS),
            reply(200, &versions("r1")),
            reply(200, &versions("r2")),
            reply(200, RIVENS),
        ]);
        let (c, db) = (client(&m.url), mem_db());
        let list = weapons(&c, &db).await.unwrap();
        assert_eq!(list.len(), 3);
        let kulstar = list.iter().find(|w| w.slug == "kulstar").unwrap();
        assert_eq!(kulstar.name, "Kulstar");
        assert_eq!(kulstar.riven_type.as_deref(), Some("pistol"));
        assert_eq!(kulstar.disposition, Some(1.3));
        assert_eq!(weapons(&c, &db).await.unwrap(), list);
        weapons(&c, &db).await.unwrap();
        let fetches = m
            .requests()
            .iter()
            .filter(|r| r.starts_with("get /v2/riven/weapons "))
            .count();
        assert_eq!(fetches, 2);
    }

    #[tokio::test]
    async fn malformed_empty_and_failed_payloads_are_errors() {
        let m = spawn(vec![
            reply(200, &versions("r1")),
            reply(200, r#"{"data":{"x":1}}"#),
            reply(200, &versions("r1")),
            reply(200, r#"{"data":[]}"#),
            reply(200, &versions("r1")),
            reply(500, "{}"),
        ]);
        let (c, db) = (client(&m.url), mem_db());
        for _ in 0..3 {
            assert!(weapons(&c, &db).await.is_err());
        }
    }
}
