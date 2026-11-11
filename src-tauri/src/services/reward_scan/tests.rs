use std::sync::Arc;

use super::era::*;
use super::fold::fold_accents;
use super::matcher::*;
use super::names::*;
use crate::services::relics::{RelicDatabase, RelicRewardItem, RewardLookup};

fn pool(names: &[&str]) -> Vec<PoolItem> {
    names.iter().map(|n| PoolItem::new(*n, None)).collect()
}

fn name_of(c: &Candidate) -> &str {
    c.item.as_ref().map_or("", |i| i.name.as_str())
}

fn best(read: &str, items: &[PoolItem]) -> (String, Mode, f64) {
    let c = match_single_reward_text(read, items);
    (name_of(&c).to_owned(), c.mode, c.confidence)
}

const ITEMS: [&str; 12] = [
    "Braton Prime Stock",
    "Trumna Prime Blueprint",
    "Forma Blueprint",
    "Caliban Prime Neuroptics Blueprint",
    "Nautilus Prime Systems",
    "Epitaph Prime Receiver",
    "Zephyr Prime Neuroptics Blueprint",
    "Wukong Prime Chassis Blueprint",
    "Rhino Prime Blueprint",
    "Trinity Prime Blueprint",
    "Limbo Prime Blueprint",
    "Saryn Prime Blueprint",
];

fn with(extra: &[&str]) -> Vec<PoolItem> {
    pool(&[extra, &ITEMS].concat())
}

#[test]
fn matches_fuzzy_reads() {
    let items = pool(&ITEMS);
    assert_eq!(best("Forma BlueDrint", &items).0, "Forma Blueprint");
    assert_eq!(
        best("E itaDh Prime Receiver", &items).0,
        "Epitaph Prime Receiver"
    );
    let ranked = rank_reward_candidates("Caliban Blueprint Naut", &items, 3);
    assert!(
        ranked
            .iter()
            .any(|c| name_of(c) == "Caliban Prime Neuroptics Blueprint")
    );
}

#[test]
fn a_counted_card_keeps_its_count() {
    let items = with(&["2X Forma Blueprint"]);
    assert_eq!(best("2X Forma Blueprint", &items).0, "2X Forma Blueprint");
    assert_eq!(best("2x Forma Blueprint", &items).0, "2X Forma Blueprint");
    assert_eq!(best("Forma Blueprint", &items).0, "Forma Blueprint");
}

#[test]
fn short_names_do_not_leak_out_of_long_titles() {
    let items = pool(&["Paris Prime Upper Limb", "Ris"]);
    let ranked = rank_reward_candidates("Paris Prime Upper Limb", &items, 3);
    assert_eq!(name_of(&ranked[0]), "Paris Prime Upper Limb");
    assert!(ranked.iter().all(|c| name_of(c) != "Ris"));

    let mods = [
        "Fass", "Jahu", "Khra", "Lohk", "Netra", "Ris", "Vome", "Xata", "Oull",
    ];
    let items = pool(&[&["Paris Prime Upper Limb"], &mods[..]].concat());
    for m in mods {
        let r = rank_reward_candidates(&m.to_uppercase(), &items, 3);
        assert_eq!((name_of(&r[0]), r[0].mode), (m, Mode::Exact));
    }
}

#[test]
fn unique_prefix_is_lifted_over_the_slot_gate() {
    let items = with(&[
        "Yareli Prime Chassis Blueprint",
        "Yareli Prime Blueprint",
        "Yareli Prime Systems Blueprint",
        "Yareli Prime Neuroptics Blueprint",
    ]);
    let r = rank_reward_candidates("Yareli Prime Chassis", &items, 4);
    assert_eq!(name_of(&r[0]), "Yareli Prime Chassis Blueprint");
    assert_eq!(r[0].mode, Mode::Substring);
    assert!(r[0].confidence >= 0.92);
}

