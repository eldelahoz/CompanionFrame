//! Every riven contract/auction call to the v1 API, and nothing else: `/auctions/*`, the profile
//! routes and the review POST have no documented v2 equivalent (see
//! `docs/wfm-auth.md` §4). v1 can vanish without notice, so swap or delete this module when WFM ships v2
//! routes; `contracts`, `riven_search` and `reviews` only talk to the functions below.
//!
//! The four v2 contract routes in `CANDIDATES` are unconfirmed probes kept from WFHelper: they are
//! tried after the v1 routes and skipped on 4xx, so a dead one costs a request, never an error.

use reqwest::{Method, Url};
use serde_json::Value;
use zeroize::Zeroizing;

use super::client::{Api, Sent, WfmClient};
use super::errors::WfmError;

pub struct Candidate {
    pub name: &'static str,
    pub api: Api,
    /// Path with `{user}` where the account slug goes.
    pub path: &'static str,
}

impl Candidate {
    pub fn needs_slug(&self) -> bool {
        self.path.contains("{user}")
    }
}

/// Own contracts, in the order WFHelper tries them.
pub const CANDIDATES: [Candidate; 7] = [
    Candidate {
        name: "v1_my_profile_auctions",
        api: Api::V1,
        path: "/profile/auctions",
    },
    Candidate {
        name: "v1_profile_auctions",
        api: Api::V1,
        path: "/profile/{user}/auctions",
    },
    Candidate {
        name: "v2_contracts_my",
        api: Api::V2,
        path: "/contracts/my",
    },
    Candidate {
        name: "v2_auctions_my",
        api: Api::V2,
        path: "/auctions/my",
    },
    Candidate {
        name: "v2_profile_contracts",
        api: Api::V2,
        path: "/profile/{user}/contracts",
    },
    Candidate {
        name: "v2_profile_auctions",
        api: Api::V2,
        path: "/profile/{user}/auctions",
    },
    Candidate {
        name: "v1_profile_contracts",
        api: Api::V1,
        path: "/profile/{user}/contracts",
    },
];

pub fn query(page: u32, limit: u32) -> String {
    let mut params = vec![];
    if page > 1 {
        params.push(format!("page={page}"));
    }
    if limit > 0 {
        params.push(format!("limit={limit}"));
    }
    if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    }
}

/// Path of `c`; `slug` must already be a safe path segment (see `profile_slug::path_safe`).
pub fn candidate_path(c: &Candidate, slug: &str, page: u32, limit: u32) -> String {
    let user = urlencode(slug);
    format!("{}{}", c.path.replace("{user}", &user), query(page, limit))
}

pub fn urlencode(s: &str) -> String {
    use std::fmt::Write;
    s.bytes().fold(String::new(), |mut out, b| {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
        out
    })
}

fn json(body: &Value) -> Option<Zeroizing<String>> {
    Some(Zeroizing::new(body.to_string()))
}

pub async fn get_candidate(
    client: &WfmClient,
    c: &Candidate,
    slug: &str,
    page: u32,
    limit: u32,
) -> Result<Value, WfmError> {
    let path = candidate_path(c, slug, page, limit);
    client.request(c.api, Method::GET, &path, None).await
}

/// `query` is the already-encoded tail after `type=riven&weapon_url_name=…`.
pub async fn auction_search(
    client: &WfmClient,
    weapon_slug: &str,
    tail: &str,
) -> Result<Value, WfmError> {
    let path = format!(
        "/auctions/search?type=riven&weapon_url_name={}{tail}",
        urlencode(weapon_slug)
    );
    client.request(Api::V1, Method::GET, &path, None).await
}

pub async fn auction_create(client: &WfmClient, body: &Value) -> Result<Value, WfmError> {
    client
        .request(Api::V1, Method::POST, "/auctions/create", json(body))
        .await
}

pub async fn auction_update(client: &WfmClient, id: &str, body: &Value) -> Result<Value, WfmError> {
    let path = format!("/auctions/entry/{}", urlencode(id));
    client
        .request(Api::V1, Method::PUT, &path, json(body))
        .await
}

/// WFM has no auction delete (v1 answers 405 to DELETE): its own remove button PUTs this route.
pub async fn auction_close(client: &WfmClient, id: &str) -> Result<(), WfmError> {
    let path = format!("/auctions/entry/{}/close", urlencode(id));
    client
        .request(Api::V1, Method::PUT, &path, None)
        .await
        .map(drop)
}

/// A `Location` resolved against the request URL (it is as often a bare path as a full URL).
fn absolute(client: &WfmClient, path: &str, location: &str) -> Option<String> {
    let root = client.base(Api::V1);
    Url::parse(&format!("{root}{path}"))
        .and_then(|base| base.join(location))
        .ok()
        .map(String::from)
}

/// Where WFM redirects `/profile/{name}/reviews/`, or `None` when it serves it directly. Only a
/// target on the v1 root is trusted. A failed probe is `None` too; the caller then confirms the name.
pub async fn profile_redirect(client: &WfmClient, name: &str) -> Result<Option<String>, WfmError> {
    let path = format!("/profile/{}/reviews/", urlencode(name));
    match client
        .request_manual(Api::V1, Method::HEAD, &path, None)
        .await
    {
        Ok(Sent::Redirect(Some(loc))) => Ok(absolute(client, &path, &loc)
            .filter(|t| t.starts_with(&format!("{}/", client.base(Api::V1))))),
        Ok(_) => Ok(None),
        Err(e @ WfmError::RateLimited { .. }) => Err(e),
        Err(_) => Ok(None),
    }
}

/// Same, at a path that already is a `/profile/{slug}/review`. A redirect comes back as an absolute URL.
pub async fn review_at(client: &WfmClient, path: &str) -> Result<Sent, WfmError> {
    let body = serde_json::json!({"review_type": 1, "text": ""});
    match client
        .request_manual(Api::V1, Method::POST, path, json(&body))
        .await?
    {
        Sent::Redirect(loc) => Ok(Sent::Redirect(loc.and_then(|l| absolute(client, path, &l)))),
        done => Ok(done),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_auction_routes_come_first_and_carry_the_query() {
        let first: Vec<_> = CANDIDATES[..2]
            .iter()
            .map(|c| (c.name, c.needs_slug(), candidate_path(c, "testuser", 1, 40)))
            .collect();
        assert_eq!(
            first,
            [
                (
                    "v1_my_profile_auctions",
                    false,
                    "/profile/auctions?limit=40".to_owned()
                ),
                (
                    "v1_profile_auctions",
                    true,
                    "/profile/testuser/auctions?limit=40".to_owned()
                ),
            ]
        );
        assert_eq!(query(3, 40), "?page=3&limit=40");
        assert_eq!(query(1, 0), "");
    }

    #[test]
    fn slugs_are_percent_encoded_into_the_path() {
        let c = &CANDIDATES[1];
        assert_eq!(
            candidate_path(c, "a/b?c", 1, 0),
            "/profile/a%2Fb%3Fc/auctions"
        );
        assert_eq!(
            urlencode("dark_split_sword_(dual_swords)"),
            "dark_split_sword_%28dual_swords%29"
        );
    }
}
