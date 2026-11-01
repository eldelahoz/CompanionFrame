use super::*;
use crate::db::mem_db;
use serde_json::json;

fn id(c: char) -> String {
    c.to_string().repeat(24)
}

fn slot(item: &str, m: u32, cus: u32) -> Value {
    json!({"ItemId": {"$oid": item}, "mod": m, "cus": cus})
}

fn inventory() -> Value {
    let (frame, primary, modid, absent) = (id('a'), id('b'), id('c'), id('d'));
    json!({
        "LoadOutPresets": {
            "NORMAL": [
                {"n": " Fixture loadout ", "s": slot(&frame, 1, 0), "l": slot(&primary, 1, 0),
                 "p": slot(&absent, 0, 0), "m": {}},
                {"n": "Other", "s": {"ItemId": {"$oid": &frame}, "mod": 0, "cus": 1, "hide": true}},
                "junk"
            ],
            "ARCHWING": [{"n": "ignored", "s": slot(&frame, 0, 0)}]
        },
        "Suits": [{
            "ItemId": {"$oid": &frame}, "ItemType": "/Lotus/Powersuits/Fixture",
            "Configs": [
                {"Name": "Look", "Skins": ["/Lotus/Skins/A", "/Lotus/Skins/EmptyCustomization", id('e')],
                 "pricol": {"t0": 0x11223344u32, "bogus": 1}},
                {"Name": "Build", "Upgrades": [&modid, "", "/Lotus/Upgrades/Mods/X", &absent]}
            ]
        }],
        "LongGuns": [{
            "ItemId": {"$oid": &primary}, "ItemType": "/Lotus/Weapons/Fixture",
            "Configs": [{"Skins": []}, {"Upgrades": [&modid]}]
        }],
        "Upgrades": [{
            "ItemId": {"$oid": &modid}, "ItemType": "/Lotus/Upgrades/Mods/Fixture",
            "UpgradeFingerprint": "{\"lvl\":7}"
        }],
        "WeaponSkins": [{"ItemId": {"$oid": id('e')}, "ItemType": "/Lotus/Skins/Owned"}]
    })
}

#[test]
fn loadouts_resolve_slots_skins_and_mods() {
    let l = parse_personal_loadouts(&inventory());
    assert_eq!(l.len(), 2, "only NORMAL presets that are objects");
    assert_eq!(l[0].name.as_deref(), Some("Fixture loadout"));
    let cats: Vec<_> = l[0].slots.iter().map(|s| s.category.as_str()).collect();
    assert_eq!(
        cats,
        ["Suits", "LongGuns", "Pistols"],
        "empty melee slot is skipped"
    );
    assert!(
        l[0].slots[2].item.is_none(),
        "missing equipment keeps an empty slot"
    );

    let suit = l[0].slots[0].item.as_ref().unwrap();
    assert_eq!(suit.kind, "/Lotus/Powersuits/Fixture");
    assert_eq!(
        (suit.active_config, suit.active_mod_config),
        (Some(0), Some(1))
    );
    let skins: Vec<_> = suit.configs[0]
        .skins
        .iter()
        .map(|s| (s.slot, s.kind.as_str()))
        .collect();
    assert_eq!(skins, [(0, "/Lotus/Skins/A"), (2, "/Lotus/Skins/Owned")]);
    assert_eq!(
        suit.configs[0].colors["pricol"]["t0"],
        "#223344".to_owned() + "11"
    );
    assert!(!suit.configs[0].colors["pricol"].contains_key("bogus"));

    let ups = suit.mod_configs[1].upgrades.as_ref().unwrap();
    let got: Vec<_> = ups
        .iter()
        .map(|u| (u.slot, u.kind.as_deref(), u.rank))
        .collect();
    assert_eq!(
        got,
        [
            (0, Some("/Lotus/Upgrades/Mods/Fixture"), Some(7)),
            (2, Some("/Lotus/Upgrades/Mods/X"), None),
            (3, None, None),
        ]
    );
    assert_eq!(suit.mod_configs[0].upgrades.as_ref().map(Vec::len), None);

    let second = l[1].slots[0].item.as_ref().unwrap();
    assert_eq!(second.hidden_when_holstered, Some(true));
    assert_eq!(second.active_config, Some(1));
    assert!(parse_personal_loadouts(&json!({"Suits": []})).is_empty());
}

#[test]
fn profile_merges_duplicates_scans_and_appearance() {
    let (frame, other) = (id('a'), id('f'));
    let payload = json!({
        "Results": [{
            "displayName": "Tenno", "PlayerLevel": 31, "Created": {"$date": {"$numberLong": "1500000000000"}},
            "LoadOutInventory": {"Suits": [
                {"ItemId": {"$oid": &frame}, "ItemType": "/Lotus/Powersuits/A", "Configs": [{"Name": "x"}]},
                {"ItemId": {"$oid": &other}, "ItemType": "/Lotus/Powersuits/B", "Configs": [{}]}
            ]},
            "LoadOutPreset": {"s": {"ItemId": {"$oid": &frame}, "cus": 0, "hide": false}},
            "Stats": {"Missions": [{"type": "/m", "highScore": 5}]}
        }],
        "Stats": {
            "TimePlayedSec": 3600, "Deaths": -1,
            "Weapons": [{"type": "/w", "kills": 5, "xp": 10}, {"type": "/w", "kills": 3, "headshots": 2}],
            "Enemies": [{"type": "/e", "kills": 4}, {"type": "/f", "kills": 1, "scans": 9}],
            "Scans": [{"type": "/e", "scans": 12}, {"type": "/f", "scans": 99}],
            "Abilities": [{"type": "/a", "used": 2, "name": "Slash"}]
        }
    });
    let p = parse_personal_profile(&payload).unwrap();
    assert_eq!(p.display_name.as_deref(), Some("Tenno"));
    assert_eq!((p.mastery_rank, p.registered_at), (Some(31), Some(1.5e12)));
    assert_eq!(p.career.get("TimePlayedSec"), Some(&3600.0));
    assert!(
        !p.career.contains_key("Deaths"),
        "negative counters are dropped"
    );
    let w = &p.equipment.unwrap()[0];
    assert_eq!(
        (w.stats["kills"], w.stats["xp"], w.stats["headshots"]),
        (5.0, 10.0, 2.0)
    );
    let e = p.enemies.unwrap();
    assert_eq!((e[0].stats["scans"], e[1].stats["scans"]), (12.0, 9.0));
    assert_eq!(p.abilities.unwrap()[0].name.as_deref(), Some("Slash"));
    assert_eq!(p.missions.unwrap()[0].stats["highScore"], 5.0);
    assert_eq!(p.appearance.len(), 2);
    assert_eq!(
        (
            p.appearance[0].active_config,
            p.appearance[0].hidden_when_holstered
        ),
        (Some(0), Some(false))
    );
    assert_eq!(p.appearance[1].active_config, None);
    assert!(parse_personal_profile(&json!({"Results": [{}]})).is_none());
    assert!(parse_personal_profile(&json!([])).is_none());
}

#[test]
fn profile_cache_roundtrips() {
    let db = mem_db();
    let acct = id('1');
    assert!(read_cache(&db, &acct).is_none());
    let p = PersonalProfile {
        display_name: Some("T".into()),
        ..Default::default()
    };
    write_cache(&db, &acct, 5, &p).unwrap();
    assert_eq!(read_cache(&db, &acct), Some((5, p)));
}
