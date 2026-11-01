use crate::db::mem_db;

use super::*;

const BUNDLED: Option<&str> = Some("1.1276.6");

fn relic(i: usize, rewards: Vec<Value>) -> Value {
    json!({
        "uniqueName": format!("{RELIC_PATH_PREFIX}T1VoidProjectionA{i}Bronze"),
        "name": format!("Lith A{i} Intact"),
        "vaulted": false,
        "dropCount": 0,
        "rewards": rewards,
    })
}

fn reward(name: &str) -> Value {
    json!({"chance": 25.33, "rarity": "Common", "item": {"uniqueName": "/Lotus/X", "name": name}})
}

fn rows(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| relic(i, vec![reward(&format!("Part {}", i % 50))]))
        .collect()
}

fn doc_json(version: &str, relics: Vec<Value>) -> Value {
    json!({"version": version, "publishedAt": "2026-09-25T10:00:00.000Z", "relics": relics})
}

fn doc(version: &str) -> Value {
    doc_json(version, rows(2_600))
}

fn body(version: &str, tag: &str) -> Outcome {
    Outcome::Body(doc(version), Some(tag.into()))
}

fn applied_version(step: &Step) -> Option<&str> {
    match &step.swap {
        Some(Swap::Apply(d)) => Some(&d.version),
        _ => None,
    }
}

// ---- validation ----

#[test]
fn drops_each_malformed_row_shape_and_keeps_the_good_ones() {
    let good = relic(9_999, vec![reward("Good Prime Blueprint")]);
    let with_reward = |patch: Value| {
        let mut r = good.clone();
        for (k, v) in patch.as_object().unwrap() {
            r["rewards"][0][k] = v.clone();
        }
        r
    };
    let with_item = |patch: Value| {
        let mut r = good.clone();
        for (k, v) in patch.as_object().unwrap() {
            r["rewards"][0]["item"][k] = v.clone();
        }
        r
    };
    let with = |k: &str, v: Value| {
        let mut r = good.clone();
        r[k] = v;
        r
    };
    let malformed = vec![
        with("uniqueName", json!("/Lotus/Types/Recipes/NotARelic")),
        with(
            "uniqueName",
            json!(format!("{RELIC_PATH_PREFIX}X9999Platinum")),
        ),
        with("name", json!("Axi Z99")),
        with("name", json!("Omni Z99 Intact")),
        with("name", json!("Axi Z99 Pristine")),
        with("vaulted", json!("yes")),
        with("dropCount", json!(-1)),
        with("dropCount", json!(1.5)),
        with("rewards", json!([])),
        with("rewards", json!(vec![reward("x"); 13])),
        with("imageName", json!(7)),
        with("imageName", json!("x".repeat(257))),
        with_reward(json!({"chance": 0})),
        with_reward(json!({"chance": 101})),
        with_reward(json!({"chance": "2"})),
        with_reward(json!({"rarity": "Legendary"})),
        with_reward(json!({"item": null})),
        with_item(json!({"name": ""})),
        with_item(json!({"name": "x".repeat(65)})),
        with_item(json!({"uniqueName": 5})),
        with_item(json!({"ducats": "45"})),
        with_item(json!({"warframeMarket": {"urlName": 12}})),
        json!("not a row"),
    ];
    let mut all = rows(2_600);
    all.push(good.clone());
    all.extend(malformed);
    let doc = validate_doc(&doc_json("1.1277.0", all)).unwrap();
    assert_eq!(doc.relics.len(), 2_601);
    assert_eq!(doc.relics.last().unwrap()["uniqueName"], good["uniqueName"]);
    assert_eq!(
        doc.published_at.as_deref(),
        Some("2026-09-25T10:00:00.000Z")
    );
}

