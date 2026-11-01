//! "+rep" (port of WFHelper `wfmReviews.ts`): one positive review for a trade partner, by user action
//! only. A review is outward-facing, so an unconfirmed name is never written to: the names WFM
//! reshapes are exactly the ones that collide with another user.

use serde::Serialize;
use tauri::State;

use super::client::{Api, Sent, WfmClient};
use super::errors::WfmError;
use super::legacy_v1;
use super::profile_slug::{self, Resolution};
use super::session::Session;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum SendRepResult {
    Sent,
    AlreadyExists,
    UserNotFound,
    ProfileUnresolved,
    Failed,
}

fn from_error(e: &WfmError) -> SendRepResult {
    match e {
        e if e.to_string().contains("app.review.already_exist") => SendRepResult::AlreadyExists,
        WfmError::Api { status: 404, .. } => SendRepResult::UserNotFound,
        _ => SendRepResult::Failed,
    }
}

pub async fn send_plus_rep(client: &WfmClient, username: &str) -> SendRepResult {
    let name = username.trim();
    if name.is_empty() || client.token().is_none() {
        return SendRepResult::Failed;
    }
    let slug = match profile_slug::probe(client, name).await {
        Resolution::Resolved(slug) => slug,
        Resolution::NotFound => return SendRepResult::UserNotFound,
        Resolution::Unresolved => return SendRepResult::ProfileUnresolved,
    };
    let path = format!("/profile/{}/review", legacy_v1::urlencode(&slug));
    let sent = match legacy_v1::review_at(client, &path).await {
        // WFM answers a POST to a non-canonical slug with a 301 and never re-sends the body, so
        // re-send once, and only to another slug on the review route itself.
        Ok(Sent::Redirect(loc)) => {
            let to = loc
                .and_then(|l| profile_slug::slug_in(&l, client.base(Api::V1), "review"))
                .map(|s| format!("/profile/{}/review", legacy_v1::urlencode(&s)))
                .filter(|p| *p != path);
            match to {
                Some(p) => legacy_v1::review_at(client, &p).await,
                None => return SendRepResult::Failed,
            }
        }
        other => other,
    };
    match sent {
        Ok(Sent::Done(_)) => SendRepResult::Sent,
        Ok(Sent::Redirect(_)) => SendRepResult::Failed,
        Err(e) => {
            tracing::warn!(error = %e, "wfm +rep failed");
            from_error(&e)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn wfm_send_plus_rep(
    session: State<'_, Session>,
    username: String,
) -> Result<SendRepResult, WfmError> {
    Ok(send_plus_rep(session.client(), &username).await)
}

#[cfg(test)]
mod tests {
    use super::super::mock::{Mock, Reply, reply, spawn};
    use super::*;

    fn client(m: &Mock) -> WfmClient {
        let c = WfmClient::new(
            &format!("{}/v1", m.url),
            &format!("{}/v2", m.url),
            Box::new(|_| {}),
        )
        .unwrap();
        c.set_token(Some("tok".into()));
        c
    }

    /// The HEAD probe's answer: a redirect to the slug's reviews page (relative, like the real API).
    fn to_slug(slug: &str) -> Reply {
        reply(301, "").header("Location", &format!("/v1/profile/{slug}/reviews/"))
    }

    fn user(slug: &str, name: &str) -> Reply {
        reply(
            200,
            &format!(r#"{{"data":{{"slug":"{slug}","ingameName":"{name}"}}}}"#),
        )
    }

    async fn rep(script: Vec<Reply>, name: &str) -> (SendRepResult, Vec<String>) {
        let m = spawn(script);
        let r = send_plus_rep(&client(&m), name).await;
        (r, m.requests())
    }

    #[tokio::test]
    async fn posts_to_the_slug_wfm_redirects_the_name_to() {
        for (name, slug) in [
            ("Squad_Mate", "squad-mate"),
            ("Trade Partner", "trade-partner"),
            (".Courier.", "courier"),
            ("-Alt-Handle", "alt-handle"),
        ] {
            let (r, reqs) = rep(vec![to_slug(slug), reply(200, "{}")], name).await;
            assert_eq!(r, SendRepResult::Sent, "{name}");
            assert_eq!(reqs.len(), 2, "no confirmation call needed");
            assert!(reqs[0].starts_with("head /v1/profile/"), "{}", reqs[0]);
            assert!(
                reqs[1].starts_with(&format!("post /v1/profile/{slug}/review ")),
                "{}",
                reqs[1]
            );
            assert!(
                reqs[1].ends_with(r#"{"review_type":1,"text":""}"#),
                "{}",
                reqs[1]
            );
        }
    }

    #[tokio::test]
    async fn posts_to_the_name_itself_when_wfm_confirms_it() {
        let (r, reqs) = rep(
            vec![
                reply(200, ""),
                user("partner_02", "partner_02"),
                reply(200, "{}"),
            ],
            "partner_02",
        )
        .await;
        assert_eq!(r, SendRepResult::Sent);
        assert!(reqs[1].starts_with("get /v2/user/partner_02 "));
        assert!(reqs[2].starts_with("post /v1/profile/partner_02/review "));
    }

    #[tokio::test]
    async fn refuses_to_post_when_the_profile_cannot_be_confirmed() {
        let cases: Vec<(Vec<Reply>, SendRepResult)> = vec![
            (
                vec![reply(200, ""), reply(500, "{}")],
                SendRepResult::ProfileUnresolved,
            ),
            (
                vec![reply(200, ""), reply(200, r#"{"data":{"ingameName":"p"}}"#)],
                SendRepResult::ProfileUnresolved,
            ),
            (
                vec![reply(301, "").header("Location", "https://warframe.market/login")],
                SendRepResult::ProfileUnresolved,
            ),
            (
                vec![reply(200, ""), user("squadmate", "SquadMate")],
                SendRepResult::UserNotFound,
            ),
            (
                vec![reply(200, ""), reply(404, "{}")],
                SendRepResult::UserNotFound,
            ),
        ];
        for (script, expected) in cases {
            let (r, reqs) = rep(script, "p").await;
            assert_eq!(r, expected);
            assert!(reqs.iter().all(|q| !q.starts_with("post ")), "{reqs:?}");
        }
    }

    #[tokio::test]
    async fn maps_the_review_errors() {
        let dup = reply(400, r#"{"error":{"_form":["app.review.already_exist"]}}"#);
        let (r, _) = rep(vec![to_slug("p"), dup], "p").await;
        assert_eq!(r, SendRepResult::AlreadyExists);
        let (r, _) = rep(vec![to_slug("p"), reply(404, "{}")], "p").await;
        assert_eq!(r, SendRepResult::UserNotFound);
        let (r, _) = rep(vec![to_slug("p"), reply(500, "{}")], "p").await;
        assert_eq!(r, SendRepResult::Failed);
    }

    #[tokio::test]
    async fn resends_once_to_the_slug_a_301_on_the_post_points_at() {
        let moved = reply(301, "").header("Location", "/v1/profile/krakenzer-2/review");
        let (r, reqs) = rep(
            vec![to_slug("krakenzer"), moved, reply(200, "{}")],
            "KraKenZer",
        )
        .await;
        assert_eq!(r, SendRepResult::Sent);
        assert!(reqs[1].starts_with("post /v1/profile/krakenzer/review "));
        assert!(reqs[2].starts_with("post /v1/profile/krakenzer-2/review "));
    }

    #[tokio::test]
    async fn does_not_follow_a_redirect_that_leaves_the_review_endpoint() {
        for loc in ["https://warframe.market/login", "/v1/profile/buyer/review"] {
            let moved = reply(302, "").header("Location", loc);
            let (r, reqs) = rep(vec![reply(200, ""), user("buyer", "Buyer"), moved], "Buyer").await;
            assert_eq!(r, SendRepResult::Failed, "{loc}");
            assert_eq!(reqs.len(), 3, "{loc}: no second POST");
        }
    }

    #[tokio::test]
    async fn an_empty_name_or_signed_out_client_never_reaches_wfm() {
        let m = spawn(vec![]);
        let c = client(&m);
        assert_eq!(send_plus_rep(&c, "   ").await, SendRepResult::Failed);
        c.set_token(None);
        assert_eq!(send_plus_rep(&c, "p").await, SendRepResult::Failed);
        assert!(m.requests().is_empty());
    }
}
