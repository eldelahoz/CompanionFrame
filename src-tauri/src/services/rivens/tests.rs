//! Ported from WFHelper's rivenGrading / rivenBestAttributes / rivenGoodRoll tests. The weapon
//! tables come from the real bundle, so those tests skip when `pnpm game-data` has not run.

use std::sync::OnceLock;

use super::best_attributes::{GoodRoll, GoodRollData, GoodRolls, SheetClass, parse_good_roll_csv};
use super::data::RivenData;
use super::grading::*;
use crate::services::bundled_data::BundledData;

fn data() -> Option<&'static RivenData> {
    static DATA: OnceLock<Option<RivenData>> = OnceLock::new();
    DATA.get_or_init(|| {
        crate::services::bundled_data::test_root()
            .map(|root| RivenData::load(&BundledData::new(root)))
    })
    .as_ref()
}

/// Binds `$d` to the real data or skips the test.
macro_rules! real {
    ($d:ident) => {
        let Some($d) = data() else { return };
    };
}

fn s(name: &str, positive: bool, value: f64) -> RivenStatInput {
    RivenStatInput {
        name: name.into(),
        positive,
        display_positive: None,
        value: Some(value),
        multiplier: None,
    }
}

fn mult(name: &str, positive: bool, value: f64) -> RivenStatInput {
    RivenStatInput {
        multiplier: Some(true),
        ..s(name, positive, value)
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| (*x).into()).collect()
}

fn roll(mandatory: &[&str], optional: &[&str]) -> GoodRoll {
    GoodRoll {
        mandatory: strs(mandatory),
        optional: strs(optional),
    }
}

fn sheet(rows: Vec<(&str, GoodRollData)>, updated_at: Option<&str>) -> GoodRolls {
    GoodRolls::new(
        rows.into_iter().map(|(n, d)| (n.to_owned(), d)).collect(),
        updated_at.map(str::to_owned),
    )
}

fn single(mandatory: &str) -> GoodRollData {
    GoodRollData {
        good_attrs: vec![roll(&[mandatory], &[])],
        accepted_bad_attrs: vec![],
    }
}

fn grading_sheet() -> GoodRolls {
    sheet(
        vec![
            (
                "lex",
                GoodRollData {
                    good_attrs: vec![roll(
                        &["WeaponCritDamageMod"],
                        &[
                            "WeaponFireIterationsMod",
                            "WeaponToxinDamageMod",
                            "WeaponDamageAmountMod",
                            "WeaponFireRateMod",
                            "WeaponCritChanceMod",
                            "WeaponPunctureDepthMod",
                        ],
                    )],
                    accepted_bad_attrs: strs(&[
                        "WeaponZoomFovMod",
                        "WeaponRecoilReductionMod",
                        "WeaponArmorPiercingDamageMod",
                    ]),
                },
            ),
            (
                "galatine",
                GoodRollData {
                    good_attrs: vec![roll(
                        &[
                            "WeaponCritDamageMod",
                            "WeaponFireRateMod",
                            "WeaponMeleeRangeIncMod",
                        ],
                        &[],
                    )],
                    accepted_bad_attrs: strs(&[
                        "WeaponMeleeComboEfficiencyMod",
                        "SlideAttackCritChanceMod",
                        "WeaponMeleeFinisherDamageMod",
                    ]),
                },
            ),
            (
                "angstrum",
                GoodRollData {
                    good_attrs: vec![
                        roll(
                            &["WeaponCritDamageMod"],
                            &[
                                "WeaponFireIterationsMod",
                                "WeaponToxinDamageMod",
                                "WeaponDamageAmountMod",
                                "WeaponFireRateMod",
                                "WeaponCritChanceMod",
                            ],
                        ),
                        roll(
                            &["WeaponFireIterationsMod", "WeaponDamageAmountMod"],
                            &["WeaponStunChanceMod", "WeaponToxinDamageMod"],
                        ),
                    ],
                    accepted_bad_attrs: strs(&["WeaponZoomFovMod"]),
                },
            ),
        ],
        None,
    )
}

fn grade(d: &RivenData, weapon: &str, stats: &[RivenStatInput]) -> Option<RivenGradeResult> {
    grade_riven(d, &grading_sheet(), weapon, stats, None)
}

fn grade_at(
    d: &RivenData,
    weapon: &str,
    stats: &[RivenStatInput],
    rank: i32,
) -> Option<RivenGradeResult> {
    grade_riven(d, &grading_sheet(), weapon, stats, Some(rank))
}

fn grades(r: &RivenGradeResult) -> Vec<&str> {
    r.stats.iter().map(|x| x.grade.as_str()).collect()
}

fn near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() < tol, "{a} vs {b}");
}

fn interior(r: &RivenGradeResult) {
    for st in &r.stats {
        assert!(st.roll_float > 0.0 && st.roll_float < 1.0, "{st:?}");
    }
}

// ---- floatToGrade / unparse ----

#[test]
fn float_to_grade_boundaries_match_the_parser() {
    assert_eq!(float_to_grade(1.0, false), "S");
    assert_eq!(float_to_grade(0.0, false), "F");
    assert_eq!(float_to_grade(0.5, false), "B");
    for (f, g) in [
        (0.975, "S"),
        (0.974, "A+"),
        (0.875, "A+"),
        (0.874, "A"),
        (0.775, "A"),
        (0.774, "A-"),
        (0.675, "A-"),
        (0.674, "B+"),
        (0.575, "B+"),
        (0.574, "B"),
        (0.425, "B"),
        (0.424, "B-"),
        (0.325, "B-"),
        (0.324, "C+"),
        (0.025, "C-"),
        (0.024, "F"),
    ] {
        assert_eq!(float_to_grade(f, false), g, "{f}");
    }
}