#[test]
fn rejects_the_whole_document_for_a_bad_version_or_row_count() {
    assert_eq!(
        validate_doc(&doc_json("1.1277", rows(2_600))),
        Err("bad version".into())
    );
    assert!(validate_doc(&doc_json("v1.1277.0", rows(2_600))).is_err());
    let err = validate_doc(&doc_json("1.1277.0", rows(2_400))).unwrap_err();
    assert!(err.ends_with("valid relics"), "{err}");
    assert!(validate_doc(&doc_json("1.1277.0", rows(10_001))).is_err());
    let renamed: Vec<Value> = (0..2_600)
        .map(|i| relic(i, vec![reward(&format!("Part {i}"))]))
        .collect();
    assert_eq!(
        validate_doc(&doc_json("1.1277.0", renamed)),
        Err("too many reward names".into())
    );
    assert!(validate_doc(&Value::Null).is_err());
}

#[test]
fn compares_versions_numerically() {
    assert!(is_newer("1.1277.0", "1.1276.6"));
    assert!(is_newer("1.1276.10", "1.1276.9"));
    assert!(!is_newer("1.1276.6", "1.1276.6"));
    assert!(!is_newer("1.1275.99", "1.1276.0"));
    assert!(!is_newer("latest", "1.1276.6"));
    assert!(is_same_major("1.5.0", "1.1276.6"));
    assert!(!is_same_major("2.0.0", "1.1276.6"));
}

// ---- state machine ----

#[test]
fn applies_a_newer_download_and_caches_it() {
    let mut u = Updater::default();
    let step = u.handle(body("1.1277.0", "\"a\""), BUNDLED);
    assert_eq!(applied_version(&step), Some("1.1277.0"));
    let cached = step.write_cache.unwrap();
    assert_eq!(cached.etag.as_deref(), Some("\"a\""));
    assert_eq!(cached.doc.unwrap()["version"], "1.1277.0");
    assert_eq!(
        (u.applied_version(), u.etag()),
        (Some("1.1277.0"), Some("\"a\""))
    );
}

#[test]
fn ignores_a_download_that_is_not_newer_than_the_bundle() {
    for version in ["1.1276.6", "1.1275.9", "2.0.0"] {
        let mut u = Updater::default();
        let step = u.handle(Outcome::Body(doc(version), None), BUNDLED);
        assert_eq!(step, Step::default(), "{version}");
    }
}

#[test]
fn remembers_only_the_tag_of_a_download_that_is_not_newer() {
    let mut u = Updater::default();
    let step = u.handle(body("1.1276.6", "\"r0\""), BUNDLED);
    assert_eq!(step.swap, None);
    assert_eq!(
        step.write_cache,
        Some(Cached {
            etag: Some("\"r0\"".into()),
            doc: None
        })
    );
    assert_eq!(u.etag(), Some("\"r0\""));
}

#[test]
fn follows_the_backend_back_to_an_older_doc() {
    let mut u = Updater::default();
    u.handle(body("1.1277.1", "\"e\""), BUNDLED);
    let step = u.handle(body("1.1277.0", "\"e2\""), BUNDLED);
    assert_eq!(applied_version(&step), Some("1.1277.0"));
}

#[test]
fn a_repeat_of_the_applied_version_only_refreshes_a_changed_tag() {
    let mut u = Updater::default();
    u.handle(body("1.1277.0", "\"a\""), BUNDLED);
    assert_eq!(
        u.handle(body("1.1277.0", "\"a\""), BUNDLED),
        Step::default()
    );
    let step = u.handle(body("1.1277.0", "\"b\""), BUNDLED);
    assert_eq!(step.swap, None);
    assert_eq!(step.write_cache.unwrap().etag.as_deref(), Some("\"b\""));
}

