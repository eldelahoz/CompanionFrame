//! Rules, store and engine tests: local mock server only, no real network or credentials.

use crate::db::mem_db;
use std::sync::Mutex;

use serde_json::{Value, json};

use super::checker::{Ctx, Engine};
use super::rules::*;
use super::store;
use crate::db::Db;
use crate::services::notify::{NotificationDispatch, Notifier};
use crate::services::wfm::client::WfmClient;
use crate::services::wfm::mock::{Mock, reply, spawn};

const MIN: i64 = 60_000;

fn rule(v: Value) -> Result<MarketAlertRule, String> {
    let mut r: MarketAlertRule = serde_json::from_value(v).map_err(|e| e.to_string())?;
    r.validate()?;
    Ok(r)
}

fn riven(extra: Value) -> Value {
    let mut m = json!({"weaponUrlName": "rubico", "requirePositive": ["critical_chance"]});
    m.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"id": "r1", "name": "Crit Rubico", "kind": "riven", "riven": m})
}

fn item(extra: Value) -> Value {
    let mut m = json!({"itemUrlName": "nekros_prime_set", "side": "sell", "maxPlatinum": 50});
    m.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"id": "i1", "name": "Cheap Set", "kind": "item", "item": m})
}

fn err_of(v: Value) -> String {
    rule(v).unwrap_err()
}

// --- rules ------------------------------------------------------------------------------------

#[test]
fn accepts_a_riven_rule_and_fills_defaults() {
    let r = rule(riven(json!({}))).unwrap();
    assert!(r.enabled && !r.no_cooldown);
    assert_eq!(r.cooldown_minutes, DEFAULT_COOLDOWN_MINUTES);
    assert!(r.riven.unwrap().exclude_attributes.is_empty());
}

#[test]
fn rejects_a_display_name_where_a_slug_belongs() {
    assert!(err_of(riven(json!({"weaponUrlName": "Rubico Prime"}))).contains("not a slug"));
    assert!(rule(riven(json!({"weaponUrlName": "kuva_bramma"}))).is_ok());
}

#[test]
fn rejects_attributes_outside_the_vocabulary() {
    assert!(
        err_of(riven(json!({"requirePositive": ["critical_chance&x=1"]})))
            .contains("unknown attribute")
    );
    assert!(err_of(riven(json!({"excludeAttributes": ["nope"]}))).contains("unknown attribute"));
    assert!(
        rule(riven(
            json!({"requirePositive": ["base_damage_/_melee_damage"]})
        ))
        .is_ok()
    );
}

#[test]
fn keeps_only_the_first_bound_per_attribute() {
    let r = rule(riven(json!({"statBounds": [
        {"attribute": "critical_chance", "min": 100},
        {"attribute": "critical_chance", "min": 200},
    ]})))
    .unwrap();
    let b = r.riven.unwrap().stat_bounds;
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].min, Some(100.0));
}

#[test]
fn rejects_bad_bounds() {
    assert!(
        err_of(riven(
            json!({"statBounds": [{"attribute": "critical_chance"}]})
        ))
        .contains("min or a max")
    );
    assert!(
        err_of(riven(
            json!({"statBounds": [{"attribute": "critical_chance", "min": 5, "max": 1}]})
        ))
        .contains("above max")
    );
}

#[test]
fn rejects_unknown_fields_anywhere() {
    assert!(rule(riven(json!({"bogus": 1}))).is_err());
    let mut v = riven(json!({}));
    v["bogus"] = json!(1);
    assert!(rule(v).is_err());
}

#[test]
fn rejects_a_kind_and_section_that_disagree() {
    let mut v = riven(json!({}));
    v["kind"] = json!("item");
    assert!(rule(v).unwrap_err().contains("cannot carry"));
    let v = json!({"name": "x", "kind": "item"});
    assert!(rule(v).is_err());
}

#[test]
fn rejects_contradictory_riven_criteria() {
    assert!(
        err_of(riven(json!({"excludeAttributes": ["critical_chance"]}))).contains("contradicts")
    );
    assert!(err_of(riven(json!({"minPlatinum": 10, "maxPlatinum": 5}))).contains("above"));
}

#[test]
fn bounds_every_numeric_field() {
    assert!(err_of(riven(json!({"minModRank": 9}))).contains("out of range"));
    assert!(err_of(riven(json!({"positiveCount": 4}))).contains("out of range"));
    let mut v = riven(json!({}));
    v["cooldownMinutes"] = json!(1);
    assert!(rule(v).unwrap_err().contains("cooldown"));
}