#[test]
fn float_to_grade_inverts_for_curses() {
    assert_eq!(float_to_grade(1.0, true), "F");
    assert_eq!(float_to_grade(0.0, true), "S");
    assert_eq!(float_to_grade(0.5, true), "B");
}

const CC: Option<&str> = Some("WeaponCritChanceMod");
const RECOIL: Option<&str> = Some("WeaponRecoilReductionMod");

#[test]
fn unparse_buff_reference_values() {
    near(unparse_buff(157.5, 0.016666, 0.7, 1, 0, CC, 8), 0.5, 0.05);
    near(unparse_buff(173.2, 0.016666, 0.7, 1, 0, CC, 8), 1.0, 0.05);
    near(unparse_buff(141.7, 0.016666, 0.7, 1, 0, CC, 8), 0.0, 0.05);
    // Curse attenuation boost: pow(1.25, numCurses).
    near(unparse_buff(78.7, 0.016666, 0.7, 3, 0, CC, 8), 0.5, 0.05);
    near(unparse_buff(98.4, 0.016666, 0.7, 3, 1, CC, 8), 0.5, 0.05);
    // Rubico Prime crit, roll 0.95.
    near(unparse_buff(107.3, 0.016666, 0.7, 3, 1, CC, 8), 0.95, 0.05);
    assert_eq!(unparse_buff(50.0, 0.0, 1.0, 1, 0, None, 8), 0.5);
}

#[test]
fn unparse_buff_reports_the_raw_roll_for_impossible_values() {
    near(
        unparse_buff(9999.0, 0.016666, 0.7, 1, 0, CC, 8),
        312.94,
        0.05,
    );
    near(unparse_buff(0.0, 0.016666, 0.7, 1, 0, CC, 8), -4.5, 1e-10);
}

#[test]
fn unparse_curse_reference_values() {
    let r = unparse_curse(49.1, -0.01, 0.7, 3, 1, RECOIL, 8);
    assert!((0.0..=1.0).contains(&r));
    near(r, 0.696, 0.05);
    near(
        unparse_curse(-49.1, -0.01, 0.7, 3, 1, RECOIL, 8),
        unparse_curse(49.1, -0.01, 0.7, 3, 1, RECOIL, 8),
        1e-5,
    );
    // Swapped indexing: buffs table by curse count, curse table by buff count.
    let a = unparse_curse(30.0, 0.01, 1.0, 3, 1, None, 8);
    let b = unparse_curse(30.0, 0.01, 1.0, 2, 2, None, 8);
    assert!((a - b).abs() >= 0.005);
    assert_eq!(unparse_curse(-25.0, 0.0, 1.0, 1, 1, None, 8), 0.5);
}

// ---- rivenData ----

#[test]
fn dispositions() {
    real!(d);
    let dispo = d.weapon_disposition("Rubico Prime").unwrap();
    assert!(dispo > 0.0 && dispo < 2.0);
    assert_eq!(d.weapon_disposition("rubico prime"), Some(dispo));
    assert_eq!(d.weapon_disposition("Nonexistent Weapon"), None);
}

#[test]
fn duviri_melee_resolves_to_the_tenno_weapon_not_the_drifter_twin() {
    real!(d);
    for (name, dispo) in [
        ("Sampotes", 1.0),
        ("Syam", 0.75),
        ("Edun", 1.15),
        ("Argo & Vel", 1.15),
        ("Sun & Moon", 0.8),
        ("Azothane", 1.1),
    ] {
        assert_eq!(d.weapon_disposition(name), Some(dispo), "{name}");
    }
    assert_eq!(d.family_variants("Edun"), vec![("Edun".to_owned(), 1.15)]);
}

#[test]
fn is_melee_weapon() {
    real!(d);
    assert!(!d.is_melee_weapon("Nonexistent"));
    assert!(!d.is_melee_weapon("Rubico Prime"));
    assert!(!d.is_melee_weapon("Bolto"));
    assert!(d.is_melee_weapon("Skana"));
    // Zaw strikes export as Pistols; a melee-holstered companion weapon is melee too.
    assert!(d.is_melee_weapon("Sepfahn"));
    assert!(d.is_melee_weapon("Plague Kripath"));
    assert!(!d.is_melee_weapon("Catchmoon"));
    assert!(d.is_melee_weapon("Edun"));
    assert!(d.is_melee_weapon("Deconstructor"));
    assert!(!d.is_melee_weapon("Verglas"));
}

fn riven_type(d: &RivenData, weapon: &str, expect: &str) {
    let t = d.resolve_riven_type(weapon);
    assert!(t.is_some_and(|t| t.contains(expect)), "{weapon}: {t:?}");
}

#[test]
fn resolve_riven_type_by_class() {
    real!(d);
    riven_type(d, "Rubico Prime", "RifleRandomModRare");
    riven_type(d, "Bolto", "PistolRandomModRare");
    riven_type(d, "Skana", "MeleeWeaponRandomModRare");
    riven_type(d, "Boar", "ShotgunRandomModRare");
    riven_type(d, "Tigris Prime", "ShotgunRandomModRare");
    riven_type(d, "Kuva Sobek", "ShotgunRandomModRare");
    for w in [
        "Drakgoon",
        "Kuva Drakgoon",
        "Convectrix",
        "Phage",
        "Bubonico",
        "Coda Bubonico",
        "Steflos",
    ] {
        riven_type(d, w, "ShotgunRandomModRare");
    }
    riven_type(d, "Quanta", "RifleRandomModRare");
    riven_type(d, "Cernos Prime", "RifleRandomModRare");
    riven_type(d, "Sepfahn", "ModularMeleeRandomModRare");
    riven_type(d, "Catchmoon", "ModularPistolRandomModRare");
    riven_type(d, "Sun & Moon", "MeleeWeaponRandomModRare");
    riven_type(d, "Edun", "MeleeWeaponRandomModRare");
}

