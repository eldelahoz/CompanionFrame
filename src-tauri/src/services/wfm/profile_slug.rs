//! Profile slugs (port of WFHelper `wfmProfileSlug.ts` + the slug half of `wfmSession.ts`).
//! WFM mints a profile slug from the in-game name (lowercased, punctuation stripped, spaces to
//! hyphens); nothing here invents one. A write must refuse rather than fall back to the raw name.

use std::sync::Mutex;

use reqwest::Method;
use serde_json::Value;

use super::client::{Api, WfmClient};
use super::errors::WfmError;
use super::legacy_v1;

/// Where a profile name lives on warframe.market. `Unresolved`: the lookup failed, so a caller
/// that writes must refuse.
#[derive(Debug, PartialEq)]
pub enum Resolution {
    Resolved(String),
    NotFound,
    Unresolved,
}

/// A slug WFM already minted, or `None`. The allowlist keeps path separators out of every route:
/// lowercase letters/digits first and last, `_ ( ) ' ’ -` and combining marks between.
pub fn sanitize(v: &str) -> Option<String> {
    let s = v.trim().to_lowercase();
    let edge = |c: char| (c.is_alphabetic() && !c.is_uppercase()) || c.is_numeric();
    let mid = |c: char| edge(c) || "_()'’-".contains(c) || ('\u{300}'..='\u{36f}').contains(&c);
    let (first, last) = (s.chars().next()?, s.chars().next_back()?);
    let ok = s.chars().count() <= 120
        && edge(first)
        && (s.chars().count() == 1 || (edge(last) || last == ')'))
        && s.chars().all(mid);
    ok.then_some(s)
}

/// WFHelper `normalizeForSlug`: display name -> `trade_partner`. Only the fallback for an absent answer.
fn fold(name: &str) -> Option<String> {
    let mut out = String::new();
    for c in name
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !"'’‘".contains(*c))
    {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_');
    (!out.is_empty()).then(|| out.to_owned())
}

/// The signed-in account's slug, read once per user.
#[derive(Default)]
pub struct SlugCache(Mutex<Option<(String, String)>>);