#[test]
fn dedupes_attribute_lists_and_statuses() {
    let r = rule(riven(json!({
        "allowedNegatives": ["zoom", "zoom"], "statuses": ["ingame", "ingame"]
    })))
    .unwrap();
    let m = r.riven.unwrap();
    assert_eq!(m.allowed_negatives.unwrap(), ["zoom"]);
    assert_eq!(m.statuses, [SellerStatus::Ingame]);
}

#[test]
fn item_rules_need_a_threshold_and_valid_statuses() {
    assert!(rule(item(json!({}))).is_ok());
    let mut v = item(json!({}));
    v["item"].as_object_mut().unwrap().remove("maxPlatinum");
    assert!(rule(v).unwrap_err().contains("price threshold"));
    assert!(rule(item(json!({"statuses": ["afk"]}))).is_err());
    assert!(err_of(item(json!({"minQuantity": 0}))).contains("out of range"));
}

#[test]
fn baro_rules_round_trip_but_are_validated() {
    let v = json!({"name": "b", "kind": "baro", "baro": {"itemUrlName": "x", "maxDucats": 100}});
    assert!(rule(v).is_ok());
    let v = json!({"name": "b", "kind": "baro", "baro": {"itemUrlName": "X Y"}});
    assert!(rule(v).is_err());
}

#[test]
fn no_cooldown_keeps_the_minutes() {
    let mut v = riven(json!({}));
    v["noCooldown"] = json!(true);
    v["cooldownMinutes"] = json!(30);
    let r = rule(v).unwrap();
    assert!(r.no_cooldown && r.cooldown_minutes == 30);
}

#[test]
fn export_round_trips_with_fresh_ids() {
    let r = rule(riven(json!({"polarity": "madurai"}))).unwrap();
    let text = build_export(std::slice::from_ref(&r));
    assert!(!text.contains("native"));
    let back = parse_import(&text).unwrap();
    assert_eq!(back.len(), 1);
    assert!(back[0].id.is_empty());
    assert_eq!(back[0].riven, r.riven);
}