#[test]
fn companion_weapons_roll_the_pool_of_the_class_they_holster_as() {
    real!(d);
    riven_type(d, "Verglas", "RifleRandomModRare");
    riven_type(d, "Verglas Prime", "RifleRandomModRare");
    riven_type(d, "Vulklok", "RifleRandomModRare");
    riven_type(d, "Sweeper", "ShotgunRandomModRare");
    riven_type(d, "Deconstructor", "MeleeWeaponRandomModRare");
    riven_type(d, "Burst Laser", "PistolRandomModRare");
    riven_type(d, "Prisma Burst Laser", "PistolRandomModRare");
}

#[test]
fn no_riven_type_for_weapons_that_cannot_roll_one() {
    real!(d);
    for w in [
        "Artemis Bow",
        "Shadow Claws",
        "Sirocco",
        "Batoten",
        "Nonexistent",
    ] {
        assert_eq!(d.resolve_riven_type(w), None, "{w}");
    }
}

#[test]
fn family_variants() {
    real!(d);
    let names: Vec<String> = d.family_variants("Boar").into_iter().map(|v| v.0).collect();
    assert!(names.contains(&"Boar".to_owned()) && names.contains(&"Boar Prime".to_owned()));
    assert!(d.family_variants("Boar").iter().all(|v| v.1 > 0.0));
    assert_eq!(
        d.family_variants("Tombfinger"),
        vec![("Tombfinger".to_owned(), 0.85)]
    );
    assert_eq!(
        d.family_variants("Rattleguts"),
        vec![("Rattleguts".to_owned(), 1.0)]
    );
}

#[test]
fn family_slug() {
    real!(d);
    for (name, slug) in [
        ("Boar Prime", "boar"),
        ("Rubico Prime", "rubico"),
        ("Kuva Karak", "karak"),
        ("MK1-Braton", "braton"),
        ("Prisma Obex", "obex"),
        ("Prisma Grakata", "grakata"),
        ("Prisma Gorgon", "gorgon"),
        ("Dex Nikana", "dex_nikana"),
        ("Gotva Prime", "gotva_prime"),
        ("Kuva Bramma", "kuva_bramma"),
        ("Tenet Envoy", "tenet_envoy"),
    ] {
        assert_eq!(d.family_slug(name), slug, "{name}");
    }
}

#[test]
fn stat_name_to_tag() {
    use super::vocab::stat_name_to_tag as tag;
    assert_eq!(tag("Critical Chance"), Some("WeaponCritChanceMod"));
    assert_eq!(tag("Multishot"), Some("WeaponFireIterationsMod"));
    assert_eq!(tag("Damage"), Some("WeaponDamageAmountMod"));
    assert_eq!(tag("CRITICAL CHANCE"), Some("WeaponCritChanceMod"));
    assert_eq!(tag("Attack Speed"), Some("WeaponFireRateMod"));
    assert_eq!(tag("Range"), Some("WeaponMeleeRangeIncMod"));
    assert_eq!(tag("Melee Damage"), Some("WeaponMeleeDamageMod"));
    assert_eq!(tag("Nonexistent Stat"), None);
}

#[test]
fn find_upgrade_entry() {
    real!(d);
    let t = d.resolve_riven_type("Rubico Prime").unwrap();
    let e = d.find_upgrade_entry(t, "WeaponCritChanceMod").unwrap();
    assert_eq!(e.tag, "WeaponCritChanceMod");
    assert!(e.base_value > 0.0);
    assert!(d.find_upgrade_entry(t, "NonexistentTag").is_none());
}

#[test]
fn generate_suffix_orders_buffs_by_roll() {
    real!(d);
    let t = d.resolve_riven_type("Boar").unwrap();
    let name = |buffs: &[(&str, f64)]| d.generate_suffix(t, buffs);
    assert_eq!(
        name(&[
            ("WeaponFireRateMod", 500_000_000.0),
            ("WeaponFireIterationsMod", 900_000_000.0)
        ]),
        "Satidra"
    );
    assert_eq!(
        name(&[
            ("WeaponFireIterationsMod", 500_000_000.0),
            ("WeaponCritChanceMod", 900_000_000.0)
        ]),
        "Critacan"
    );
    assert_eq!(
        name(&[
            ("WeaponFireIterationsMod", 900_000_000.0),
            ("WeaponCritChanceMod", 600_000_000.0),
            ("WeaponFireRateMod", 300_000_000.0)
        ]),
        "Sati-critadra"
    );
}

// ---- gradeRiven ----

#[test]
fn grade_rejects_empty_unknown_and_impossible() {
    real!(d);
    assert!(grade(d, "Rubico Prime", &[]).is_none());
    assert!(grade(d, "Made Up Weapon", &[s("Critical Chance", true, 100.0)]).is_none());
    let four_buffs = [
        s("Status Chance", true, 42.6),
        s("Heat", true, 41.2),
        s("Reload Speed", true, 23.0),
        s("Zoom", true, 29.8),
    ];
    assert!(grade(d, "Kuva Nukor", &four_buffs).is_none());
}

#[test]
fn grades_single_and_multiple_stats() {
    real!(d);
    let one = grade(d, "Rubico Prime", &[s("Critical Chance", true, 90.0)]).unwrap();
    assert_eq!(one.stats.len(), 1);
    assert!(!one.stats[0].grade.is_empty() && !one.overall_grade.is_empty());
    assert!((0.0..=1.0).contains(&one.stats[0].roll_float));

    let three = grade(
        d,
        "Rubico Prime",
        &[
            s("Critical Chance", true, 90.0),
            s("Multishot", true, 70.0),
            s("Zoom", false, 30.0),
        ],
    )
    .unwrap();
    assert_eq!(three.stats.len(), 3);
    assert!(!three.stats[2].positive);
}