/// The account's own slug. `/v2/me` mints it; folding the name is only the fallback for an absent or
/// failed answer (a failed answer is not cached, so the next call asks again).
pub async fn own_slug(client: &WfmClient, cache: &SlugCache, user: &str) -> Option<String> {
    if let Some((u, s)) = cache.0.lock().unwrap().as_ref()
        && u == user
    {
        return Some(s.clone());
    }
    let folded = fold(user);
    let me = client.request(Api::V2, Method::GET, "/me", None).await;
    match me {
        Ok(v) => {
            let slug = v
                .pointer("/data/slug")
                .and_then(Value::as_str)
                .and_then(sanitize)
                .or(folded);
            if let Some(s) = &slug {
                *cache.0.lock().unwrap() = Some((user.to_owned(), s.clone()));
            }
            slug
        }
        Err(e) => {
            tracing::warn!(error = %e, "wfm profile slug lookup failed");
            folded
        }
    }
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The slug in `{root}/profile/{slug}/{tail}[/]`, decoded and allowlisted. `None` when the
/// location has another shape or the segment is not a slug.
pub fn slug_in(location: &str, root: &str, tail: &str) -> Option<String> {
    let rest = location.strip_prefix(&format!("{root}/profile/"))?;
    let seg = rest
        .trim_end_matches('/')
        .strip_suffix(&format!("/{tail}"))?;
    if seg.is_empty() || seg.contains(['/', '?', '#']) {
        return None;
    }
    sanitize(&percent_decode(seg)?)
}

/// A HEAD with no redirect is ambiguous (served as-is, no profile, or probe failed). The account
/// route disambiguates but is case sensitive, so the name is folded first.
async fn confirm_served_name(client: &WfmClient, name: &str) -> Resolution {
    let Some(candidate) = sanitize(&name.to_lowercase()) else {
        return Resolution::NotFound;
    };
    let path = format!("/user/{}", legacy_v1::urlencode(&candidate));
    let v = match client.request(Api::V2, Method::GET, &path, None).await {
        Ok(v) => v,
        Err(WfmError::Api { status: 404, .. }) => return Resolution::NotFound,
        Err(e) => {
            tracing::warn!(error = %e, "wfm profile lookup failed");
            return Resolution::Unresolved;
        }
    };
    let Some(slug) = v
        .pointer("/data/slug")
        .and_then(Value::as_str)
        .and_then(sanitize)
    else {
        return Resolution::Unresolved;
    };
    let served = v.pointer("/data/ingameName").and_then(Value::as_str);
    // Folding can collide, so the served name must match the one we traded with.
    if served.map(|s| s.trim().to_lowercase()) != Some(name.trim().to_lowercase()) {
        return Resolution::NotFound;
    }
    Resolution::Resolved(slug)
}

/// The slug WFM serves `name` under, read back from WFM itself.
pub async fn probe(client: &WfmClient, name: &str) -> Resolution {
    let name = name.trim();
    if name.is_empty() {
        return Resolution::NotFound;
    }
    let Ok(location) = legacy_v1::profile_redirect(client, name).await else {
        return Resolution::Unresolved;
    };
    match location {
        None => confirm_served_name(client, name).await,
        Some(loc) => match slug_in(&loc, client.base(Api::V1), "reviews") {
            Some(slug) => Resolution::Resolved(slug),
            // The redirect said the name is not the slug, so a target the allowlist
            // refuses leaves nothing safe to write to.
            None => Resolution::Unresolved,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::mock::{reply, spawn};
    use super::*;

    fn client(base: &str) -> WfmClient {
        let c = WfmClient::new(
            &format!("{base}/v1"),
            &format!("{base}/v2"),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        c
    }

    /// Relative on purpose: the mock's port is only known after it is spawned.
    fn redirect(slug: &str) -> super::super::mock::Reply {
        reply(301, "").header("Location", &format!("/v1/profile/{slug}/reviews/"))
    }

    #[test]
    fn sanitize_keeps_minted_slugs_and_refuses_the_rest() {
        for ok in [
            "alt-handle",
            "relay-fox-7",
            "courier",
            "partner_02",
            "a",
            "spare-parts",
        ] {
            assert_eq!(sanitize(ok).as_deref(), Some(ok));
        }
        assert_eq!(sanitize(" Courier ").as_deref(), Some("courier"));
        for bad in [
            "",
            "-alt",
            "alt-",
            "a/b",
            "a b",
            "..",
            "a?x",
            &"a".repeat(130),
        ] {
            assert_eq!(sanitize(bad), None, "{bad}");
        }
    }

    #[test]
    fn fold_matches_wfhelpers_fallback() {
        assert_eq!(fold("Trade Partner").as_deref(), Some("trade_partner"));
        assert_eq!(fold(".Courier.").as_deref(), Some("courier"));
        assert_eq!(fold("..."), None);
    }

    #[test]
    fn slug_in_reads_only_the_expected_shape() {
        let root = "https://api.warframe.market/v1";
        let loc = |s: &str| format!("{root}/profile/{s}/reviews/");
        assert_eq!(
            slug_in(&loc("squad-mate"), root, "reviews").as_deref(),
            Some("squad-mate")
        );
        assert_eq!(
            slug_in(&loc("a%2Db"), root, "reviews").as_deref(),
            Some("a-b")
        );
        assert_eq!(slug_in(&loc("a%2Fb"), root, "reviews"), None);
        assert_eq!(slug_in(&loc("A%20B"), root, "reviews"), None);
        assert_eq!(
            slug_in("https://evil.test/v1/profile/x/reviews/", root, "reviews"),
            None
        );
        assert_eq!(
            slug_in(&format!("{root}/profile/x/review"), root, "reviews"),
            None
        );
    }

    #[tokio::test]
    async fn probe_takes_the_slug_the_redirect_names() {
        let m3 = spawn(vec![redirect("trade-partner")]);
        let c3 = client(&m3.url);
        assert_eq!(
            probe(&c3, "Trade Partner").await,
            Resolution::Resolved("trade-partner".into())
        );
        let req = &m3.requests()[0];
        assert!(
            req.starts_with("head /v1/profile/trade%20partner/reviews/ "),
            "{req}"
        );
        assert_eq!(m3.requests().len(), 1, "no confirmation call needed");
    }

    #[tokio::test]
    async fn no_redirect_is_confirmed_through_the_account_route() {
        let m = spawn(vec![
            reply(200, ""),
            reply(
                200,
                r#"{"data":{"slug":"partner_02","ingameName":"partner_02"}}"#,
            ),
        ]);
        assert_eq!(
            probe(&client(&m.url), "partner_02").await,
            Resolution::Resolved("partner_02".into())
        );
        assert!(m.requests()[1].starts_with("get /v2/user/partner_02 "));
    }

    #[tokio::test]
    async fn a_folded_name_that_belongs_to_someone_else_is_not_found() {
        let m = spawn(vec![
            reply(200, ""),
            reply(
                200,
                r#"{"data":{"slug":"squadmate","ingameName":"SquadMate"}}"#,
            ),
        ]);
        assert_eq!(
            probe(&client(&m.url), "Squad_Mate").await,
            Resolution::NotFound
        );
    }

    #[tokio::test]
    async fn failed_or_slugless_lookups_are_unresolved_and_a_404_is_not_found() {
        let m = spawn(vec![reply(200, ""), reply(500, "{}")]);
        assert_eq!(
            probe(&client(&m.url), "partner").await,
            Resolution::Unresolved
        );
        let m = spawn(vec![
            reply(200, ""),
            reply(200, r#"{"data":{"ingameName":"partner"}}"#),
        ]);
        assert_eq!(
            probe(&client(&m.url), "partner").await,
            Resolution::Unresolved
        );
        let m = spawn(vec![reply(200, ""), reply(404, "{}")]);
        assert_eq!(
            probe(&client(&m.url), "partner").await,
            Resolution::NotFound
        );
        assert_eq!(probe(&client(&m.url), "  ").await, Resolution::NotFound);
    }

    #[tokio::test]
    async fn a_redirect_target_that_is_not_a_slug_is_unresolved() {
        let m = spawn(vec![
            reply(301, "").header("Location", "https://warframe.market/login"),
        ]);
        assert_eq!(
            probe(&client(&m.url), "buyer").await,
            Resolution::Unresolved
        );
    }

    #[tokio::test]
    async fn own_slug_prefers_me_caches_and_falls_back_to_folding() {
        let m = spawn(vec![reply(200, r#"{"data":{"slug":"relay-fox-7"}}"#)]);
        let cache = SlugCache::default();
        let c = client(&m.url);
        assert_eq!(
            own_slug(&c, &cache, "Relay Fox").await.as_deref(),
            Some("relay-fox-7")
        );
        assert_eq!(
            own_slug(&c, &cache, "Relay Fox").await.as_deref(),
            Some("relay-fox-7")
        );
        assert_eq!(m.requests().len(), 1, "asked /me once");

        let m = spawn(vec![reply(200, r#"{"data":{}}"#)]);
        let cache = SlugCache::default();
        assert_eq!(
            own_slug(&client(&m.url), &cache, "Trade Partner")
                .await
                .as_deref(),
            Some("trade_partner")
        );
        let m = spawn(vec![reply(500, "{}")]);
        let cache = SlugCache::default();
        assert_eq!(
            own_slug(&client(&m.url), &cache, "Trade Partner")
                .await
                .as_deref(),
            Some("trade_partner")
        );
        assert!(
            cache.0.lock().unwrap().is_none(),
            "a failed answer is not cached"
        );
    }
}
