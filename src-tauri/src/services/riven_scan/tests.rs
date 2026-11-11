//! Ported from WFHelper's rivenOverlay (parseRivenStats, findWeaponInText), rivenWeaponLabel,
//! rivenFingerprint and rivenSession tests. `fixtures/riven_scan/text_golden.jsonl` holds the
//! reference parser's output for every input those tests (and the saved roll-scan replay) feed it,
//! so the port is checked against WFHelper itself, not against re-typed expectations.

use std::sync::OnceLock;

use serde_json::{Value, json};

use super::fingerprint::decode_all_rivens;
use super::session::{RollTally, ScanGeneration};
use super::text::*;
use super::weapon_label::*;
use crate::services::bundled_data::BundledData;
use crate::services::rivens::best_attributes::GoodRolls;
use crate::services::rivens::data::RivenData;

fn parse(text: &str) -> Vec<RivenStat> {
    parse_riven_stats(text, None)
}

fn stat(name: &str, positive: bool, value: Option<f64>) -> RivenStat {
    RivenStat {
        name: name.into(),
        positive,
        display_positive: None,
        value,
        multiplier: false,
    }
}

/// JSON equality where 112 and 112.0 are the same number (JS prints no decimal point).
fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w)))
        }
        _ => a == b,
    }
}

#[test]
fn matches_the_reference_parser_on_every_saved_input() {
    let golden = include_str!("../../../tests/fixtures/riven_scan/text_golden.jsonl");
    let cards = |v: &Value| -> Vec<RivenStat> { serde_json::from_value(v.clone()).unwrap() };
    let mut checked = 0;
    let mut failures = Vec::new();
    for line in golden.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let (input, expected) = (&case["input"], &case["output"]);
        let got = match case["fn"].as_str().unwrap() {
            "parse" => {
                let mut diag = ParseDiagnostics::default();
                let stats = parse_riven_stats(input.as_str().unwrap(), Some(&mut diag));
                // The reference leaves the title flag unset only for empty input.
                let title = diag.title_seen.map_or(Value::Null, Value::Bool);
                json!({ "stats": stats, "dropped": diag.dropped_lines, "title": title })
            }
            "whole" => json!(looks_like_whole_stat_line(input.as_str().unwrap())),
            kind => {
                let scanned = cards(&input["s"]);
                let known: Vec<Vec<RivenStat>> =
                    input["k"].as_array().unwrap().iter().map(cards).collect();
                if kind == "stale" {
                    json!(looks_like_stale_card_read(&scanned, &known))
                } else {
                    json!(roll_rescan_reason(&scanned, &known))
                }
            }
        };
        checked += 1;
        if !same_json(&got, expected) {
            failures.push(format!("{input}\n  want {expected}\n  got  {got}"));
        }
    }
    assert!(checked > 150, "golden file shrank: {checked}");
    assert!(
        failures.is_empty(),
        "{} of {checked} differ from the reference:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn reads_signs_values_and_locale_commas() {
    assert!(parse("").is_empty());
    assert_eq!(
        parse("+94,5% Critical Chance"),
        [stat("Critical Chance", true, Some(94.5))]
    );
    assert_eq!(parse("-27.3% Zoom"), [stat("Zoom", false, Some(27.3))]);
    // Recoil shows buffs with a minus: positive in meaning, negative as printed.
    let recoil = &parse("\u{2013}94.5% Recoil")[0];
    assert!(recoil.positive);
    assert_eq!(recoil.display_positive, Some(false));
    // A recoil curse (more recoil) prints a plus: a curse in meaning, positive as printed.
    let curse = &parse("+51.2% Recoil")[0];
    assert!(!curse.positive);
    assert_eq!(curse.display_positive, Some(true));
    assert_eq!(
        parse("Critical Chance"),
        [stat("Critical Chance", true, None)]
    );
}