#[test]
fn clamps_a_roll_no_rank_or_disposition_can_fit() {
    real!(d);
    let r = grade(d, "Rubico Prime", &[s("Critical Chance", true, 9999.0)]).unwrap();
    assert_eq!(r.stats[0].roll_float, 1.0);
    assert_eq!(r.stats[0].grade, "S");
}

#[test]
fn grades_a_duviri_melee_card_against_the_tenno_disposition() {
    real!(d);
    let r = grade(
        d,
        "Edun",
        &[
            s("Melee Damage", true, 177.6),
            s("Critical Damage", true, 101.3),
            s("Attack Speed", true, 56.8),
            s("Impact", false, 101.2),
        ],
    )
    .unwrap();
    interior(&r);
    assert_eq!(grades(&r), ["B", "A-", "C+", "B+"]);
}

#[test]
fn grades_shotguns_against_shotgun_bases() {
    real!(d);
    let r = grade(
        d,
        "Kuva Drakgoon",
        &[
            s("Reload Speed", true, 78.1),
            s("Multishot", true, 181.6),
            s("Status Duration", false, 52.3),
        ],
    )
    .unwrap();
    interior(&r);

    let r = grade(
        d,
        "Boar",
        &[
            s("Multishot", true, 199.3),
            s("Critical Chance", true, 163.6),
            s("Slash", false, 75.9),
        ],
    )
    .unwrap();
    interior(&r);
    assert_eq!(r.stats[0].grade, "B");
    assert!(["S", "A+"].contains(&r.stats[1].grade.as_str()));
    assert_eq!(r.stats[2].grade, "A-");
}

#[test]
fn grades_a_kuva_nukor_card_with_nothing_clamped() {
    real!(d);
    let r = grade(
        d,
        "Kuva Nukor",
        &[
            s("Status Chance", true, 42.6),
            s("Heat", true, 41.2),
            s("Reload Speed", true, 23.0),
            s("Zoom", false, 29.8),
        ],
    )
    .unwrap();
    interior(&r);
}

#[test]
fn unknown_stat_names_get_a_b_and_null_values_a_question_mark() {
    real!(d);
    let r = grade(
        d,
        "Rubico Prime",
        &[
            s("Critical Chance", true, 90.0),
            s("Some Unknown Stat", true, 42.0),
        ],
    )
    .unwrap();
    assert_eq!(
        (r.stats[1].grade.as_str(), r.stats[1].roll_float),
        ("B", 0.5)
    );

    let none = RivenStatInput {
        value: None,
        ..s("Critical Chance", true, 0.0)
    };
    assert_eq!(
        grade(d, "Rubico Prime", &[none]).unwrap().stats[0].grade,
        "?"
    );
}

#[test]
fn grades_the_linked_boar_prime_card_against_its_exact_disposition() {
    real!(d);
    let r = grade(
        d,
        "Boar Prime",
        &[
            s("Critical Damage", true, 109.9),
            s("Damage", true, 202.5),
            s("Critical Chance", true, 124.0),
            s("Impact", false, 121.0),
        ],
    )
    .unwrap();
    assert_eq!(grades(&r), ["B-", "B-", "S", "B"]);
}

#[test]
fn handles_the_x_multiplier_format() {
    real!(d);
    let r = grade(d, "Rubico Prime", &[mult("Critical Damage", true, 1.59)]).unwrap();
    assert!((0.0..=1.0).contains(&r.stats[0].roll_float));
}

#[test]
fn grades_a_kitgun_chamber_against_its_own_disposition() {
    real!(d);
    assert_eq!(d.weapon_disposition("Tombfinger"), Some(0.85));
    let r = grade(
        d,
        "Tombfinger",
        &[
            s("Critical Damage", true, 66.4),
            s("Multishot", true, 86.4),
            s("Critical Chance", true, 115.6),
            s("Zoom", false, 51.1),
        ],
    )
    .unwrap();
    assert_eq!(grades(&r), ["C", "C-", "B-", "B"]);
}

#[test]
fn faction_multipliers_grade_inside_their_range_and_in_order() {
    real!(d);
    let r = grade(
        d,
        "Tatsu",
        &[
            s("Status Duration", true, 111.6),
            mult("Damage to Corpus", true, 1.51),
            s("Slash", false, 58.5),
        ],
    )
    .unwrap();
    let faction = &r.stats[1];
    assert!(faction.roll_float > 0.0 && faction.roll_float < 1.0);

    let at = |v: f64| {
        grade(
            d,
            "Proboscis Cernos",
            &[
                mult("Damage to Infested", true, v),
                s("Projectile Speed", true, 77.2),
            ],
        )
        .unwrap()
        .stats[0]
            .roll_float
    };
    assert!(at(1.4) > at(1.36));
}

// ---- correctScannedStats ----

fn nami(middle: &str) -> Vec<RivenStatInput> {
    vec![
        s("Additional Combo Count Chance", true, 69.3),
        s(middle, true, 190.2),
        s("Heat", true, 95.8),
    ]
}

fn names(stats: &[RivenStatInput]) -> Vec<&str> {
    stats.iter().map(|x| x.name.as_str()).collect()
}

