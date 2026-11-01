use super::*;
use crate::db::mem_db;
use serde_json::json;

const ID: &str = "0123456789abcdef01234567";

#[test]
fn account_id_is_found_in_a_bare_id_or_a_link() {
    let url = format!(
        "https://www.warframe.com/profile?playerId={}",
        ID.to_uppercase()
    );
    assert_eq!(
        account_id_from_input(&format!(" {ID} ")).as_deref(),
        Some(ID)
    );
    assert_eq!(account_id_from_input(&url).as_deref(), Some(ID));
    assert_eq!(account_id_from_input("not-an-id"), None);
    assert_eq!(account_id_from_input(&ID[..23]), None);
    assert_eq!(
        account_id_from_authz(&format!("?accountId={ID}&nonce=42")),
        Some(ID)
    );
    assert_eq!(account_id_from_authz("?accountId=XYZ&nonce=1"), None);
}

#[test]
fn scans_parse_from_either_location_and_keep_the_max() {
    let a = json!({"Stats": {"Scans": [
        {"type": "/a", "scans": 3}, {"type": "/a", "scans": 9}, {"type": "", "scans": 1},
        {"type": "/b", "scans": -1}, {"type": "/c"}, 5
    ]}});
    let want = vec![CodexScanEntry {
        kind: "/a".into(),
        count: 9,
    }];
    assert_eq!(parse_profile_scans(&a), Some(want.clone()));
    let b = json!({"Results": [{"Stats": {"Scans": [{"type": "/a", "scans": 9}]}}]});
    assert_eq!(parse_profile_scans(&b), Some(want));
    assert_eq!(parse_profile_scans(&json!({"Stats": {}})), None);
}

#[test]
fn account_roundtrip_and_cache_replace() {
    let dir = std::env::temp_dir().join(format!("cf-profile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(load_account(&dir), None);
    save_account(&dir, ID).unwrap();
    assert_eq!(load_account(&dir).as_deref(), Some(ID));
    std::fs::remove_dir_all(&dir).ok();

    let db = mem_db();
    let s = |t: &str, c| CodexScanEntry {
        kind: t.into(),
        count: c,
    };
    write_cache(&db, ID, 10, &[s("/a", 1), s("/b", 2)]).unwrap();
    write_cache(&db, ID, 20, &[s("/a", 5)]).unwrap();
    assert_eq!(read_cache(&db, ID), Some((20, vec![s("/a", 5)])));
}

#[test]
fn profile_inventory_keeps_gear_xp_and_missions_only() {
    let payload = json!({
        "Results": [{
            "PlayerLevel": 21,
            "Missions": [{ "Tag": "SolNode1", "Completes": 3 }],
            "LoadOutInventory": {
                "Suits": [{ "ItemType": "/Lotus/Powersuits/Excalibur/Excalibur", "XP": 900_000 }],
                "XPInfo": [{ "ItemType": "/Lotus/Weapons/Tenno/Rifle/Acceltra", "XP": 450_000 }]
            },
            "Stats": { "Scans": [] }
        }]
    });
    let inv = parse_profile_inventory(&payload).unwrap();
    assert_eq!(inv["PlayerLevel"], 21);
    assert_eq!(inv["XPInfo"][0]["XP"], 450_000);
    assert_eq!(inv["Suits"].as_array().unwrap().len(), 1);
    assert_eq!(inv["Missions"][0]["Tag"], "SolNode1");
    assert!(inv.get("Stats").is_none() && inv.get("MiscItems").is_none());
    // A private or empty profile has no XP: nothing to compute mastery from.
    assert_eq!(
        parse_profile_inventory(&json!({ "Results": [{ "PlayerLevel": 3 }] })),
        None
    );
}