#[test]
fn ambiguous_reads_stay_below_the_gate() {
    let braton = pool(&[
        "Braton Prime Stock",
        "Braton Prime Blueprint",
        "Braton Prime Receiver",
        "Braton Prime Barrel",
    ]);
    for c in rank_reward_candidates("Braton Prime", &braton, 4) {
        if c.mode == Mode::Substring {
            assert!(c.confidence < 0.92);
        }
    }
    for c in rank_reward_candidates("Yareli", &pool(&["Yareli Prime Blueprint"]), 3) {
        assert!(c.confidence < 0.92);
    }
    let two = pool(&[
        "Wukong Prime Chassis Blueprint",
        "Yareli Prime Chassis Blueprint",
    ]);
    for c in rank_reward_candidates("Prime Chassis Blueprint", &two, 4) {
        assert!(c.confidence < 0.92);
    }
    for c in rank_reward_candidates("Forma Blueprint Braton Prime Stock", &pool(&ITEMS), 4) {
        assert!(c.confidence < 0.92);
    }
    let hit = best(
        "Forma Blueprint Braton Prime Stock",
        &pool(&["Forma Blueprint", "Braton Prime Stock"]),
    );
    assert!(hit.2 < 0.92);
}

#[test]
fn lifts_noisy_but_unambiguous_reads() {
    let items = pool(&ITEMS);
    let r = rank_reward_candidates("2x Forma Blueprint", &items, 4);
    assert_eq!(name_of(&r[0]), "Forma Blueprint");
    assert!(r[0].confidence >= 0.92);
    let r = rank_reward_candidates("Braton Prime Stock Trumna", &items, 4);
    assert_eq!(name_of(&r[0]), "Braton Prime Stock");
    assert!(r[0].confidence >= 0.92);

    let items = with(&["Wukong Prime Chassis Blueprint", "Wukong Prime Blueprint"]);
    for read in ["Wukong Chassis Blueprint", "Wukon Chassis ålueorint"] {
        let r = rank_reward_candidates(read, &items, 4);
        assert_eq!(name_of(&r[0]), "Wukong Prime Chassis Blueprint", "{read}");
        assert!(r[0].confidence >= 0.92, "{read}");
    }
}

#[test]
fn corrects_common_misreads_via_token_aliases() {
    let items = pool(&ITEMS);
    for (read, want) in [
        ("Rhino Prlme BlueDrint", "Rhino Prime Blueprint"),
        (
            "Zephyr Prime Neurootics Blueprint",
            "Zephyr Prime Neuroptics Blueprint",
        ),
        ("Nautilus Prime Svstems", "Nautilus Prime Systems"),
        ("Trinlty Prime Blueprint", "Trinity Prime Blueprint"),
        (
            "Wukong Prime Chassls Blueprint",
            "Wukong Prime Chassis Blueprint",
        ),
    ] {
        assert_eq!(best(read, &items).0, want, "{read}");
    }
}

#[test]
fn short_pool_name_against_long_read() {
    let items = pool(&[
        "Khra",
        "Khora Prime Blueprint",
        "Khora Prime Chassis Blueprint",
    ]);
    for read in ["Khora Prime Bluepr", "Khora Prime Bluepr ll"] {
        let r = rank_reward_candidates(read, &items, 4);
        assert_eq!(name_of(&r[0]), "Khora Prime Blueprint", "{read}");
        assert!(r.iter().all(|c| name_of(c) != "Khra"), "{read}");
    }
    for read in ["Khra", "KHRA", "Khra "] {
        let h = best(read, &items);
        assert_eq!((h.0.as_str(), h.1), ("Khra", Mode::Exact), "{read}");
    }
    let fuzzy = best("Netro", &pool(&["Netra", "Khora Prime Blueprint"]));
    assert_eq!((fuzzy.0.as_str(), fuzzy.1), ("Netra", Mode::Fuzzy));
}