#[test]
fn reverts_when_the_backend_serves_a_doc_the_bundle_has_overtaken() {
    let mut u = Updater::default();
    u.handle(body("1.1277.0", "\"a\""), BUNDLED);
    let step = u.handle(body("1.1277.0", "\"b\""), Some("1.1278.0"));
    // Same version as applied: nothing to do until the version changes.
    assert_eq!(step.swap, None);
    let step = u.handle(body("1.1277.5", "\"c\""), Some("1.1278.0"));
    assert_eq!(step.swap, Some(Swap::Revert));
    assert_eq!(
        step.write_cache,
        Some(Cached {
            etag: Some("\"c\"".into()),
            doc: None
        })
    );
    assert_eq!(u.applied_version(), None);
}

#[test]
fn trims_a_raw_wfcd_row_to_the_validated_shape() {
    let raw = json!({
        "uniqueName": "/Lotus/Types/Game/Projections/T1VoidProjectionAxi1Bronze",
        "name": "Axi A1 Intact", "vaulted": true, "drops": [{}, {}], "imageName": "x.png",
        "tradable": true,
        "rewards": [{"chance": 25.33, "rarity": "Common", "item": {
            "uniqueName": "/Lotus/A", "name": "A Prime Blueprint", "ducats": 45,
            "warframeMarket": {"id": "i", "url_name": "a_prime_blueprint", "extra": 1}}}],
    });
    assert_eq!(
        trim_relic(&raw).unwrap(),
        json!({
            "uniqueName": "/Lotus/Types/Game/Projections/T1VoidProjectionAxi1Bronze",
            "name": "Axi A1 Intact", "vaulted": true, "dropCount": 2, "imageName": "x.png",
            "rewards": [{"chance": 25.33, "rarity": "Common", "item": {
                "uniqueName": "/Lotus/A", "name": "A Prime Blueprint", "ducats": 45,
                "warframeMarket": {"id": "i", "urlName": "a_prime_blueprint"}}}],
        })
    );
    assert_eq!(trim_relic(&json!({"name": "no rewards"})), None);
}

#[test]
fn failures_and_invalid_documents_keep_what_is_in_use() {
    let mut u = Updater::default();
    for outcome in [
        Outcome::Failed("offline".into()),
        Outcome::NotModified,
        Outcome::Body(doc("latest"), None),
        Outcome::Body(Value::Null, None),
    ] {
        assert_eq!(u.handle(outcome, BUNDLED), Step::default());
    }
    u.handle(body("1.1277.0", "\"a\""), BUNDLED);
    assert_eq!(
        u.handle(Outcome::Failed("x".into()), BUNDLED),
        Step::default()
    );
    assert_eq!(u.applied_version(), Some("1.1277.0"));
}

#[test]
fn applies_the_cached_copy_until_a_newer_bundle_replaces_it() {
    let cached = Cached {
        etag: Some("\"r1\"".into()),
        doc: Some(doc("1.1277.0")),
    };
    let mut u = Updater::default();
    let step = u.apply_cache(Some(cached.clone()), BUNDLED);
    assert_eq!(applied_version(&step), Some("1.1277.0"));
    assert_eq!(u.etag(), Some("\"r1\""));

    let mut later = Updater::default();
    let step = later.apply_cache(Some(cached), Some("1.1278.0"));
    assert_eq!(step.swap, None);
    // The rows that lost to the bundle are dropped; the tag stays for a 304.
    assert_eq!(
        step.write_cache,
        Some(Cached {
            etag: Some("\"r1\"".into()),
            doc: None
        })
    );
    assert_eq!(later.etag(), Some("\"r1\""));
}

#[test]
fn nothing_is_applied_without_a_known_bundled_version() {
    let mut u = Updater::default();
    let step = u.handle(body("1.1277.0", "\"a\""), None);
    assert_eq!(step.swap, None);
}

// ---- cache io ----