#[test]
fn repairs_dropped_decimals_and_spaced_digits() {
    assert_eq!(parse("+1552% Critical Damage")[0].value, Some(155.2));
    assert_eq!(parse("-739% Slash")[0].value, Some(73.9));
    assert_eq!(parse("+219.5% Damage")[0].value, Some(219.5));
    assert_eq!(parse("-73 9% Slash")[0].value, Some(73.9));
    assert_eq!(parse("+2.5 Range\n+151 .4% Impact")[1].value, Some(151.4));
}

#[test]
fn reads_faction_multipliers_and_their_misreads() {
    let got =
        parse("+115,9% Damage\n+74,1% 3:Toxin\n<1,32 Damage to Infeste\nx0,75 Damage to Corpu");
    let names: Vec<&str> = got.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["Damage", "Toxin", "Damage to Infested", "Damage to Corpus"]
    );
    assert!(got[2].multiplier && got[2].positive && got[2].value == Some(1.32));
    assert!(got[3].multiplier && !got[3].positive && got[3].value == Some(0.75));
    assert_eq!(parse("x I , 44 Damage to Grineer")[0].value, Some(1.44));
}

#[test]
fn joins_wrapped_names_and_orphan_values() {
    assert_eq!(parse("+144,9% Finisher\nDamage")[0].name, "Finisher Damage");
    let slide = parse("+128,1% Critical Chance\nfor Slide Attack\n+157% Melee Damage");
    assert_eq!(slide[0].name, "Critical Chance for Slide Attack");
    let orphans = parse("+95,5%\n+122,4%\nCold\nImpact\nx1,46 Damage to Corpus");
    assert_eq!(
        (orphans[0].name.as_str(), orphans[0].value),
        ("Cold", Some(95.5))
    );
    assert_eq!(
        (orphans[1].name.as_str(), orphans[1].value),
        ("Impact", Some(122.4))
    );
}

#[test]
fn qualifiers_are_not_stats() {
    let got = parse("+185,5% Critical Chance\n(x2 for Heavy Attacks)\n+8,5s Combo Duration");
    let names: Vec<&str> = got.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Critical Chance", "Combo Duration"]);
    assert_eq!(got[1].value, Some(8.5));
}

#[test]
fn reports_signed_lines_that_parse_to_nothing() {
    let mut diag = ParseDiagnostics::default();
    let got = parse_riven_stats(
        "+48.3% Critical Chance\n+12.3% Xyzqgarbleword\nMR1624",
        Some(&mut diag),
    );
    assert_eq!(got.len(), 1);
    assert_eq!(diag.dropped_lines, ["+12.3% Xyzqgarbleword"]);
}

#[test]
fn damage_types_share_a_combined_value_only_when_adjacent() {
    let combined = parse("+112% Electricity Impact");
    assert_eq!(
        (combined[0].value, combined[1].value),
        (Some(112.0), Some(112.0))
    );
    let separate = parse("+126.2% Status Duration + Electricity");
    assert_eq!(separate[1].value, None);
}

#[test]
fn a_stale_card_is_two_exact_values_of_a_known_card() {
    let known = vec![vec![
        stat("Damage", true, Some(103.3)),
        stat("Toxin", true, Some(42.7)),
    ]];
    let same = [
        stat("Damage", true, Some(103.3)),
        stat("Toxin", true, Some(42.7)),
    ];
    let reroll = [
        stat("Damage", true, Some(99.0)),
        stat("Toxin", true, Some(42.7)),
    ];
    assert!(looks_like_stale_card_read(&same, &known));
    assert!(!looks_like_stale_card_read(&reroll, &known));
    assert_eq!(roll_rescan_reason(&[], &known), Some("read nothing"));
    assert_eq!(
        roll_rescan_reason(&same, &known),
        Some("matches a pre-roll card")
    );
    assert_eq!(roll_rescan_reason(&reroll, &known), None);
}