#[test]
fn correct_renames_a_garbled_stat_to_the_sibling_that_fits() {
    real!(d);
    for garbled in ["Damage", "Critical Damage"] {
        let (stats, n) = correct_scanned_stats(d, "Nami Solo", &nami(garbled));
        assert_eq!(n, 1, "{garbled}");
        assert_eq!(stats[1].name, "Melee Damage");
        assert_eq!(stats[1].value, Some(190.2));
    }
}

#[test]
fn correct_leaves_good_and_unfixable_cards_alone() {
    real!(d);
    let roll = nami("Melee Damage");
    let (stats, n) = correct_scanned_stats(d, "Nami Solo", &roll);
    assert_eq!(n, 0);
    assert_eq!(names(&stats), names(&roll));

    let (stats, n) = correct_scanned_stats(d, "Nami Solo", &[s("Melee Damage", true, 1000.0)]);
    assert_eq!(
        (n, stats[0].name.as_str(), stats[0].value),
        (0, "Melee Damage", Some(1000.0))
    );

    let (_, n) = correct_scanned_stats(
        d,
        "Soma",
        &[s("Critical Chance", true, 150.0), s("Damage", true, 165.0)],
    );
    assert_eq!(n, 0);
}

#[test]
fn correct_skips_multiplier_and_valueless_stats() {
    real!(d);
    let input = vec![
        mult("Damage to Grineer", true, 1.3),
        RivenStatInput {
            value: None,
            ..s("Damage", true, 0.0)
        },
    ];
    let (stats, n) = correct_scanned_stats(d, "Soma", &input);
    assert_eq!(n, 0);
    assert_eq!(names(&stats), ["Damage to Grineer", "Damage"]);
}

#[test]
fn correct_keeps_names_when_a_dropped_curse_explains_the_misfit() {
    real!(d);
    let buffs = vec![
        s("Range", true, 2.3),
        s("Critical Damage", true, 104.6),
        s("Finisher Damage", true, 141.8),
    ];
    let (stats, n) = correct_scanned_stats(d, "Prisma Obex", &buffs);
    assert_eq!(n, 0);
    assert_eq!(
        names(&stats),
        ["Range", "Critical Damage", "Finisher Damage"]
    );

    let mut with_curse = buffs;
    with_curse.push(s("Impact", false, 104.0));
    let (stats, n) = correct_scanned_stats(d, "Prisma Obex", &with_curse);
    assert_eq!(n, 0);
    assert_eq!(
        names(&stats),
        ["Range", "Critical Damage", "Finisher Damage", "Impact"]
    );
}

#[test]
fn correct_does_not_rename_when_more_than_one_stat_misfits() {
    real!(d);
    let roll = vec![
        s("Critical Damage", true, 47.4),
        s("Heat", true, 44.8),
        s("Finisher Damage", true, 63.0),
    ];
    let (stats, n) = correct_scanned_stats(d, "Pride", &roll);
    assert_eq!(n, 0);
    assert_eq!(
        names(&stats),
        ["Critical Damage", "Heat", "Finisher Damage"]
    );
    assert_eq!(correct_scanned_stats(d, "Paracesis", &roll).1, 0);

    let absurd = vec![
        s("Damage", true, 4000.0),
        s("Critical Damage", true, 4000.0),
        s("Heat", true, 4000.0),
    ];
    let (stats, n) = correct_scanned_stats(d, "Nami Solo", &absurd);
    assert_eq!(n, 0);
    assert_eq!(names(&stats), ["Damage", "Critical Damage", "Heat"]);
}

// ---- unranked cards / stated rank ----

fn wolf() -> Vec<RivenStatInput> {
    vec![
        s("Range", true, 0.2),
        s("Critical Damage", true, 12.3),
        s("Attack Speed", true, 7.3),
        s("Impact", false, 12.2),
    ]
}

fn scaled(stats: &[RivenStatInput], k: f64) -> Vec<RivenStatInput> {
    stats
        .iter()
        .map(|x| RivenStatInput {
            value: x.value.map(|v| v * k),
            ..x.clone()
        })
        .collect()
}

#[test]
fn grades_unranked_cards_on_their_own_rank() {
    real!(d);
    let obex = vec![
        s("Range", true, 0.3),
        s("Electricity", true, 11.4),
        s("Status Chance", true, 13.2),
        s("Impact", false, 12.9),
    ];
    let panthera = vec![
        s("Zoom", true, 10.1),
        s("Cold", true, 16.3),
        s("Status Chance", false, 6.6),
    ];
    for (weapon, stats) in [
        ("Wolf Sledge", wolf()),
        ("Obex", obex),
        ("Panthera", panthera),
    ] {
        let r = grade(d, weapon, &stats).unwrap();
        assert!(
            r.stats
                .iter()
                .any(|x| x.roll_float > 0.0 && x.roll_float < 1.0),
            "{weapon}"
        );
        assert_ne!(r.overall_grade, "F", "{weapon}");
    }
}

#[test]
fn unranked_and_max_rank_scans_grade_alike() {
    real!(d);
    let unranked = grade(d, "Wolf Sledge", &wolf()).unwrap();
    let max = grade(d, "Wolf Sledge", &scaled(&wolf(), 9.0)).unwrap();
    assert_eq!(grades(&max), grades(&unranked));
}

#[test]
fn grades_an_unranked_sentinel_weapon_on_the_rifle_pool() {
    real!(d);
    let r = grade(
        d,
        "Verglas",
        &[
            s("Critical Damage", true, 16.4),
            s("Damage", true, 23.9),
            s("Critical Chance", true, 18.5),
            mult("Damage to Infested", false, 0.95),
        ],
    )
    .unwrap();
    interior(&r);
}