#[test]
fn destroyed_component_word_keeps_the_base_below_the_gate() {
    let sets = [
        ("Wukong", "Cha55i5", "Wukong Prime Chassis Blueprint"),
        ("Frost", "Neur optics", "Frost Prime Neuroptics Blueprint"),
    ];
    for (frame, part, want) in sets {
        let base = format!("{frame} Prime Blueprint");
        let items = pool(&[
            &base,
            &format!("{frame} Prime Chassis Blueprint"),
            &format!("{frame} Prime Neuroptics Blueprint"),
            &format!("{frame} Prime Systems Blueprint"),
        ]);
        for read in [
            format!("{frame} Prime {part} Blueprint"),
            format!("{frame} Prime {part} Blueprint ll"),
        ] {
            let r = rank_reward_candidates(&read, &items, 4);
            let base_conf = r
                .iter()
                .find(|c| name_of(c) == base)
                .map_or(0.0, |c| c.confidence);
            assert!(base_conf < 0.86, "{read}");
            assert_eq!(name_of(&r[0]), want, "{read}");
        }
    }
}

#[test]
fn noise_tokens_do_not_cost_confidence() {
    let items = pool(&[
        "Wukong Prime Blueprint",
        "Wukong Prime Chassis Blueprint",
        "Wukong Prime Neuroptics Blueprint",
        "Wukong Prime Systems Blueprint",
    ]);
    for read in [
        "Wukong Prime Systems Blueprin",
        "Wukong Prime Systems Blueprin ll",
        "Wukong Prime Systems Blueprin ll vv",
    ] {
        let h = best(read, &items);
        assert_eq!(h.0, "Wukong Prime Systems Blueprint", "{read}");
        assert!(h.2 >= 0.92, "{read}");
    }
}

#[test]
fn one_bad_character_does_not_make_the_matcher_more_confident() {
    let items = pool(&[
        "Titania Prime Blueprint",
        "Titania Prime Chassis Blueprint",
        "Titania Prime Neuroptics Blueprint",
        "Titania Prime Systems Blueprint",
    ]);
    let clean = best("Titania Prime", &items).2;
    let corrupted = best("Tltanla Prlme", &items).2;
    assert!(clean < 0.92);
    assert!(corrupted < 0.86 && corrupted < clean);
}

#[test]
fn quantity_pairs_follow_the_read() {
    let pair = pool(&["Forma Blueprint", "2X Forma Blueprint"]);
    let bare = best("Forma Blueprin", &pair);
    assert_eq!(bare.0, "Forma Blueprint");
    assert!(bare.2 >= 0.86);
    for read in [
        "2X Forma Blueprint",
        "2 X Forma Blueprint",
        "2x forma blueprint",
    ] {
        let h = best(read, &pair);
        assert_eq!(
            (h.0.as_str(), h.1),
            ("2X Forma Blueprint", Mode::Exact),
            "{read}"
        );
    }
    for read in [
        "2 XForma Blueprint",
        "2XForma Blueprint",
        "2 X Forma 81ueprint",
    ] {
        let h = best(read, &pair);
        assert_eq!(
            (h.0.as_str(), h.1),
            ("2X Forma Blueprint", Mode::Exact),
            "{read}"
        );
    }
    for read in ["Forma Blueprint", "Forma 81ueprint", "forma blueprint"] {
        assert_eq!(best(read, &pair).0, "Forma Blueprint", "{read}");
    }
    let kuva = pool(&["1200X Kuva", "Forma Blueprint"]);
    assert_eq!(best("1200 X Kuva", &kuva).0, "1200X Kuva");
    assert_eq!(best("1200X Kuv", &kuva).0, "1200X Kuva");
}