fn weapons() -> WeaponIndex {
    WeaponIndex::new([
        "Braton",
        "MK1-Braton",
        "Bolto",
        "Boltor",
        "Boar",
        "Boar Prime",
        "Sobek",
        "Kuva Sobek",
        "Vulklok",
        "Akzani",
        "Lato",
        "Rubico Prime",
        "Gotva Prime",
        "Wolf Sledge",
        "Cobra & Crane",
        "Kuva Bramma",
        "Verglas",
        "Hema",
        "Hera",
    ])
}

fn label(name: &str, exact: bool) -> Option<WeaponLabelMatch> {
    Some(WeaponLabelMatch {
        name: name.into(),
        exact,
    })
}

#[test]
fn label_line_matches_the_linked_variant_among_captions() {
    let w = weapons();
    assert_eq!(
        w.find_by_label_line(&["FITS IN", "Kuva Sobek", "SHOW RANKED", "CANCEL"]),
        label("Kuva Sobek", true)
    );
    assert_eq!(
        w.find_by_label_line(&["MK1-Braton"]),
        label("MK1-Braton", true)
    );
    assert_eq!(
        w.find_by_label_line(&["Boar", "Boar Prime"]),
        label("Boar Prime", true)
    );
}

#[test]
fn label_line_tolerates_one_misread_letter_from_five_characters() {
    let w = weapons();
    assert_eq!(
        w.find_by_label_line(&["Kuva Sobck"]),
        label("Kuva Sobek", false)
    );
    assert_eq!(
        w.find_by_label_line(&["FTSIN", "vuiklok"]),
        label("Vulklok", false)
    );
    assert_eq!(
        w.find_by_label_line(&["FITS IN", "Akzanl"]),
        label("Akzani", false)
    );
    // Four characters is too short to guess from; "boltr" fits Bolto and Boltor equally.
    assert_eq!(w.find_by_label_line(&["Lat0"]), None);
    assert_eq!(w.find_by_label_line(&["boltr"]), None);
}

#[test]
fn label_line_reads_nothing_from_headings_and_stat_rows() {
    let w = weapons();
    assert_eq!(w.find_by_label_line(&["FITS IN", "FTSIN", "WFTSIN"]), None);
    assert_eq!(
        w.find_by_label_line(&["Multishot7", "Magazine27", "Reloado 1s", "toxicron"]),
        None
    );
    assert_eq!(
        w.find_by_label_line(&["FITS IN", "SHOW RANKED", "CANCEL", "Remaining Kuva 89528"]),
        None
    );
    assert_eq!(w.find_by_label_line(&["FITS IN", "EXIT"]), None);
    assert_eq!(
        w.find_by_label_line(&["+72,7% Fire Rate (x2 for Bows)"]),
        None
    );
    assert_eq!(w.find_by_label_line::<&str>(&[]), None);
}

#[test]
fn label_replaces_the_weapon_by_source_and_family() {
    let exact = label("Kuva Sobek", true).unwrap();
    let fuzzy = label("Kuva Sobek", false).unwrap();
    assert!(should_apply_label_weapon(
        &exact,
        "",
        WeaponSource::None,
        false
    ));
    assert!(should_apply_label_weapon(
        &fuzzy,
        "Riven",
        WeaponSource::None,
        false
    ));
    assert!(!should_apply_label_weapon(
        &exact,
        "Kuva Sobek",
        WeaponSource::Diorama,
        true
    ));
    // Within the family the label is the live linked variant.
    let boar = label("Boar", false).unwrap();
    assert!(should_apply_label_weapon(
        &boar,
        "Boar Prime",
        WeaponSource::Label,
        true
    ));
    for source in [
        WeaponSource::Ocr,
        WeaponSource::Diorama,
        WeaponSource::Dialog,
        WeaponSource::None,
    ] {
        assert!(should_apply_label_weapon(&exact, "Hema", source, false));
    }
    // A fuzzy read across families only displaces a card-title OCR guess.
    assert!(should_apply_label_weapon(
        &fuzzy,
        "Hema",
        WeaponSource::Ocr,
        false
    ));
    for source in [
        WeaponSource::Diorama,
        WeaponSource::Dialog,
        WeaponSource::Label,
    ] {
        assert!(!should_apply_label_weapon(&fuzzy, "Hema", source, false));
    }
}