#[test]
fn a_rank_needs_two_stats_and_a_dropped_curse_does_not_change_the_buffs() {
    real!(d);
    let lone = grade(d, "Wolf Sledge", &[s("Critical Damage", true, 12.3)]).unwrap();
    assert_eq!(lone.stats[0].roll_float, 0.0);

    let max = scaled(&wolf(), 9.0);
    let buffs_only: Vec<_> = max.iter().filter(|x| x.positive).cloned().collect();
    let whole = grade(d, "Wolf Sledge", &max).unwrap();
    let dropped = grade(d, "Wolf Sledge", &buffs_only).unwrap();
    let whole_buffs: Vec<_> = whole
        .stats
        .iter()
        .filter(|x| x.positive)
        .map(|x| x.grade.as_str())
        .collect();
    assert_eq!(grades(&dropped), whole_buffs);
}

#[test]
fn a_card_that_fits_no_rank_stays_at_the_bottom() {
    real!(d);
    let r = grade(d, "Wolf Sledge", &scaled(&wolf(), 90.0)).unwrap();
    assert!(
        r.stats
            .iter()
            .all(|x| x.roll_float == 0.0 || x.roll_float == 1.0)
    );
}

fn boar_rank0() -> Vec<RivenStatInput> {
    vec![
        s("Critical Chance", true, 17.2),
        s("Multishot", true, 22.6),
        s("Ammo Maximum", false, 7.2),
    ]
}

fn boar_rank8() -> Vec<RivenStatInput> {
    boar_rank0()
        .into_iter()
        .map(|x| RivenStatInput {
            value: x.value.map(|v| (v * 9.0 * 10.0).round() / 10.0),
            ..x
        })
        .collect()
}

#[test]
fn a_stated_rank_grades_a_rank_0_card_like_its_rank_8_twin() {
    real!(d);
    let stated = grade_at(d, "Boar", &boar_rank0(), 0).unwrap();
    let max = grade_at(d, "Boar", &boar_rank8(), 8).unwrap();
    assert!(
        stated
            .stats
            .iter()
            .all(|x| x.roll_float > 0.0 && x.roll_float < 1.0)
    );
    assert_eq!(grades(&stated), grades(&max));
    assert_eq!(stated.overall_grade, max.overall_grade);
}

#[test]
fn a_stated_rank_that_clamps_every_buff_falls_back_to_the_search() {
    real!(d);
    let searched = grade(d, "Boar", &boar_rank8()).unwrap();
    let as_rank0 = grade_at(d, "Boar", &boar_rank8(), 0).unwrap();
    assert_eq!(grades(&as_rank0), grades(&searched));
    assert_eq!(as_rank0.overall_grade, searched.overall_grade);
    assert!(
        !as_rank0
            .stats
            .iter()
            .all(|x| x.roll_float == 0.0 || x.roll_float == 1.0)
    );
}

#[test]
fn the_stated_rank_beats_the_searched_one_and_nonsense_is_ignored() {
    real!(d);
    let one = &boar_rank0()[..1];
    let stated = grade_at(d, "Boar", one, 0).unwrap();
    let searched = grade(d, "Boar", one).unwrap();
    assert!(stated.stats[0].roll_float > 0.0 && stated.stats[0].roll_float < 1.0);
    assert_eq!(searched.stats[0].roll_float, 0.0);

    let by_search = grade(d, "Boar", &boar_rank0()).unwrap();
    assert_eq!(
        grades(&by_search),
        grades(&grade_at(d, "Boar", &boar_rank0(), 0).unwrap())
    );
    assert_eq!(
        grades(&grade_at(d, "Boar", &boar_rank0(), 42).unwrap()),
        grades(&by_search)
    );
}

// ---- best attributes ----

#[test]
fn best_attributes_come_from_the_sheet() {
    real!(d);
    let rolls = grading_sheet();
    let lex = rolls.best_attributes(d, "Lex", false).unwrap();
    assert!(lex.positives.contains(&"Critical Damage".to_owned()));
    assert!(!lex.negatives.is_empty());

    let gal = rolls.best_attributes(d, "Galatine", true).unwrap();
    assert!(gal.positives.contains(&"Attack Speed".to_owned()));
    assert!(!gal.positives.contains(&"Fire Rate".to_owned()));

    let ang = rolls.best_attributes(d, "Angstrum", false).unwrap();
    assert_eq!(
        ang.positives,
        [
            "Critical Damage",
            "Multishot",
            "Damage",
            "Toxin",
            "Fire Rate",
            "Critical Chance",
            "Status Chance"
        ]
    );
    assert_eq!(ang.negatives, ["Zoom"]);
    assert!(rolls.best_attributes(d, "NotAWeaponName", false).is_none());
}

// ---- good-roll lookup (rivenGoodRoll.test.ts) ----

fn lookup_sheet(updated_at: Option<&str>) -> GoodRolls {
    sheet(
        vec![
            ("boltor", single("WeaponCritChanceMod")),
            ("hek", single("WeaponStunChanceMod")),
            ("bubonico", single("WeaponFireIterationsMod")),
            ("laser rifle", single("WeaponCritDamageMod")),
            ("penta", single("WeaponDamageAmountMod")),
            ("lacera", single("WeaponMeleeDamageMod")),
            ("detron", single("WeaponFireRateMod")),
            ("nikana", single("WeaponMeleeRangeIncMod")),
            ("karak", single("WeaponReloadSpeedMod")),
            ("braton", single("WeaponClipMaxMod")),
            ("telos akbolto", single("WeaponProcTimeMod")),
            ("akbolto", single("WeaponAmmoMaxMod")),
            ("bramma", single("WeaponImpactDamageMod")),
            ("gotva", single("WeaponSlashDamageMod")),
        ],
        updated_at,
    )
}

fn row<'a>(rolls: &'a GoodRolls, key: &str) -> Option<&'a GoodRollData> {
    rolls.get_by_name(key)
}