#[test]
fn merged_glyph_pairs_at_4k() {
    let items = with(&[
        "Caliban Prime Blueprint",
        "Caliban Prime Chassis Blueprint",
        "Caliban Prime Neuroptics Blueprint",
        "Ash Prime Chassis Blueprint",
        "Xaku Prime Blueprint",
        "Sarofang Prime Handle",
    ]);
    let h = best("Caliban Prime81ueprint", &items);
    assert_eq!(
        (h.0.as_str(), h.1),
        ("Caliban Prime Blueprint", Mode::Exact)
    );
    assert!(h.2 >= 0.98);
    let h = best("Caliban Primeülueprint", &items);
    assert_eq!(h.0, "Caliban Prime Blueprint");
    assert!(h.2 >= 0.92);
    for read in ["Caliban Prime81ueprint", "Caliban Primeülueprint"] {
        for c in rank_reward_candidates(read, &items, 4) {
            if name_of(&c) != "Caliban Prime Blueprint" {
                assert!(c.confidence < 0.92, "{read} -> {}", name_of(&c));
            }
        }
    }
    assert_eq!(
        best("Caliban Prime Chassis 81ueprint", &items).0,
        "Caliban Prime Chassis Blueprint"
    );
    let counted = pool(&["2X Forma Blueprint", "5X Forma Blueprint"]);
    assert_eq!(best("5X Forma Blueprint", &counted).0, "5X Forma Blueprint");
    for c in rank_reward_candidates("5X Forma Blueprint", &counted, 4) {
        if name_of(&c) != "5X Forma Blueprint" {
            assert!(c.confidence < 0.92);
        }
    }
}

#[test]
fn match_items_finds_whole_names_in_a_long_read() {
    let items = pool(&ITEMS);
    let r = match_items("Forma Blueprint Braton Prime Stock", 0.74, &items);
    let names: Vec<&str> = r.matches.iter().map(|m| m.item.name.as_str()).collect();
    assert_eq!(names, ["Forma Blueprint", "Braton Prime Stock"]);
    assert_eq!(r.exact_count, 2);
    assert!(match_items("", 0.74, &items).matches.is_empty());
}

// ---- eras ----

fn era(h: EraHit) -> Option<&'static str> {
    h.era
}

#[test]
fn the_all_tab_reads_in_every_supported_language() {
    for text in ["TOUT", "Tout", "ALLE", "TODO", "全部", "TOUT "] {
        assert_eq!(era(detect_from_filter_label(text)), Some("omnia"), "{text}");
    }
    // A word that merely contains one is not the tab.
    assert_eq!(era(detect_from_filter_label("TOUTES")), None);
}

#[test]
fn filter_label_eras() {
    assert_eq!(
        detect_from_filter_label("ALL"),
        EraHit {
            era: Some("omnia"),
            confidence: 1.0
        }
    );
    assert_eq!(era(detect_from_filter_label("all")), Some("omnia"));
    for text in ["AII", "A11", "ALI", "AIL", "A1L"] {
        let h = detect_from_filter_label(text);
        assert_eq!(h.era, Some("omnia"), "{text}");
        assert!(h.confidence >= 0.95, "{text}");
    }
    assert_eq!(
        detect_from_filter_label("LITH"),
        EraHit {
            era: Some("lith"),
            confidence: 1.0
        }
    );
    assert_eq!(era(detect_from_filter_label("REQUIEM")), Some("requiem"));
    assert_ne!(era(detect_from_filter_label("AXL")), Some("omnia"));
    assert_eq!(era(detect_from_filter_label("")), None);
    assert_eq!(
        era(detect_from_filter_label("VOID RELICS REFINEMENT")),
        None
    );
}

#[test]
fn era_ambiguity_counts_misread_forms() {
    for text in [
        "REQUIEM FISSURE GARUS LITK FISSURE KORO",
        "REQUIEM RELICS MES RELICS",
        "AXL RELIC REQUIEM RELIC",
    ] {
        assert_eq!(era(detect_from_band(text)), None, "{text}");
        assert_eq!(era(detect_from_tile_label(text)), None, "{text}");
    }
    assert_eq!(era(detect_from_band("LITK RELICS")), Some("lith"));
    assert_eq!(
        era(detect_from_band("REQUIEM FISSURE GARUS KUVA FORTRESS")),
        Some("requiem")
    );
    assert_eq!(
        era(detect_from_tile_label("Meso V6 Relic [Radiant]")),
        Some("meso")
    );
    assert_eq!(era(detect_from_tile_label("Axi A12 Relic")), Some("axi"));
}