#[test]
fn import_rejects_hostile_files() {
    assert!(
        parse_import(&"x".repeat(IMPORT_MAX_BYTES + 1))
            .unwrap_err()
            .contains("too large")
    );
    assert!(parse_import("nope").is_err());
    assert!(
        parse_import(r#"{"schema":2,"rules":[]}"#)
            .unwrap_err()
            .contains("schema")
    );
    assert!(parse_import(r#"{"schema":1,"rules":[],"extra":1}"#).is_err());
    let many = vec![json!({}); MAX_RULES + 1];
    let text = json!({"schema": 1, "rules": many}).to_string();
    assert!(parse_import(&text).unwrap_err().contains("too many"));
    let bad =
        json!({"schema": 1, "rules": [riven(json!({})), riven(json!({"weaponUrlName": "A B"}))]});
    assert!(
        parse_import(&bad.to_string())
            .unwrap_err()
            .contains("rule 2")
    );
}

// --- matching ---------------------------------------------------------------------------------

fn auction(id: &str, extra: Value) -> Value {
    let mut a = json!({
        "id": id, "buyout_price": 100, "starting_price": 10,
        "owner": {"ingame_name": "Seller", "status": "Online"},
        "item": {"mastery_level": 13, "mod_rank": 0, "re_rolls": 0, "polarity": "madurai",
                 "attributes": [{"url_name": "critical_chance", "value": 100, "positive": true}]},
    });
    for (k, v) in extra.as_object().unwrap() {
        match k.as_str() {
            "attributes" | "mod_rank" | "re_rolls" | "mastery_level" => a["item"][k] = v.clone(),
            "status" => a["owner"]["status"] = v.clone(),
            "owner" => a["owner"]["ingame_name"] = v.clone(),
            _ => a[k] = v.clone(),
        }
    }
    a
}

fn first(v: Value) -> Auction {
    parse_auctions(&json!({"payload": {"auctions": [v]}})).remove(0)
}

fn m(extra: Value) -> RivenAlertMatch {
    rule(riven(extra)).unwrap().riven.unwrap()
}

#[test]
fn parses_an_auction_view() {
    let a = first(auction("a1", json!({})));
    assert_eq!(
        (a.platinum, a.seller_status.as_str(), a.bid_only),
        (100.0, "online", false)
    );
    let bid = first(auction("a2", json!({"buyout_price": null})));
    assert!(bid.bid_only && bid.platinum == 10.0);
    assert!(parse_auctions(&json!({})).is_empty());
}

#[test]
fn riven_hit_leads_with_stats_and_endo() {
    let r = rule(riven(json!({}))).unwrap();
    let h = riven_hit(&r, &first(auction("abc", json!({}))));
    assert_eq!(h.url, "https://warframe.market/auction/abc");
    assert_eq!((h.platinum, h.endo), (Some(100.0), Some(515)));
    assert!((h.endo_per_plat.unwrap() - 5.2).abs() < 0.05);
    assert!(
        h.detail
            .starts_with("+100% Critical Chance - 100p - MR13 r0 0 rerolls - 515 endo")
    );
}

#[test]
fn curse_is_signed_by_the_flag() {
    let a = first(auction(
        "a",
        json!({"attributes": [
        {"url_name": "critical_chance", "value": 90, "positive": true},
        {"url_name": "zoom", "value": 40, "positive": false}]}),
    ));
    assert!(stat_line(&a).contains("-40% Zoom"));
}

fn stat_line(a: &Auction) -> String {
    riven_hit(&rule(riven(json!({}))).unwrap(), a).detail
}

#[test]
fn attributes_match_by_exact_url_name() {
    let slide = first(auction(
        "a",
        json!({"attributes": [
        {"url_name": "critical_chance_on_slide_attack", "value": 50, "positive": true}]}),
    ));
    assert!(!match_riven(&m(json!({})), &slide));
    assert!(match_riven(
        &m(json!({"requirePositive": ["critical_chance_on_slide_attack"]})),
        &slide
    ));
}

#[test]
fn curse_gates() {
    let clean = first(auction("c", json!({})));
    let cursed = first(auction(
        "d",
        json!({"attributes": [
        {"url_name": "critical_chance", "value": 90, "positive": true},
        {"url_name": "zoom", "value": 40, "positive": false}]}),
    ));
    assert!(match_riven(&m(json!({"hasNegative": false})), &clean));
    assert!(!match_riven(&m(json!({"hasNegative": false})), &cursed));
    assert!(match_riven(&m(json!({"hasNegative": true})), &cursed));
    assert!(!match_riven(&m(json!({"hasNegative": true})), &clean));
    assert!(match_riven(
        &m(json!({"allowedNegatives": ["zoom"]})),
        &cursed
    ));
    assert!(!match_riven(
        &m(json!({"allowedNegatives": ["recoil"]})),
        &cursed
    ));
    assert!(match_riven(
        &m(json!({"allowedNegatives": ["recoil"]})),
        &clean
    ));
    assert!(!match_riven(
        &m(json!({"excludeNegatives": ["zoom"]})),
        &cursed
    ));
    // The same stat as a buff is not excluded by excludeNegatives.
    let buff = first(auction(
        "e",
        json!({"attributes": [
        {"url_name": "critical_chance", "value": 90, "positive": true},
        {"url_name": "zoom", "value": 40, "positive": true}]}),
    ));
    assert!(match_riven(
        &m(json!({"excludeNegatives": ["zoom"]})),
        &buff
    ));
}

#[test]
fn positive_count_and_similarity() {
    let two = first(auction(
        "a",
        json!({"attributes": [
        {"url_name": "critical_chance", "value": 90, "positive": true},
        {"url_name": "multishot", "value": 80, "positive": true}]}),
    ));
    assert!(match_riven(&m(json!({"positiveCount": 2})), &two));
    assert!(!match_riven(&m(json!({"positiveCount": 3})), &two));
    let want = json!({"requirePositive": ["critical_chance", "critical_damage"]});
    assert!(!match_riven(&m(want.clone()), &two));
    let mut half = want;
    half["minSimilarityPct"] = json!(50);
    assert!(match_riven(&m(half), &two));
}

#[test]
fn stat_bounds_compare_at_max_rank() {
    let unranked = first(auction(
        "u",
        json!({"mod_rank": 0, "attributes": [
        {"url_name": "critical_chance", "value": 24.4, "positive": true}]}),
    ));
    let weak = first(auction(
        "w",
        json!({"mod_rank": 8, "attributes": [
        {"url_name": "critical_chance", "value": 10, "positive": true}]}),
    ));
    let rule = m(json!({"statBounds": [{"attribute": "critical_chance", "min": 100}]}));
    assert!(match_riven(&rule, &unranked));
    assert!(!match_riven(&rule, &weak));
    let missing = m(json!({"statBounds": [{"attribute": "multishot", "min": 1}]}));
    assert!(!match_riven(&missing, &unranked));
}

#[test]
fn bid_only_needs_an_opt_in() {
    let bid = first(auction("b", json!({"buyout_price": null})));
    assert!(!match_riven(&m(json!({})), &bid));
    assert!(match_riven(&m(json!({"includeBidOnly": true})), &bid));
    assert!(
        riven_hit(&rule(riven(json!({}))).unwrap(), &bid)
            .detail
            .contains("starting bid")
    );
}

#[test]
fn numeric_filters_status_polarity_and_endo() {
    let a = first(auction("a", json!({})));
    assert!(match_riven(
        &m(json!({"maxPlatinum": 100, "minMasteryRank": 13})),
        &a
    ));
    assert!(!match_riven(&m(json!({"maxPlatinum": 99})), &a));
    assert!(!match_riven(&m(json!({"minRerolls": 1})), &a));
    assert!(!match_riven(&m(json!({"polarity": "vazarin"})), &a));
    assert!(match_riven(
        &m(json!({"statuses": ["online", "ingame"]})),
        &a
    ));
    assert!(!match_riven(&m(json!({"statuses": ["ingame"]})), &a));
    assert!(match_riven(&m(json!({"minEndoPerPlat": 5})), &a));
    assert!(!match_riven(&m(json!({"minEndoPerPlat": 6})), &a));
}

#[test]
fn search_path_pushes_only_what_wfm_honours() {
    let p = riven_search_path(&m(json!({
        "requirePositive": ["critical_chance", "critical_damage"], "hasNegative": true,
        "allowedNegatives": ["zoom"], "polarity": "madurai", "minMasteryRank": 9,
        "maxMasteryRank": 15, "minRerolls": 1, "maxRerolls": 50
    })));
    for part in [
        "weapon_url_name=rubico",
        "positive_stats=critical_chance%2Ccritical_damage",
        "negative_stats=zoom",
        "polarity=madurai",
        "mastery_rank_min=9",
        "mastery_rank_max=15",
        "re_rolls_min=1",
        "re_rolls_max=50",
        "sort_by=price_asc",
    ] {
        assert!(p.contains(part), "{p} lacks {part}");
    }
    assert!(!p.contains("similarity"));
    let two = riven_search_path(&m(
        json!({"hasNegative": true, "allowedNegatives": ["zoom", "recoil"]}),
    ));
    assert!(!two.contains("negative_stats"));
    let partial = riven_search_path(&m(json!({"minSimilarityPct": 50})));
    assert!(!partial.contains("positive_stats"));
    let slash = riven_search_path(&m(
        json!({"requirePositive": ["base_damage_/_melee_damage"]}),
    ));
    assert!(slash.contains("base_damage_%2F_melee_damage"));
}

fn order(v: Value) -> crate::services::wfm::orders::BookOrder {
    let mut o = json!({"id": "o1", "type": "sell", "platinum": 40, "quantity": 1,
                       "user": {"ingameName": "Other", "status": "ingame"}});
    o.as_object_mut()
        .unwrap()
        .extend(v.as_object().unwrap().clone());
    serde_json::from_value(o).unwrap()
}

#[test]
fn item_orders_match_on_side_price_status_quantity_and_owned() {
    let im = |extra| rule(item(extra)).unwrap().item.unwrap();
    let o = order(json!({}));
    assert!(match_item_order(&im(json!({})), &o, None));
    assert!(!match_item_order(&im(json!({"maxPlatinum": 30})), &o, None));
    assert!(!match_item_order(&im(json!({"side": "buy"})), &o, None));
    assert!(!match_item_order(&im(json!({"minPlatinum": 41})), &o, None));
    assert!(!match_item_order(
        &im(json!({"statuses": ["online"]})),
        &o,
        None
    ));
    assert!(!match_item_order(&im(json!({"minQuantity": 2})), &o, None));
    let gate = im(json!({"ownedBelow": 2}));
    assert!(match_item_order(&gate, &o, Some(1)));
    assert!(!match_item_order(&gate, &o, Some(2)));
    // No owned count known: the gate fails open.
    assert!(match_item_order(&gate, &o, None));
    assert!(!match_item_order(
        &im(json!({"ownedAbove": 3})),
        &o,
        Some(3)
    ));
}

#[test]
fn item_hit_reads_like_warframe_market() {
    let r = rule(item(json!({}))).unwrap();
    let h = item_hit(
        &r,
        r.item.as_ref().unwrap(),
        &order(json!({"quantity": 3})),
        &item_display_name("nekros_prime_set"),
    );
    assert_eq!(h.detail, "WTS Nekros Prime Set 40p x3");
    assert_eq!(h.url, "https://warframe.market/items/nekros_prime_set");
    assert_eq!(h.seller.as_deref(), Some("Other"));
}

// --- store ------------------------------------------------------------------------------------

fn save(db: &Db, v: Value) -> MarketAlertRule {
    store::save(&db.lock(), rule(v).unwrap(), MarketAlertBinding::default()).unwrap()
}

#[test]
fn store_saves_loads_and_replaces_by_id() {
    let db = mem_db();
    save(&db, riven(json!({})));
    let mut edited = riven(json!({}));
    edited["name"] = json!("Renamed");
    save(&db, edited);
    let rows = store::load(&db.lock()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0.name, "Renamed");
    let fresh = store::save(
        &db.lock(),
        rule(json!({"name": "n", "kind": "item", "item": item(json!({}))["item"]})).unwrap(),
        MarketAlertBinding { native: false },
    )
    .unwrap();
    assert!(!fresh.id.is_empty());
    assert!(!store::load(&db.lock()).unwrap()[1].1.native);
}

#[test]
fn store_enforces_the_rule_limit() {
    let db = mem_db();
    let conn = db.lock();
    for _ in 0..MAX_RULES {
        let r =
            rule(json!({"name": "n", "kind": "item", "item": item(json!({}))["item"]})).unwrap();
        store::save(&conn, r, MarketAlertBinding::default()).unwrap();
    }
    let r = rule(json!({"name": "n", "kind": "item", "item": item(json!({}))["item"]})).unwrap();
    assert_eq!(
        store::save(&conn, r, MarketAlertBinding::default()).unwrap_err(),
        "rule limit reached"
    );
}

#[test]
fn delete_takes_dedup_state_and_an_unused_owned_snapshot() {
    let db = mem_db();
    let conn = db.lock();
    let a = store::save(
        &conn,
        rule(item(json!({}))).unwrap(),
        MarketAlertBinding::default(),
    )
    .unwrap();
    let mut second = item(json!({}));
    second["id"] = json!("i2");
    store::save(&conn, rule(second).unwrap(), MarketAlertBinding::default()).unwrap();
    store::set_owned(&conn, "nekros_prime_set", 3).unwrap();
    store::mark_seen(&conn, &a.id, &["k:1".into()], 1).unwrap();
    assert!(store::delete(&conn, "i1").unwrap());
    assert!(store::seen(&conn, "i1").unwrap().is_empty());
    assert_eq!(store::owned(&conn, "nekros_prime_set"), Some(3));
    store::delete(&conn, "i2").unwrap();
    assert_eq!(store::owned(&conn, "nekros_prime_set"), None);
    assert!(!store::delete(&conn, "i2").unwrap());
}

#[test]
fn seen_keys_expire_and_stay_bounded() {
    let db = mem_db();
    let conn = db.lock();
    store::mark_seen(&conn, "r", &["old".into()], 0).unwrap();
    store::mark_seen(&conn, "r", &["new".into()], 8 * 24 * 60 * MIN).unwrap();
    assert_eq!(store::seen(&conn, "r").unwrap().len(), 1);
    let keys: Vec<String> = (0..600).map(|i| format!("k{i}")).collect();
    store::mark_seen(&conn, "r", &keys, 8 * 24 * 60 * MIN).unwrap();
    assert_eq!(store::seen(&conn, "r").unwrap().len(), 500);
}

#[test]
fn hit_history_is_capped_and_newest_first() {
    let db = mem_db();
    let conn = db.lock();
    let r = rule(riven(json!({}))).unwrap();
    for i in 0..(HISTORY_MAX + 5) {
        let mut h = riven_hit(&r, &first(auction(&format!("a{i}"), json!({}))));
        h.id = format!("h{i}");
        store::record_hits(&conn, &[h]).unwrap();
    }
    let hits = store::hits(&conn).unwrap();
    assert_eq!(hits.len(), HISTORY_MAX);
    assert_eq!(hits[0].id, format!("h{}", HISTORY_MAX + 4));
    store::clear_hits(&conn).unwrap();
    assert!(store::hits(&conn).unwrap().is_empty());
}

// --- engine -----------------------------------------------------------------------------------

#[derive(Default)]
struct Sink(Mutex<Vec<NotificationDispatch>>);

impl Notifier for Sink {
    fn notify(&self, d: NotificationDispatch) {
        self.0.lock().unwrap().push(d);
    }
}

struct Rig {
    db: Db,
    sink: Sink,
    client: WfmClient,
    engine: Engine,
    mock: Mock,
    own: Option<String>,
}

fn rig(script: Vec<crate::services::wfm::mock::Reply>) -> Rig {
    let mock = spawn(script);
    let client = WfmClient::new_unthrottled(
        &format!("{}/v1", mock.url),
        &format!("{}/v2", mock.url),
        Box::new(|_| {}),
    )
    .unwrap();
    Rig {
        db: mem_db(),
        sink: Sink::default(),
        client,
        engine: Engine::default(),
        mock,
        own: None,
    }
}

impl Rig {
    async fn tick(&self, now: i64) -> usize {
        let cx = Ctx {
            client: &self.client,
            db: &self.db,
            notifier: &self.sink,
            own_name: self.own.clone(),
            changed: &|| {},
        };
        self.engine.tick(&cx, now).await
    }

    fn fired(&self) -> usize {
        self.sink.0.lock().unwrap().len()
    }

    fn hits(&self) -> Vec<MarketAlertHit> {
        store::hits(&self.db.lock()).unwrap()
    }
}

fn auctions(list: Vec<Value>) -> crate::services::wfm::mock::Reply {
    reply(200, &json!({"payload": {"auctions": list}}).to_string())
}

#[tokio::test]
async fn fires_once_per_listing_not_every_cycle() {
    let r = rig(vec![
        auctions(vec![auction("a1", json!({}))]),
        auctions(vec![auction("a1", json!({})), auction("a2", json!({}))]),
    ]);
    save(&r.db, riven(json!({})));
    assert_eq!(r.tick(0).await, 1);
    assert_eq!(r.fired(), 1);
    let d = r.sink.0.lock().unwrap()[0].clone();
    assert_eq!(d.title, "Riven: Crit Rubico");
    assert!(d.native_enabled);
    assert_eq!(r.hits().len(), 1);
    // Inside the cooldown (and the 3-minute re-check) nothing is even requested.
    assert_eq!(r.tick(MIN).await, 0);
    // After the cooldown only the new listing fires; the old one is deduped.
    assert_eq!(r.tick(61 * MIN).await, 1);
    assert_eq!(r.fired(), 2);
    assert_eq!(r.hits()[0].url, "https://warframe.market/auction/a2");
    assert_eq!(r.mock.requests().len(), 2);
    assert!(r.mock.requests()[0].contains("/v1/auctions/search?type=riven&weapon_url_name=rubico"));
}

#[tokio::test]
async fn a_price_drop_on_the_same_listing_fires_again() {
    let r = rig(vec![
        auctions(vec![auction("a1", json!({}))]),
        auctions(vec![auction("a1", json!({"buyout_price": 80}))]),
    ]);
    let mut v = riven(json!({}));
    v["noCooldown"] = json!(true);
    save(&r.db, v);
    r.tick(0).await;
    r.tick(4 * MIN).await;
    assert_eq!(r.fired(), 2);
    assert_eq!(r.hits()[0].platinum, Some(80.0));
}

#[tokio::test]
async fn no_cooldown_still_dedups() {
    let r = rig(vec![
        auctions(vec![auction("a1", json!({}))]),
        auctions(vec![auction("a1", json!({}))]),
    ]);
    let mut v = riven(json!({}));
    v["noCooldown"] = json!(true);
    save(&r.db, v);
    r.tick(0).await;
    r.tick(4 * MIN).await;
    assert_eq!(r.fired(), 1);
    assert!(r.engine.cooldown_until("r1", 4 * MIN).is_none());
}

#[tokio::test]
async fn cooldown_is_reported_and_clearable() {
    let r = rig(vec![
        auctions(vec![auction("a1", json!({}))]),
        auctions(vec![auction("a2", json!({}))]),
    ]);
    save(&r.db, riven(json!({})));
    r.tick(0).await;
    assert_eq!(r.engine.cooldown_until("r1", 0), Some(60 * MIN));
    r.engine.reset_rule("r1");
    assert_eq!(r.engine.cooldown_until("r1", 0), None);
    r.tick(MIN).await;
    assert_eq!(r.fired(), 2);
}

#[tokio::test]
async fn own_listings_never_alert() {
    let mut r = rig(vec![auctions(vec![auction("a1", json!({"owner": "ME"}))])]);
    r.own = Some("me".into());
    save(&r.db, riven(json!({})));
    r.tick(0).await;
    assert_eq!(r.fired(), 0);
}

#[tokio::test]
async fn binding_can_mute_the_toast() {
    let r = rig(vec![auctions(vec![auction("a1", json!({}))])]);
    store::save(
        &r.db.lock(),
        rule(riven(json!({}))).unwrap(),
        MarketAlertBinding { native: false },
    )
    .unwrap();
    r.tick(0).await;
    assert!(!r.sink.0.lock().unwrap()[0].native_enabled);
}

#[tokio::test]
async fn disabled_and_baro_rules_are_skipped() {
    let r = rig(vec![]);
    let mut off = riven(json!({}));
    off["enabled"] = json!(false);
    save(&r.db, off);
    save(
        &r.db,
        json!({"id": "b", "name": "b", "kind": "baro", "baro": {"itemUrlName": "x"}}),
    );
    assert_eq!(r.tick(0).await, 0);
    assert!(r.mock.requests().is_empty());
}

#[tokio::test]
async fn item_rules_read_the_v2_order_book() {
    let body = json!({"data": [
        {"id": "o1", "type": "sell", "platinum": 40, "quantity": 1, "visible": true, "user": {"ingameName": "A", "status": "ingame"}},
        {"id": "o2", "type": "sell", "platinum": 45, "quantity": 1, "user": {"ingameName": "B", "status": "ingame", "platform": "xbox"}},
        {"id": "o3", "type": "sell", "platinum": 45, "quantity": 1, "visible": false, "user": {"ingameName": "C", "status": "ingame"}},
        {"id": "o4", "type": "sell", "platinum": 99, "quantity": 1, "user": {"ingameName": "D", "status": "ingame"}},
        {"id": "o5", "type": "buy", "platinum": 10, "quantity": 1, "user": {"ingameName": "E", "status": "ingame"}},
        {"id": "o6", "type": "sell", "platinum": 41, "quantity": 1, "user": {"ingameName": "Me", "status": "ingame"}},
    ]});
    let mut r = rig(vec![reply(200, &body.to_string())]);
    r.own = Some("me".into());
    save(&r.db, item(json!({})));
    r.tick(0).await;
    let hits = r.hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].detail, "WTS Nekros Prime Set 40p x1");
    assert!(r.mock.requests()[0].contains("/v2/orders/item/nekros_prime_set"));
}

#[tokio::test]
async fn owned_snapshot_gates_item_rules() {
    let body = json!({"data": [{"id": "o1", "type": "sell", "platinum": 40, "quantity": 1, "user": {"ingameName": "A", "status": "ingame"}}]});
    let r = rig(vec![reply(200, &body.to_string())]);
    save(&r.db, item(json!({"ownedBelow": 2})));
    store::set_owned(&r.db.lock(), "nekros_prime_set", 5).unwrap();
    r.tick(0).await;
    assert_eq!(r.fired(), 0);
}

#[tokio::test]
async fn at_most_four_rules_per_tick_longest_waiting_first() {
    let r = rig((0..8).map(|_| auctions(vec![])).collect());
    for i in 0..6 {
        let mut v = riven(json!({}));
        v["id"] = json!(format!("r{i}"));
        save(&r.db, v);
    }
    assert_eq!(r.tick(0).await, 4);
    // The two that waited get the next slots before any of the first four is due again.
    assert_eq!(r.tick(MIN).await, 2);
    assert_eq!(r.mock.requests().len(), 6);
}

#[tokio::test]
async fn failures_back_off_and_recover() {
    let r = rig(vec![
        reply(500, "boom"),
        reply(500, "boom"),
        auctions(vec![auction("a1", json!({}))]),
    ]);
    save(&r.db, riven(json!({})));
    r.tick(0).await;
    let st = r.engine.status(&r.db);
    assert!(st.last_error.is_some());
    // First failure waits 5 minutes, the second 10.
    assert_eq!(r.tick(4 * MIN).await, 0);
    assert_eq!(r.tick(5 * MIN).await, 1);
    assert_eq!(r.tick(14 * MIN).await, 0);
    assert_eq!(r.tick(15 * MIN).await, 1);
    assert_eq!(r.fired(), 1);
    assert!(r.engine.status(&r.db).last_error.is_none());
}

#[tokio::test]
async fn an_error_clears_with_its_rule() {
    let r = rig(vec![reply(500, "boom")]);
    save(&r.db, riven(json!({})));
    r.tick(0).await;
    assert!(r.engine.status(&r.db).last_error.is_some());
    r.engine.forget_rule("other");
    assert!(r.engine.status(&r.db).last_error.is_some());
    r.engine.forget_rule("r1");
    assert!(r.engine.status(&r.db).last_error.is_none());
}

#[tokio::test]
async fn test_fire_ignores_dedup_and_leaves_state_alone() {
    let r = rig(vec![
        auctions(vec![auction("a1", json!({}))]),
        auctions(vec![auction("a1", json!({}))]),
    ]);
    save(&r.db, riven(json!({})));
    let cx = Ctx {
        client: &r.client,
        db: &r.db,
        notifier: &r.sink,
        own_name: None,
        changed: &|| {},
    };
    let out = r.engine.test_fire(&cx, "r1").await.unwrap();
    assert_eq!(out.matches, 1);
    assert_eq!(r.sink.0.lock().unwrap()[0].title, "Test: Crit Rubico");
    assert!(r.hits().is_empty());
    assert!(r.engine.cooldown_until("r1", 0).is_none());
    // The real tick still fires: the test did not mark the listing seen.
    r.tick(0).await;
    assert_eq!(r.hits().len(), 1);
    assert_eq!(
        r.engine.test_fire(&cx, "nope").await.unwrap_err(),
        "unknown rule"
    );
}

#[tokio::test]
async fn test_fire_refuses_baro() {
    let r = rig(vec![]);
    save(
        &r.db,
        json!({"id": "b", "name": "b", "kind": "baro", "baro": {"itemUrlName": "x"}}),
    );
    let cx = Ctx {
        client: &r.client,
        db: &r.db,
        notifier: &r.sink,
        own_name: None,
        changed: &|| {},
    };
    assert!(
        r.engine
            .test_fire(&cx, "b")
            .await
            .unwrap_err()
            .contains("baro")
    );
}

#[tokio::test]
async fn status_counts_rules() {
    let r = rig(vec![]);
    save(&r.db, riven(json!({})));
    let mut off = item(json!({}));
    off["enabled"] = json!(false);
    save(&r.db, off);
    let st = r.engine.status(&r.db);
    assert_eq!((st.rule_count, st.enabled_count, st.running), (2, 1, false));
    r.engine.arm();
    assert!(r.engine.status(&r.db).running);
}

// --- live feed --------------------------------------------------------------------------------

fn catalog_item(id: &str, slug: &str) -> crate::services::wfm::types::CatalogItem {
    crate::services::wfm::types::CatalogItem {
        id: id.into(),
        slug: slug.into(),
        name: slug.into(),
        thumb: None,
        icon: None,
        max_rank: None,
        game_ref: None,
    }
}

fn watch(r: &Rig) {
    let slugs = ["nekros_prime_set".to_owned()].into_iter().collect();
    r.engine
        .set_watch(&slugs, &[catalog_item("id1", "nekros_prime_set")]);
}

impl Rig {
    fn live_order(&self, o: crate::services::wfm::orders::BookOrder, now: i64) {
        let cx = Ctx {
            client: &self.client,
            db: &self.db,
            notifier: &self.sink,
            own_name: self.own.clone(),
            changed: &|| {},
        };
        let slug = self.engine.watched_slug("id1").unwrap();
        self.engine.on_new_order(&cx, &slug, o, now);
    }
}

#[tokio::test]
async fn a_live_order_hits_and_the_next_poll_does_not_repeat_it() {
    let body = json!({"data": [{"id": "o1", "type": "sell", "platinum": 40, "quantity": 1,
        "user": {"ingameName": "Other", "status": "ingame"}}]});
    let r = rig(vec![reply(200, &body.to_string())]);
    save(&r.db, item(json!({})));
    watch(&r);
    r.live_order(order(json!({})), 0);
    assert_eq!(r.fired(), 1);
    assert_eq!(r.hits()[0].detail, "WTS Nekros Prime Set 40p x1");
    // The cooldown is over, but the order is already seen.
    r.engine.reset_rule("i1");
    r.tick(0).await;
    assert_eq!(r.fired(), 1);
    assert_eq!(r.hits().len(), 1);
}

#[test]
fn live_orders_use_the_poll_filters() {
    let r = rig(vec![]);
    let mut own = rig(vec![]);
    own.own = Some("other".into());
    for rg in [&r, &own] {
        save(&rg.db, item(json!({})));
        watch(rg);
    }
    r.live_order(order(json!({"platinum": 99})), 0);
    r.live_order(order(json!({"visible": false})), 0);
    r.live_order(
        order(json!({"user": {"ingameName": "X", "status": "ingame", "platform": "xbox"}})),
        0,
    );
    own.live_order(order(json!({})), 0);
    assert_eq!(r.fired() + own.fired(), 0);
}

#[test]
fn events_for_unwatched_items_do_nothing() {
    let r = rig(vec![]);
    save(&r.db, item(json!({})));
    watch(&r);
    assert!(r.engine.watched_slug("other_id").is_none());
    assert_eq!(r.fired(), 0);
}

#[test]
fn live_needs_the_feed_and_a_catalog_that_covers_every_rule() {
    let r = rig(vec![]);
    let slugs: std::collections::HashSet<String> =
        ["nekros_prime_set".to_owned(), "unknown_slug".to_owned()]
            .into_iter()
            .collect();
    r.engine.set_subscribed(true);
    r.engine
        .set_watch(&slugs, &[catalog_item("id1", "nekros_prime_set")]);
    assert!(!r.engine.is_live() && !r.engine.status(&r.db).live);
    watch(&r);
    assert!(r.engine.status(&r.db).live);
    r.engine.set_subscribed(false);
    assert!(!r.engine.is_live());
}

#[test]
fn item_rules_recheck_every_five_minutes_only_while_live() {
    let r = rig(vec![]);
    let item_ms = |r: &Rig| r.engine.eval_interval_ms(MarketAlertKind::Item);
    let before = item_ms(&r);
    assert_eq!(before, 3 * MIN);
    watch(&r);
    r.engine.set_subscribed(true);
    assert_eq!(item_ms(&r), 5 * MIN);
    assert_eq!(r.engine.eval_interval_ms(MarketAlertKind::Riven), before);
}