#[test]
fn freshness() {
    let ago = |secs: i64| (chrono::Utc::now() - chrono::Duration::seconds(secs)).to_rfc3339();
    assert!(!lookup_sheet(Some(&ago(8 * 24 * 3600))).is_current());
    assert!(lookup_sheet(Some(&ago(60))).is_current());
    assert!(!sheet(vec![], Some(&ago(0))).is_current());
    // A null timestamp keeps the loader from treating an injected sheet as stale.
    assert!(lookup_sheet(None).is_current());
}

#[test]
fn lookup_reaches_the_base_row_through_affixes() {
    real!(d);
    let rolls = lookup_sheet(None);
    let get = |w: &str| rolls.get(d, w);
    for (weapon, key) in [
        ("Boltor", "boltor"),
        ("Telos Boltor", "boltor"),
        ("Vaykor Hek", "hek"),
        ("Secura Penta", "penta"),
        ("Coda Bubonico", "bubonico"),
        ("Dex Nikana", "nikana"),
        ("Mara Detron", "detron"),
        ("Ceti Lacera", "lacera"),
        ("Carmine Penta", "penta"),
        ("Prime Laser Rifle", "laser rifle"),
        ("Boltor Prime", "boltor"),
        ("Kuva Karak", "karak"),
        ("MK1-Braton", "braton"),
        ("Braton Vandal", "braton"),
        ("Telos Akbolto", "telos akbolto"),
        ("Akbolto Prime", "akbolto"),
    ] {
        assert_eq!(get(weapon), row(&rolls, key), "{weapon}");
        assert!(get(weapon).is_some(), "{weapon}");
    }
    for weapon in [
        "Sancti Tigris",
        "Rakta Cernos",
        "Synoid Heliocor",
        "Kuva Bramma",
        "Gotva Prime",
        "Haalvu",
        "Wrath",
        "",
    ] {
        assert!(get(weapon).is_none(), "{weapon}");
    }
}

// ---- attribute grade + detail (rivenGoodRoll.test.ts) ----

fn attr_sheet() -> GoodRolls {
    sheet(
        vec![
            (
                "lex",
                GoodRollData {
                    good_attrs: vec![
                        roll(
                            &["WeaponCritDamageMod"],
                            &["WeaponFireIterationsMod", "WeaponCritChanceMod"],
                        ),
                        roll(&["WeaponFireIterationsMod", "WeaponDamageAmountMod"], &[]),
                    ],
                    accepted_bad_attrs: strs(&["WeaponZoomFovMod", "WeaponRecoilReductionMod"]),
                },
            ),
            (
                "galatine",
                GoodRollData {
                    good_attrs: vec![roll(&["WeaponCritDamageMod", "WeaponFireRateMod"], &[])],
                    // No WFM auction attribute exists for this tag, so the prefill drops it.
                    accepted_bad_attrs: strs(&["WeaponUnmappedTestMod"]),
                },
            ),
        ],
        Some("2026-09-01T00:00:00.000Z"),
    )
}

fn card(positives: &[&str], negative: Option<&str>) -> Vec<RivenStatInput> {
    positives
        .iter()
        .map(|n| s(n, true, 0.0))
        .chain(negative.map(|n| s(n, false, 0.0)))
        .collect()
}

#[test]
fn attribute_grade() {
    real!(d);
    let rolls = attr_sheet();
    let g = |p: &[&str], n: &str, w: &str| compute_attribute_grade(d, &rolls, &card(p, Some(n)), w);
    assert_eq!(g(&["Critical Damage", "Multishot"], "Zoom", "Lex"), "Great");
    assert_eq!(
        g(&["Critical Damage", "Multishot"], "Critical Chance", "Lex"),
        "OK"
    );
    assert_eq!(g(&["Multishot"], "Zoom", "Lex"), "OK");
    assert_eq!(g(&["Reload Speed"], "Zoom", "Lex"), "Bad");
    assert_eq!(g(&["Multishot", "Damage"], "Recoil", "Lex"), "Great");
    assert_eq!(g(&["Critical Damage"], "Zoom", "Made Up Weapon"), "?");
}

#[test]
fn good_roll_detail_resolves_tags_to_wfm_names_and_labels() {
    real!(d);
    let rolls = attr_sheet();
    let detail = rolls.detail(d, "Lex Prime", false).unwrap();
    assert_eq!(detail.groups.len(), 2);
    let m = &detail.groups[0].mandatory[0];
    assert_eq!(
        (
            m.tag.as_str(),
            m.wfm_url_name.as_deref(),
            m.display_name.as_str()
        ),
        (
            "WeaponCritDamageMod",
            Some("critical_damage"),
            "Critical Damage"
        )
    );
    let opt: Vec<_> = detail.groups[0]
        .optional
        .iter()
        .map(|a| a.wfm_url_name.as_deref())
        .collect();
    assert_eq!(opt, [Some("multishot"), Some("critical_chance")]);
    let neg: Vec<_> = detail
        .accepted_negatives
        .iter()
        .map(|a| a.wfm_url_name.as_deref())
        .collect();
    assert_eq!(neg, [Some("zoom"), Some("recoil")]);
    assert_eq!(
        detail.updated_at.as_deref(),
        Some("2026-09-01T00:00:00.000Z")
    );

    let melee = rolls.detail(d, "Galatine", true).unwrap();
    let fr = &melee.groups[0].mandatory[1];
    assert_eq!(
        (fr.wfm_url_name.as_deref(), fr.display_name.as_str()),
        (Some("fire_rate_/_attack_speed"), "Attack Speed")
    );
    let unmapped = &melee.accepted_negatives[0];
    assert_eq!(
        (
            unmapped.wfm_url_name.as_deref(),
            unmapped.display_name.as_str()
        ),
        (None, "WeaponUnmappedTestMod")
    );

    assert!(rolls.detail(d, "Made Up Weapon", false).is_none());
    assert_eq!(rolls.updated_at(), Some("2026-09-01T00:00:00.000Z"));
}