#[test]
fn cache_round_trips_and_a_corrupt_one_reads_as_none() {
    let db = mem_db();
    assert_eq!(read_cache(&db), None);
    let cached = Cached {
        etag: Some("\"t\"".into()),
        doc: None,
    };
    write_cache(&db, &cached);
    assert_eq!(read_cache(&db), Some(cached));
    let with_doc = Cached {
        etag: None,
        doc: Some(doc("1.1277.0")),
    };
    write_cache(&db, &with_doc);
    assert_eq!(read_cache(&db), Some(with_doc));
    crate::services::cache::kv_put(&db, CACHE_KEY, "{ not json").unwrap();
    assert_eq!(read_cache(&db), None);
    write_cache(
        &db,
        &Cached {
            etag: None,
            doc: Some(doc("latest")),
        },
    );
    assert_eq!(read_cache(&db), None);
}

#[test]
fn picks_the_newest_old_enough_undeprecated_release() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-10-26T12:00:00Z")
        .unwrap()
        .to_utc();
    let pack = json!({
        "dist-tags": {"latest": "1.1280.0"},
        "versions": {
            "1.1276.6": {}, "1.1277.0": {}, "1.1278.0": {"deprecated": "bad"},
            "1.1279.0": {}, "1.1280.0": {}, "1.1281.0": {},
        },
        "time": {
            "created": "2020-01-01T00:00:00Z",
            "1.1276.6": "2026-10-01T00:00:00Z",
            "1.1277.0": "2026-10-20T00:00:00Z",
            "1.1278.0": "2026-10-22T00:00:00Z",
            "1.1279.0": "2026-10-26T00:00:00Z",
            "1.1280.0": "2026-10-25T13:00:00Z",
            "1.1281.0": "2026-10-01T00:00:00Z",
        },
    });
    // 1.1278 is deprecated, 1.1279 and 1.1280 are under 24 h old, 1.1281 is above `latest`.
    assert_eq!(pick_releases(&pack, now), ["1.1277.0", "1.1276.6"]);
    assert!(pick_releases(&json!({}), now).is_empty());
}

const GOOD: &[u8] = include_bytes!("../../../tests/fixtures/relics/Relics.1276.29.json");
/// The same rows from 1.1276.36, where upstream emptied every reward list.
const EMPTY_REWARDS: &[u8] = include_bytes!("../../../tests/fixtures/relics/Relics.1276.36.json");

#[test]
fn trims_the_real_wfcd_table_with_its_rewards() {
    let rows = trim_table(GOOD);
    assert_eq!(rows.len(), 4);
    let lith = rows.iter().find(|r| r["name"] == "Lith A1 Intact").unwrap();
    let rewards = lith["rewards"].as_array().unwrap();
    assert_eq!(rewards.len(), 6);
    let rare = &rewards[3];
    assert_eq!(rare["item"]["name"], "Akstiletto Prime Blueprint");
    assert_eq!(
        (rare["rarity"].as_str(), rare["chance"].as_f64()),
        (Some("Rare"), Some(2.0))
    );
}

#[test]
fn a_release_with_empty_reward_lists_is_not_usable() {
    // Every row has the right name and path but no rewards, so validation drops them all.
    let doc = json!({"version": "1.1276.36", "relics": trim_table(EMPTY_REWARDS)});
    assert_eq!(validate_doc(&doc), Err("0 valid relics".into()));
}

#[test]
fn a_malformed_download_keeps_the_bundled_data_and_reports_why() {
    let mut u = Updater::default();
    let bad = json!({"version": "1.1276.36", "relics": trim_table(EMPTY_REWARDS)});
    let step = u.handle(
        Outcome::Body(bad, Some("1.1276.36".into())),
        Some("1.1276.6"),
    );
    assert_eq!(step, Step::default());
    assert_eq!(u.error(), Some("download invalid: 0 valid relics"));
    // A network problem is reported as offline, not as bad data.
    u.handle(Outcome::Offline("timeout".into()), Some("1.1276.6"));
    assert_eq!((u.error(), u.offline()), (Some("timeout"), true));
    // A later good check clears the reason.
    u.handle(Outcome::NotModified, Some("1.1276.6"));
    assert_eq!(u.error(), None);
}

