//! Riven vocabulary: stat names, upgrade tags and the tags warframe.market keys auctions on.

pub const NUM_BUFFS_ATTEN: [f64; 6] = [0.0, 1.0, 0.66000003, 0.5, 0.40000001, 0.34999999];
/// Curse attenuation indexed by number of buffs (not curses).
pub const NUM_BUFFS_CURSE_ATTEN: [f64; 6] = [0.0, 1.0, 0.33000001, 0.5, 1.25, 1.5];
pub const SPECIFIC_FIT_ATTEN: f64 = 1.5;
pub const BASE_DRAIN: f64 = 10.0;
/// Every weapon's riven disposition is normalized to 1.2 (four dots); the bundled per-weapon
/// values predate that, so a card that fits only at 1.2 is graded at it.
pub const NORMALIZED_DISPOSITION: f64 = 1.2;

const NON_PERCENTAGE_TAGS: &[&str] = &[
    "WeaponFactionDamageGrineer",
    "WeaponFactionDamageCorpus",
    "WeaponFactionDamageInfested",
    "WeaponMeleeFactionDamageGrineer",
    "WeaponMeleeFactionDamageCorpus",
    "WeaponMeleeFactionDamageInfested",
    "WeaponMeleeComboInitialBonusMod",
    "ComboDurationMod",
    "WeaponMeleeRangeIncMod",
    // Metres, like melee range: the card reads "+3.7 Punch Through", no percent.
    "WeaponPunctureDepthMod",
];

pub fn is_non_percentage(tag: &str) -> bool {
    NON_PERCENTAGE_TAGS.contains(&tag)
}

/// Card stat label (lowercase) to upgrade tag. The first label per tag is its canonical name.
const STAT_NAME_TO_TAG: &[(&str, &str)] = &[
    ("critical chance", "WeaponCritChanceMod"),
    ("critical damage", "WeaponCritDamageMod"),
    ("multishot", "WeaponFireIterationsMod"),
    ("fire rate", "WeaponFireRateMod"),
    ("damage", "WeaponDamageAmountMod"),
    ("reload speed", "WeaponReloadSpeedMod"),
    ("status chance", "WeaponStunChanceMod"),
    ("status duration", "WeaponProcTimeMod"),
    ("punch through", "WeaponPunctureDepthMod"),
    ("magazine capacity", "WeaponClipMaxMod"),
    ("ammo maximum", "WeaponAmmoMaxMod"),
    ("weapon recoil", "WeaponRecoilReductionMod"),
    ("recoil", "WeaponRecoilReductionMod"),
    ("zoom", "WeaponZoomFovMod"),
    ("projectile speed", "WeaponProjectileSpeedMod"),
    ("impact", "WeaponImpactDamageMod"),
    ("puncture", "WeaponArmorPiercingDamageMod"),
    ("slash", "WeaponSlashDamageMod"),
    ("cold", "WeaponFreezeDamageMod"),
    ("heat", "WeaponFireDamageMod"),
    ("electricity", "WeaponElectricityDamageMod"),
    ("toxin", "WeaponToxinDamageMod"),
    ("damage to grineer", "WeaponFactionDamageGrineer"),
    ("damage to corpus", "WeaponFactionDamageCorpus"),
    ("damage to infested", "WeaponFactionDamageInfested"),
    ("melee damage", "WeaponMeleeDamageMod"),
    ("attack speed", "WeaponFireRateMod"),
    ("range", "WeaponMeleeRangeIncMod"),
    ("combo duration", "ComboDurationMod"),
    (
        "critical chance for slide attack",
        "SlideAttackCritChanceMod",
    ),
    ("slide attack", "SlideAttackCritChanceMod"),
    ("finisher damage", "WeaponMeleeFinisherDamageMod"),
    ("heavy attack efficiency", "WeaponMeleeComboEfficiencyMod"),
    ("initial combo", "WeaponMeleeComboInitialBonusMod"),
    (
        "chance to gain combo count",
        "WeaponMeleeComboPointsOnHitMod",
    ),
    (
        "additional combo count chance",
        "WeaponMeleeComboBonusOnHitMod",
    ),
];

pub fn stat_name_to_tag(name: &str) -> Option<&'static str> {
    let lc = name.to_lowercase();
    let lc = lc.trim();
    STAT_NAME_TO_TAG
        .iter()
        .find(|(n, _)| *n == lc)
        .map(|(_, t)| *t)
}