#[test]
fn weapon_in_text_anchors_on_the_title_line() {
    let w = weapons();
    let find = |t: &str| w.find_in_text(t);
    assert_eq!(
        find("Rubico Prime Crita-acrit\n+185.1% Critical Chance").as_deref(),
        Some("Rubico Prime")
    );
    assert_eq!(
        find("Rubico Prine Crita-acrit\n+185.1% Critical Chance").as_deref(),
        Some("Rubico Prime")
    );
    assert_eq!(
        find("Gotva Visi-critata\n+198.2% Multishot").as_deref(),
        Some("Gotva Prime")
    );
    // The suffix is built from stat syllables ("Hera-decipha") and must never name a weapon.
    assert_eq!(find("Grelvax Hera-decipha\n+50.3% Status Duration"), None);
    assert_eq!(find("Grelvax Boari-critacan\n+103.9% Damage"), None);
    assert_eq!(
        find("Wolf Sledge Acri-\ncronitor\n+0.2 Range").as_deref(),
        Some("Wolf Sledge")
    );
    assert_eq!(
        find("Cobra & Crane Deciata").as_deref(),
        Some("Cobra & Crane")
    );
    assert_eq!(
        find("MK1-Braton Croni-visican").as_deref(),
        Some("MK1-Braton")
    );
    assert_eq!(
        find("Kuva Bramma Toxi-critacan").as_deref(),
        Some("Kuva Bramma")
    );
    assert_eq!(find("Verglas Visi-acricron").as_deref(), Some("Verglas"));
    assert_eq!(
        find("Boar Critadra\n+150.6% Fire Rate (x)\nBows).").as_deref(),
        Some("Boar")
    );
}

#[test]
fn scan_generation_drops_stale_completions() {
    let generation = ScanGeneration::default();
    let first = generation.begin();
    assert!(generation.is_current(first));
    generation.invalidate();
    assert!(!generation.is_current(first));
    let second = generation.begin();
    assert!(generation.is_current(second) && !generation.is_current(first));
    assert_eq!(generation.current(), second);
}

#[test]
fn a_failed_scan_still_counts_the_roll_and_its_kuva() {
    let mut tally = RollTally::new(900);
    assert_eq!(tally.roll_done().total_kuva_spent, 900);
    let second = tally.roll_done();
    assert_eq!((second.roll_count, second.total_kuva_spent), (2, 1800));
}

fn game() -> Option<&'static (RivenData, GoodRolls)> {
    static DATA: OnceLock<Option<(RivenData, GoodRolls)>> = OnceLock::new();
    DATA.get_or_init(|| {
        crate::services::bundled_data::test_root().map(|root| {
            (
                RivenData::load(&BundledData::new(root)),
                GoodRolls::default(),
            )
        })
    })
    .as_ref()
}

const MAX_ROLL_INT: f64 = 1_073_741_823.0;

fn rifle_inventory(lvl: Option<u32>) -> Value {
    let mut fp = json!({
        "compat": "/Lotus/Weapons/Tenno/Rifle/Rifle", "lim": 0, "lvlReq": 9, "rerolls": 2,
        "pol": "AP_ATTACK",
        "buffs": [{"Tag": "WeaponFireDamageMod", "Value": (MAX_ROLL_INT * 0.72).round()}],
        "curses": [{"Tag": "WeaponFireRateMod", "Value": (MAX_ROLL_INT * 0.3).round()}],
    });
    if let Some(l) = lvl {
        fp["lvl"] = json!(l);
    }
    json!({ "Upgrades": [{
        "ItemType": "/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare",
        "ItemId": {"$oid": "aaaaaaaaaaaaaaaaaaaaaaa1"},
        "UpgradeFingerprint": fp.to_string(),
    }]})
}