fn versions(range: std::ops::RangeInclusive<u32>) -> Vec<String> {
    range.rev().map(|p| format!("1.1276.{p}")).collect()
}

#[tokio::test]
async fn walks_back_past_releases_without_relic_data_and_remembers_them() {
    // 1.1276.30 to .40 lost their rewards upstream; .29 is the newest usable one.
    let releases = versions(1..=40);
    let good = serde_json::to_vec(&rows(2_600)).unwrap();
    let fetched = std::cell::RefCell::new(Vec::new());
    let fetch = |v: String| {
        fetched.borrow_mut().push(v.clone());
        let patch: u32 = v.rsplit('.').next().unwrap().parse().unwrap();
        let body = if patch >= 30 {
            EMPTY_REWARDS.to_vec()
        } else {
            good.clone()
        };
        async move { Ok(body) }
    };
    let mut bad = std::collections::HashSet::new();
    let out = walk(&releases, None, BUNDLED, &mut bad, fetch).await;
    assert!(
        matches!(&out, Outcome::Body(d, Some(t)) if d["version"] == "1.1276.29" && t == "1.1276.29")
    );
    assert_eq!(bad.len(), 11);
    assert!(newer_unusable(&bad, Some("1.1276.29")));
    let mut u = Updater::default();
    assert_eq!(applied_version(&u.handle(out, BUNDLED)), Some("1.1276.29"));
    assert_eq!(u.error(), None);

    // The next check skips the known-bad releases and stops at the one in use: no download.
    fetched.borrow_mut().clear();
    let out = walk(&releases, Some("1.1276.29"), BUNDLED, &mut bad, fetch).await;
    assert!(matches!(out, Outcome::NotModified));
    assert!(fetched.borrow().is_empty());

    // A newer good release is taken over the one in use.
    let releases = versions(1..=41);
    let fetch41 = |v: String| {
        let body = if v == "1.1276.41" {
            good.clone()
        } else {
            EMPTY_REWARDS.to_vec()
        };
        async move { Ok(body) }
    };
    let out = walk(&releases, Some("1.1276.29"), BUNDLED, &mut bad, fetch41).await;
    assert!(matches!(&out, Outcome::Body(d, _) if d["version"] == "1.1276.41"));
    assert!(!newer_unusable(&bad, Some("1.1276.41")));
}

#[tokio::test]
async fn with_no_usable_release_above_the_bundle_it_goes_back_to_the_bundle() {
    let releases = versions(1..=12);
    let mut bad = std::collections::HashSet::new();
    let empty = |_: String| async { Ok(EMPTY_REWARDS.to_vec()) };
    let out = walk(&releases, Some("1.1276.9"), BUNDLED, &mut bad, empty).await;
    // .9 is in use and still listed, so the walk stops there.
    assert!(matches!(out, Outcome::NotModified));
    // Once .9 is deprecated (unlisted), nothing above the bundle is usable.
    let unlisted: Vec<String> = releases.into_iter().filter(|v| v != "1.1276.9").collect();
    let out = walk(&unlisted, Some("1.1276.9"), BUNDLED, &mut bad, empty).await;
    assert!(matches!(out, Outcome::Bundled));
    // Nothing at or below the bundled version is downloaded.
    assert!(bad.iter().all(|v| is_newer(v, "1.1276.6")));
    assert!(newer_unusable(&bad, BUNDLED));

    let mut u = Updater::default();
    u.handle(body("1.1277.0", "1.1277.0"), BUNDLED);
    let step = u.handle(Outcome::Bundled, BUNDLED);
    assert_eq!(step.swap, Some(Swap::Revert));
    assert_eq!(u.etag(), None);

    let offline = |_: String| async { Err::<Vec<u8>, _>("timeout".to_owned()) };
    let out = walk(&versions(1..=50), None, BUNDLED, &mut bad, offline).await;
    assert!(matches!(out, Outcome::Offline(_)));
}