/// Title-cased first label of a tag in `STAT_NAME_TO_TAG`.
pub fn canonical_tag_name(tag: &str) -> Option<String> {
    let (name, _) = STAT_NAME_TO_TAG.iter().find(|(_, t)| *t == tag)?;
    Some(
        name.split(' ')
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().chain(c).collect::<String>())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Good-roll list label. Attack speed shares the fire-rate tag, so melee callers opt in.
pub fn stat_tag_display_name(tag: &str, melee: bool) -> String {
    if melee && tag == "WeaponFireRateMod" {
        return "Attack Speed".into();
    }
    match tag {
        "WeaponCritChanceMod" => "Critical Chance",
        "WeaponCritDamageMod" => "Critical Damage",
        "WeaponFireIterationsMod" => "Multishot",
        "WeaponFireRateMod" => "Fire Rate",
        "WeaponDamageAmountMod" => "Damage",
        "WeaponMeleeDamageMod" => "Melee Damage",
        "WeaponReloadSpeedMod" => "Reload Speed",
        "WeaponStunChanceMod" => "Status Chance",
        "WeaponProcTimeMod" => "Status Duration",
        "WeaponPunctureDepthMod" => "Punch Through",
        "WeaponClipMaxMod" => "Magazine Capacity",
        "WeaponAmmoMaxMod" => "Ammo Maximum",
        "WeaponRecoilReductionMod" => "Weapon Recoil",
        "WeaponZoomFovMod" => "Zoom",
        "WeaponProjectileSpeedMod" => "Projectile Speed",
        "WeaponImpactDamageMod" => "Impact",
        "WeaponArmorPiercingDamageMod" => "Puncture",
        "WeaponSlashDamageMod" => "Slash",
        "WeaponFreezeDamageMod" => "Cold",
        "WeaponFireDamageMod" => "Heat",
        "WeaponElectricityDamageMod" => "Electricity",
        "WeaponToxinDamageMod" => "Toxin",
        "WeaponFactionDamageGrineer" => "Damage to Grineer",
        "WeaponFactionDamageCorpus" => "Damage to Corpus",
        "WeaponFactionDamageInfested" => "Damage to Infested",
        "WeaponMeleeRangeIncMod" => "Range",
        "ComboDurationMod" => "Combo Duration",
        "SlideAttackCritChanceMod" => "Slide Attack",
        "WeaponMeleeFinisherDamageMod" => "Finisher Damage",
        "WeaponMeleeComboEfficiencyMod" => "Heavy Attack Efficiency",
        "WeaponMeleeComboInitialBonusMod" => "Initial Combo",
        "WeaponMeleeComboPointsOnHitMod" => "Chance to Gain Combo Count",
        "WeaponMeleeComboBonusOnHitMod" => "Additional Combo Count Chance",
        other => other,
    }
    .into()
}

/// Game upgrade tag to the warframe.market auction attribute `url_name`.
const TAG_TO_WFM_URL_NAME: &[(&str, &str)] = &[
    ("WeaponCritChanceMod", "critical_chance"),
    ("WeaponCritDamageMod", "critical_damage"),
    ("WeaponFireIterationsMod", "multishot"),
    ("WeaponFireRateMod", "fire_rate_/_attack_speed"),
    ("WeaponDamageAmountMod", "base_damage_/_melee_damage"),
    ("WeaponReloadSpeedMod", "reload_speed"),
    ("WeaponStunChanceMod", "status_chance"),
    ("WeaponProcTimeMod", "status_duration"),
    ("WeaponPunctureDepthMod", "punch_through"),
    ("WeaponClipMaxMod", "magazine_capacity"),
    ("WeaponAmmoMaxMod", "ammo_maximum"),
    ("WeaponRecoilReductionMod", "recoil"),
    ("WeaponZoomFovMod", "zoom"),
    ("WeaponProjectileSpeedMod", "projectile_speed"),
    ("WeaponImpactDamageMod", "impact_damage"),
    ("WeaponArmorPiercingDamageMod", "puncture_damage"),
    ("WeaponSlashDamageMod", "slash_damage"),
    ("WeaponFreezeDamageMod", "cold_damage"),
    ("WeaponFireDamageMod", "heat_damage"),
    ("WeaponElectricityDamageMod", "electric_damage"),
    ("WeaponToxinDamageMod", "toxin_damage"),
    ("WeaponFactionDamageGrineer", "damage_vs_grineer"),
    ("WeaponFactionDamageCorpus", "damage_vs_corpus"),
    ("WeaponFactionDamageInfested", "damage_vs_infested"),
    ("WeaponMeleeDamageMod", "base_damage_/_melee_damage"),
    ("WeaponMeleeRangeIncMod", "range"),
    ("ComboDurationMod", "combo_duration"),
    (
        "SlideAttackCritChanceMod",
        "critical_chance_on_slide_attack",
    ),
    ("WeaponMeleeFinisherDamageMod", "finisher_damage"),
    ("WeaponMeleeComboEfficiencyMod", "channeling_efficiency"),
    ("WeaponMeleeComboInitialBonusMod", "channeling_damage"),
    (
        "WeaponMeleeComboPointsOnHitMod",
        "chance_to_gain_combo_count",
    ),
    (
        "WeaponMeleeComboBonusOnHitMod",
        "chance_to_gain_extra_combo_count",
    ),
    ("WeaponMeleeFactionDamageGrineer", "damage_vs_grineer"),
    ("WeaponMeleeFactionDamageCorpus", "damage_vs_corpus"),
    ("WeaponMeleeFactionDamageInfested", "damage_vs_infested"),
];

pub fn tag_to_wfm_url_name(tag: &str) -> Option<&'static str> {
    TAG_TO_WFM_URL_NAME
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, u)| *u)
}

/// True for an attribute slug WFM's auction search knows (the closed set alert rules may name).
pub fn is_wfm_attribute(url_name: &str) -> bool {
    TAG_TO_WFM_URL_NAME.iter().any(|(_, u)| *u == url_name)
}