// ---- sheet parsing + fetch ----

const CSV: &str = "\u{feff}Weapon,Positive Stats,Negative Stats\r\n\
Lex,CD CC/MS/TOX or MS DMG,ZOOM/REC\r\n\
\"Galatine\",\"CD AS RANGE\",\"EFF SLIDE\"\r\n\
,,\r\n\
Nothing,,\r\n";

#[test]
fn parses_the_sheet_csv() {
    let rows = parse_good_roll_csv(CSV, SheetClass::Primary).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "lex");
    assert_eq!(
        rows[0].data.good_attrs,
        vec![
            roll(
                &["WeaponCritDamageMod"],
                &[
                    "WeaponCritChanceMod",
                    "WeaponFireIterationsMod",
                    "WeaponToxinDamageMod"
                ]
            ),
            roll(&["WeaponFireIterationsMod", "WeaponDamageAmountMod"], &[]),
        ]
    );
    assert_eq!(
        rows[0].data.accepted_bad_attrs,
        ["WeaponZoomFovMod", "WeaponRecoilReductionMod"]
    );
    let melee =
        parse_good_roll_csv("Weapon,Positive,Negative\nX,DMG,\n", SheetClass::Melee).unwrap();
    assert_eq!(
        melee[0].data.good_attrs[0].mandatory,
        ["WeaponMeleeDamageMod"]
    );
    let elem = parse_good_roll_csv(
        "Weapon,Positive,Negative\nX,ELEMENT,\n",
        SheetClass::Primary,
    )
    .unwrap();
    assert_eq!(elem[0].data.good_attrs[0].mandatory.len(), 4);
}

#[test]
fn a_bad_header_is_an_error() {
    let err = parse_good_roll_csv("Name,Foo\nX,Y\n", SheetClass::Secondary).unwrap_err();
    assert!(err.contains("bad riven good-rolls header"), "{err}");
}

#[tokio::test]
async fn fetch_sheet_reads_every_tab_and_keeps_the_first_row() {
    use crate::services::net::{client, test_server};
    let (url, hits) = test_server::spawn("Weapon,Positive Stats,Negative Stats\nLex,CD,ZOOM\n");
    let base = url.trim_end_matches("/data");
    let fetched = super::best_attributes::fetch_sheet(&client().unwrap(), base)
        .await
        .unwrap();
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 5);
    assert!(fetched.get_by_name("lex").is_some());
    assert!(fetched.is_current());
}

#[tokio::test]
async fn refresh_persists_and_falls_back_to_the_cached_sheet() {
    use crate::services::net::{client, test_server};
    let db = crate::db::mem_db();
    let c = client().unwrap();
    // Nothing cached and nothing reachable: the error surfaces.
    assert!(
        super::best_attributes::refresh(&db, &c, "http://127.0.0.1:1")
            .await
            .is_err()
    );
    let (url, _) = test_server::spawn("Weapon,Positive Stats,Negative Stats\nLex,CD,ZOOM\n");
    super::best_attributes::refresh(&db, &c, url.trim_end_matches("/data"))
        .await
        .unwrap();
    // The server is gone for this base: the persisted copy answers.
    let cached = super::best_attributes::refresh(&db, &c, "http://127.0.0.1:1")
        .await
        .unwrap();
    assert!(cached.get_by_name("lex").is_some());
}

#[test]
fn good_rolls_info_reports_size_and_age() {
    let g = sheet(
        vec![("Soma", single("Critical Chance"))],
        Some("2026-10-20T00:00:00Z"),
    );
    let info = super::RivenGoodRollsInfo::of(&g);
    assert_eq!(info.weapons, 1);
    assert_eq!(info.updated_at.as_deref(), Some("2026-10-20T00:00:00Z"));
    assert_eq!(
        super::RivenGoodRollsInfo::of(&sheet(vec![], None)).updated_at,
        None
    );
}

/// Real cards from the 1.2-everywhere era: the bundled Staticor (0.9) and Orthos (1.0) dispositions
/// are stale, and every value sits inside the range the normalized disposition computes.
#[test]
fn cards_rolled_at_the_normalized_disposition_stay_inside_their_range() {
    real!(d);
    let staticor = [
        s("Electricity", true, 94.2),
        s("Multishot", true, 145.2),
        s("Critical Chance", true, 163.5),
        mult("Infested", false, 0.62),
    ];
    let orthos = [
        s("Attack Speed", true, 64.6),
        s("Critical Damage", true, 95.5),
        mult("Corpus", false, 0.61),
    ];
    for (weapon, stats) in [("Staticor", &staticor[..]), ("Orthos", &orthos[..])] {
        let r = grade(d, weapon, stats).unwrap();
        for st in r.stats.iter().filter(|x| x.min.is_some()) {
            let (min, max) = (st.min.unwrap(), st.max.unwrap());
            let v = st.value.unwrap();
            assert!(min <= v && v <= max, "{weapon} {st:?}");
            assert!(
                st.roll_float > 0.0 && st.roll_float < 1.0,
                "{weapon} {st:?}"
            );
        }
    }
    // 90 * 1.2 * 0.9375 (three buffs and a curse) * 0.9..1.1
    let r = grade(d, "Staticor", &staticor).unwrap();
    near(r.stats[0].min.unwrap(), 91.125, 0.01);
    near(r.stats[0].max.unwrap(), 111.375, 0.01);
}