// ---- #67: game-language aliases ----

fn reward(name: &str, display: Option<&str>) -> RelicRewardItem {
    RelicRewardItem {
        name: name.into(),
        display_name: display.map(Into::into),
        unique_name: None,
        url_name: None,
        rarity: "Common".into(),
        ducats: None,
        vaulted: false,
    }
}

fn french() -> Vec<RelicRewardItem> {
    vec![
        reward("Volt Prime Chassis Blueprint", Some("Volt Prime - Châssis")),
        reward(
            "Volt Prime Systems Blueprint",
            Some("Volt Prime - Systèmes"),
        ),
        reward("Forma Blueprint", Some("Forma (Schéma)")),
        reward("2X Forma Blueprint", Some("Forma (Schéma)")),
    ]
}

#[test]
fn french_card_resolves_to_the_english_name() {
    let items = build_pool(&french());
    // The English OCR model reads "Châssis" without its accent.
    let chassis = match_single_reward_text("Volt Prime - Chassis", &items);
    assert_eq!(chassis.mode, Mode::Exact);
    assert_eq!(
        chassis.item.unwrap().english(),
        "Volt Prime Chassis Blueprint"
    );

    let double = match_single_reward_text("2 X Forma (Schema)", &items);
    assert_eq!(double.item.unwrap().english(), "2X Forma Blueprint");
    let single = match_single_reward_text("Forma (Schema)", &items);
    assert_eq!(single.item.unwrap().english(), "Forma Blueprint");
}

#[test]
fn german_omits_the_count_and_the_alias_restores_it() {
    let items = build_pool(&[
        reward("Forma Blueprint", Some("Forma (Bauplan)")),
        reward("2X Forma Blueprint", Some("Forma (Bauplan)")),
        reward("Braton Prime Stock", Some("Braton Prime Schaft")),
    ]);
    assert!(items.iter().any(|p| p.name == "2X Forma (Bauplan)"));
    for (read, want) in [
        ("2 X Forma (Bauplan)", "2X Forma Blueprint"),
        ("Forma (Bauplan)", "Forma Blueprint"),
        ("Braton Prime Schaft", "Braton Prime Stock"),
        ("Braton Prime Stock", "Braton Prime Stock"),
    ] {
        let c = match_single_reward_text(read, &items);
        assert_eq!(c.item.unwrap().english(), want, "{read}");
        assert_eq!(c.mode, Mode::Exact, "{read}");
    }
}

#[test]
fn english_pool_has_no_aliases_and_same_name_is_not_aliased() {
    let items = build_pool(&[reward("Forma Blueprint", None), reward("Ash", Some("Ash"))]);
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|p| p.english_name.is_none()));
}

#[test]
fn accents_fold_but_unmapped_letters_do_not_split_words() {
    assert_eq!(
        fold_accents("Châssis Systèmes Schéma"),
        "Chassis Systemes Schema"
    );
    let items = pool(&["Straße Prime", "Smørrebrød Prime"]);
    // ß/ø have no decomposition: they drop out of the word instead of splitting it.
    assert_eq!(normalize_reward_text("Straße Prime"), "strae prime");
    assert_eq!(normalize_reward_text("Smørrebrød"), "smrrebrd");
    assert_eq!(best("Straße Prime", &items).1, Mode::Exact);
}

struct Lookup(String);