#[test]
fn fingerprint_projects_rank_eight_without_scaling_rounded_values() {
    let Some((data, rolls)) = game() else { return };
    let decode = |lvl| {
        decode_all_rivens(&rifle_inventory(lvl), data, rolls)
            .unveiled
            .remove(0)
    };
    let (omitted, unranked, maxed) = (decode(None), decode(Some(0)), decode(Some(8)));

    assert_eq!(omitted.current_rank, 0.0);
    assert_eq!(omitted.stats, unranked.stats);
    assert_eq!(omitted.mastery_req, 9.0);
    assert_eq!(maxed.current_rank, 8.0);
    assert_eq!(omitted.stat_perfectness, maxed.stat_perfectness);
    assert_eq!(omitted.overall_grade, maxed.overall_grade);
    for (a, b) in omitted.stats.iter().zip(&maxed.stats) {
        assert_eq!(a.max_rank_value, b.display_value);
        assert_eq!(b.max_rank_value, b.display_value);
    }
    for rank in 0..=8u32 {
        let at_rank = decode(Some(rank));
        for (src, at) in maxed.stats.iter().zip(&at_rank.stats) {
            assert_eq!(src.rank_values[rank as usize], at.display_value);
        }
    }
    let curse = unranked.stats.iter().find(|s| !s.positive).unwrap();
    assert!(curse.display_value < 0.0 && curse.max_rank_value < 0.0);
    let buff = unranked.stats.iter().find(|s| s.positive).unwrap();
    assert!(buff.display_value < buff.max_rank_value);
}

#[test]
fn a_reroll_changes_the_decoded_riven_and_a_reread_does_not() {
    let Some((data, rolls)) = game() else { return };
    let a = decode_all_rivens(&rifle_inventory(Some(8)), data, rolls).unveiled;
    let b = decode_all_rivens(&rifle_inventory(Some(8)), data, rolls).unveiled;
    assert_eq!(a, b);
    let mut rerolled = rifle_inventory(Some(8));
    let upgrade = &mut rerolled["Upgrades"][0];
    let mut fp: Value =
        serde_json::from_str(upgrade["UpgradeFingerprint"].as_str().unwrap()).unwrap();
    fp["buffs"][0]["Value"] = json!((MAX_ROLL_INT * 0.55).round());
    upgrade["UpgradeFingerprint"] = json!(fp.to_string());
    let c = decode_all_rivens(&rerolled, data, rolls).unveiled;
    assert_ne!(a[0].stats[0].roll_float, c[0].stats[0].roll_float);
}

fn veiled(challenges: &[Value]) -> Value {
    json!({ "Upgrades": challenges.iter().map(|c| json!({
        "ItemType": "/Lotus/Upgrades/Mods/Randomized/LotusPistolRandomModRare",
        "UpgradeFingerprint": json!({ "challenge": c }).to_string(),
    })).collect::<Vec<_>>() })
}