impl RewardLookup for Lookup {
    fn by_name_or_slug(&self, _: Option<&str>, _: Option<&str>) -> Option<(String, Option<u32>)> {
        None
    }
    fn ducats_of(&self, _: &str) -> Option<u32> {
        None
    }
    fn display_name(&self, _: &str, english: &str) -> Option<String> {
        (self.0 != "en").then(|| format!("{english} [{}]", self.0))
    }
}

fn database(locale: &str) -> Arc<RelicDatabase> {
    use serde_json::json;
    let rows = [json!({
        "uniqueName": "/Lotus/Types/Game/Projections/T1VoidProjectionLithA1",
        "name": "Lith A1 Intact", "vaulted": false,
        "rewards": [{"chance": 25.0, "rarity": "Common", "item": {"name": "Braton Prime Stock", "uniqueName": "/x/Stock"}}],
        "drops": [{"location": "Void/Mithra", "chance": 0.29}],
    })];
    Arc::new(crate::services::relics::build(
        &rows,
        &Lookup(locale.into()),
    ))
}

#[test]
fn pool_is_rebuilt_when_the_game_language_changes() {
    let cache = RewardPool::default();
    let (en, de) = (database("en"), database("de"));
    let first = cache.get(&en, &Lookup("en".into()));
    assert!(Arc::ptr_eq(&first, &cache.get(&en, &Lookup("en".into()))));
    assert!(first.iter().all(|p| p.english_name.is_none()));

    let second = cache.get(&de, &Lookup("de".into()));
    assert!(!Arc::ptr_eq(&first, &second));
    let c = match_single_reward_text("Braton Prime Stock [de]", &second);
    assert_eq!(c.item.unwrap().english(), "Braton Prime Stock");
}

fn fr_era_words() -> Option<EraWords> {
    let root = crate::services::bundled_data::test_root()?;
    let dict = crate::services::bundled_data::BundledData::new(root)
        .dict("fr")
        .expect("dict.fr.json in the bundle");
    Some(EraWords::build(|k| {
        dict.get(k).and_then(|v| v.as_str()).map(str::to_owned)
    }))
}

#[test]
fn french_era_words_read_like_english() {
    let Some(fr) = fr_era_words() else { return };
    // An English recogniser keeps or drops the accent; both fold to the era.
    for text in [
        "Relique Réquiem C1",
        "RELIQUE REQUIEM C1",
        "Relique Rèquiem C1",
    ] {
        let hit = detect_from_tile_label(&fr.to_english(text));
        assert_eq!(hit.era, Some("requiem"), "{text}");
        assert!(hit.confidence >= 1.0, "{text}: {hit:?}");
    }
    assert_eq!(
        detect_from_band(&fr.to_english("Relique Meso A7")).era,
        Some("meso")
    );
    assert_eq!(
        detect_from_filter_label(&fr.to_english("Omnia")).era,
        Some("omnia")
    );
    // "Relique" in a band is not an era, and two eras stay ambiguous.
    assert_eq!(
        detect_from_band(&fr.to_english("Relique Lith Neo")).era,
        None
    );
}

#[test]
fn cjk_era_words_match_without_word_gaps() {
    let zh = EraWords::build(|k| {
        [
            ("/Lotus/Language/Relics/Era_LITH", "古纪"),
            ("/Lotus/Language/Relics/Era_OMNI", "全能"),
            (
                "/Lotus/Language/Alerts/AlertRewardCategoryRelic",
                "虚空遗物",
            ),
        ]
        .iter()
        .find(|(key, _)| *key == k)
        .map(|(_, v)| (*v).to_owned())
    });
    let hit = detect_from_tile_label(&zh.to_english("古纪A1虚空遗物"));
    assert_eq!(hit.era, Some("lith"));
    assert_eq!(
        detect_from_filter_label(&zh.to_english("全能")).era,
        Some("omnia")
    );
    // English text is untouched apart from the case fold.
    assert_eq!(
        detect_from_tile_label(&zh.to_english("Lith A1 Relic")).era,
        Some("lith")
    );
}