#[test]
fn veiled_challenges_share_a_group_when_only_count_and_complication_differ() {
    let data = RivenData::default();
    let rolls = GoodRolls::default();
    let ch = "/Lotus/Types/Challenges/Seasons/";
    let inv = veiled(&[
        json!({"Type": format!("{ch}RandomizedHeadshot"), "Progress": 3, "Required": 40}),
        json!({"Type": format!("{ch}DJRandomizedHeadshot"), "Required": 15,
               "Complication": "/Lotus/Types/Challenges/Complications/ResetOnDamageTaken"}),
        json!({"Type": format!("{ch}RandomizedFisherman"), "Required": 8}),
        json!({"Type": format!("{ch}LimitedSynthesis")}),
        json!({"Type": format!("{ch}RandomizedTameKavats"), "Required": 2}),
        json!({"Progress": 0, "Required": 5}),
    ]);
    let veiled = decode_all_rivens(&inv, &data, &rolls).veiled;
    let desc: Vec<_> = veiled.iter().map(|e| e.challenge_desc.as_deref()).collect();
    assert_eq!(
        desc,
        [
            Some("Kill 40 Enemies with Headshots"),
            Some("Kill 15 Enemies with Headshots without taking damage"),
            Some("Catch 8 fish without missing a throw"),
            Some(
                "Synthesize a Simaris target without using Traps or Abilities while having a Hobbled Dragon Key equipped"
            ),
            Some("Randomized Tame Kavats"),
            None,
        ]
    );
    let group: Vec<_> = veiled
        .iter()
        .map(|e| e.challenge_group.as_deref())
        .collect();
    assert_eq!(group[0], Some("Kill Enemies with Headshots"));
    assert_eq!(group[1], group[0]);
    assert_eq!(group[2], Some("Catch fish without missing a throw"));
    assert_eq!(group[4], Some("Randomized Tame Kavats"));
    assert_eq!(veiled[5].challenge_type, None);
}

#[test]
fn unseen_veiled_mods_are_counted_per_weapon_class() {
    let inv = json!({ "RawUpgrades": [
        {"ItemType": "/Lotus/Upgrades/Mods/Randomized/RawRifleRandomMod", "ItemCount": 2},
        {"ItemType": "/Lotus/Upgrades/Mods/Randomized/RawRifleRandomMod"},
        {"ItemType": "/Lotus/Upgrades/Mods/Randomized/RawMeleeRandomMod", "ItemCount": 4},
        {"ItemType": "/Lotus/Upgrades/Mods/Other", "ItemCount": 9},
    ]});
    let got = decode_all_rivens(&inv, &RivenData::default(), &GoodRolls::default()).veiled_unseen;
    let rows: Vec<_> = got.iter().map(|g| (g.label.as_str(), g.count)).collect();
    assert_eq!(rows, [("Rifle Riven Mod", 3.0), ("Melee Riven Mod", 4.0)]);
}

#[test]
fn a_stray_damage_icon_between_the_value_and_the_label_is_ignored() {
    for icon in ["", "☠", "®", "e", "©", "&", "o", "W", "*"] {
        let text = format!(
            "Orthos Toxitis\n+145% {icon}Toxin\n+122.2% Critical Damage\n+90% Multishot\n-70.5% Zoom"
        );
        let stats = parse_riven_stats(&text, None);
        let first = stats.first().map(|s| (s.name.as_str(), s.value));
        assert_eq!(
            first,
            Some(("Toxin", Some(145.0))),
            "icon {icon:?}: {stats:?}"
        );
        assert_eq!(stats.len(), 4, "icon {icon:?}: {stats:?}");
    }
}

#[test]
fn a_french_poison_line_reads_through_a_stray_icon_glyph() {
    let Some(root) = crate::services::bundled_data::test_root() else {
        return;
    };
    let labels = super::locale::CardLabels::load(&BundledData::new(root), "fr");
    for icon in ["", " ", "☠", "®", "e", "©", "&", "o", "W", "*", "8"] {
        let text = format!(
            "Orthos Toxitis\n+145% {icon}Poison\n+122,2% de Dégâts\nCritiques\n-70,5% de Chances de\nCritique pour l'Attaque\nGlissée"
        );
        let stats = parse_riven_stats(&labels.to_english(&text), None);
        let got: Vec<_> = stats
            .iter()
            .map(|s| (s.name.as_str(), s.positive))
            .collect();
        assert_eq!(
            got,
            [
                ("Toxin", true),
                ("Critical Damage", true),
                ("Critical Chance for Slide Attack", false)
            ],
            "icon {icon:?}"
        );
    }
}
