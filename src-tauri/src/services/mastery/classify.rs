//! Which catalog items are masterable, and under which category (port of masteryHelper's rules).

use std::sync::LazyLock;

use regex::Regex;

use crate::services::item_db::Item;

fn re(p: &str) -> Regex {
    Regex::new(&format!("(?i){p}")).unwrap()
}

/// Newer items arrive as "Warframes"/"Primary", older ones singular; accept both.
pub const MASTERABLE_DB_CATEGORIES: [&str; 10] = [
    "Warframe",
    "Weapon",
    "Companion",
    "Railjack",
    "Warframes",
    "Primary",
    "Secondary",
    "Melee",
    "Companions",
    "Archwing",
];

fn product_display(product: &str) -> Option<&'static str> {
    Some(match product {
        "Suits" => "Warframes",
        "LongGuns" => "Primary",
        "Pistols" => "Secondary",
        "Melee" => "Melee",
        "Sentinels" | "SentinelWeapons" => "Companions",
        "SpaceSuits" | "SpaceGuns" | "SpaceMelee" => "Archwing",
        "OperatorAmps" => "Amps",
        "MechSuits" => "Necramech",
        "CrewShipWeapons" => "Railjack",
        _ => return None,
    })
}

static PATH_RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"/OperatorAmps?/", "Amps"),
        (r"/OperatorAmplifiers?/", "Amps"),
        (r"/Sentinels/.*Weapons?/", "Companions"),
        (r"/Sentinels?/", "Companions"),
        (r"/Pets?/", "Companions"),
        (r"/SpaceSuits?/", "Archwing"),
        (r"/SpaceGuns?/", "Archwing"),
        (r"/SpaceMelee/", "Archwing"),
        (r"/MechSuits?/", "Necramech"),
        (r"/CrewShip.*Weapons?/", "Railjack"),
        (r"/Suits/", "Warframes"),
        (r"/ModularMelee\b|/Ostron.*Melee|/Zaw", "Melee"),
        (
            r"/ModularPistol|/SolarisUnited.*Secondary|/Kitgun.*Pistol",
            "Secondary",
        ),
        (
            r"/ModularPrimary|/SolarisUnited.*Primary|/Kitgun.*Rifle",
            "Primary",
        ),
        (r"/LongGuns/", "Primary"),
        (r"/Pistols/", "Secondary"),
        (r"/Melee/", "Melee"),
    ]
    .into_iter()
    .map(|(p, c)| (re(p), c))
    .collect()
});

static KEYWORD_RULES: LazyLock<Vec<(Regex, &'static [&'static str])>> = LazyLock::new(|| {
    let rules: [(&str, &[&str]); 16] = [
        (
            r"/ModularMelee\b|/Ostron.*Melee|/InfZaw|/Zaw",
            &["zaw", "modular"],
        ),
        (
            r"/ModularPistol|/ModularPrimary|/SolarisUnited.*(?:Secondary|Primary)|/Kitgun",
            &["kitgun", "modular"],
        ),
        (r"/OperatorAmps?/", &["amp", "operator"]),
        (r"/OperatorAmplifiers?/", &["amp", "operator"]),
        (r"/Hoverboard/", &["k-drive", "kdrive", "hoverboard"]),
        (r"/MechSuits?/", &["necramech", "mech"]),
        (r"/Archwing|/SpaceSuits?/", &["archwing"]),
        (r"/SpaceGuns?/", &["archgun", "arch-gun"]),
        (r"/SpaceMelee/", &["archmelee", "arch-melee"]),
        (r"/CrewShip", &["railjack"]),
        (r"/Sentinels?/", &["sentinel", "companion"]),
        (r"/Pets?/", &["companion", "pet"]),
        (r"Prime", &["prime"]),
        (r"Wraith", &["wraith"]),
        (r"Vandal", &["vandal"]),
        (r"Prisma", &["prisma"]),
    ];
    let mut v: Vec<_> = rules.into_iter().map(|(p, k)| (re(p), k)).collect();
    v.push((re("Kuva"), &["kuva", "lich"]));
    v.push((re("Tenet"), &["tenet", "sister"]));
    v.push((re("Incarnon"), &["incarnon"]));
    v
});

pub fn keywords(unique_name: &str, item_name: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for (pattern, kws) in KEYWORD_RULES.iter() {
        if pattern.is_match(unique_name) || pattern.is_match(item_name) {
            for kw in *kws {
                if !tags.iter().any(|t| t == kw) {
                    tags.push((*kw).to_owned());
                }
            }
        }
    }
    tags
}

/// Exalted weapons to exclude even if the catalog does not flag them.
const EXALTED_NAMES: [&str; 17] = [
    "regulators",
    "regulators prime",
    "iron staff",
    "iron staff prime",
    "exalted blade",
    "exalted blade prime",
    "dex pixia",
    "dex pixia prime",
    "diwata",
    "diwata prime",
    "artemis bow",
    "artemis bow prime",
    "valkyr talons",
    "valkyr talons prime",
    "desert wind",
    "desert wind prime",
    "shattered lash",
];

const KITGUN_CHAMBER_NAMES: [&str; 6] = [
    "catchmoon",
    "gaze",
    "rattleguts",
    "sporelacer",
    "tombfinger",
    "vermisplicer",
];

pub const VINQUIBUS_PRIMARY: &str = "/Lotus/Weapons/Tenno/Bayonet/TnBayonetRifleWeapon";
pub const VINQUIBUS_MELEE: &str = "/Lotus/Weapons/Tenno/Bayonet/TnBayonetMeleeWeapon";

static ZAW_STRIKE: LazyLock<Regex> = LazyLock::new(|| re(r"/Ostron/Melee/ModularMelee"));
static VENARI: LazyLock<Regex> =
    LazyLock::new(|| re(r"/Powersuits/Khora/Kavat/Khora(?:Prime)?KavatPowerSuit$"));
static TRAINING_AMP: LazyLock<Regex> =
    LazyLock::new(|| re(r"/SentTrainingAmps?/|/SentTrainingAmplifiers?/"));
static MODULAR_PET_CHASSIS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"/Pets/(?:MoaPets/MoaPet|ZanukaPets/ZanukaPet[ABC])PowerSuit$").unwrap()
});
static OPERATOR_AMPLIFIER: LazyLock<Regex> = LazyLock::new(|| re(r"/OperatorAmplifiers?/"));
static AMP_BARREL: LazyLock<Regex> = LazyLock::new(|| re(r"/Barrel/|Barrel$"));
static KITGUN_BARREL: LazyLock<Regex> = LazyLock::new(|| re(r"/(?:Barrel|Barrels)/"));
static HOVERBOARD: LazyLock<Regex> = LazyLock::new(|| re(r"/Hoverboard/"));
static K_DRIVE: LazyLock<Regex> = LazyLock::new(|| re(r"k-drive"));
static PET_PATH: LazyLock<Regex> = LazyLock::new(|| re(r"/Pets?/"));
static PET_TYPE: LazyLock<Regex> = LazyLock::new(|| re(r"\bpets?\b"));
static EXALTED_TYPE: LazyLock<Regex> = LazyLock::new(|| re("exalted"));

/// Hidden path rules: (pattern, reason). Order matches WFHelper's checks.
static PATH_EXCLUDES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"/ExaltedWeapons?/", "path-exaltedweapons"),
        (r"/SpecialItems/", "path-specialitems"),
        (r"/Cosmetics?/", "cosmetic"),
        (r"/Decorations?/", "decoration"),
        (r"/NPC/", "npc"),
        (r"/Test/", "test"),
        (r"/Developers?/", "developer"),
        (r"/FixedGun", "fixed-gun"),
    ]
    .into_iter()
    .map(|(p, r)| (re(p), r))
    .collect()
});

pub fn is_venari(unique_name: &str) -> bool {
    VENARI.is_match(unique_name)
}

/// The Mote Prism is an amp barrel in its own right; scaffolds and braces grant nothing.
pub fn is_amp_prism(name: &str, unique_name: &str) -> bool {
    OPERATOR_AMPLIFIER.is_match(unique_name)
        && AMP_BARREL.is_match(unique_name)
        && name.to_lowercase().contains(" prism")
}

pub fn is_kitgun_chamber(name: &str, unique_name: &str) -> bool {
    KITGUN_BARREL.is_match(unique_name)
        && KITGUN_CHAMBER_NAMES.contains(&name.trim().to_lowercase().as_str())
}

pub fn exclude_reason(unique_name: &str, name: &str, item: &Item) -> Option<&'static str> {
    if unique_name == VINQUIBUS_MELEE {
        return Some("shared-mastery-variant");
    }
    for (needle, reason) in [
        ("/Recipes/", "recipe"),
        ("/StoreItems/", "store-item"),
        ("/OperatorLoadOuts/", "operator-loadout"),
        ("/QuestVersions/", "quest-version"),
        ("/PrototypeVersions/", "prototype-version"),
    ] {
        if unique_name.contains(needle) {
            return Some(reason);
        }
    }
    // Only explicitly flagged exalted weapons; WFCD links can be incidental.
    if item.exalted {
        return Some("wfcd-exalted-flag");
    }
    if item.product_category.as_deref() == Some("SpecialItems") && !is_venari(unique_name) {
        return Some("specialitems-product-category");
    }
    if EXALTED_TYPE.is_match(&item.kind) {
        return Some("type-exalted");
    }
    // path-exaltedweapons / path-specialitems come before the name list, the rest after.
    let (early, late) = PATH_EXCLUDES.split_at(2);
    if let Some((_, r)) = early.iter().find(|(p, _)| p.is_match(unique_name)) {
        return Some(r);
    }
    if EXALTED_NAMES.contains(&name.to_lowercase().as_str()) {
        return Some("name-exalted-list");
    }
    if let Some((_, r)) = late.iter().find(|(p, _)| p.is_match(unique_name)) {
        return Some(r);
    }
    if TRAINING_AMP.is_match(unique_name) {
        return if is_amp_prism(name, unique_name) {
            None
        } else {
            Some("training-amp")
        };
    }
    // Modular pet chassis: mastery tracks the model head, never the power suit.
    if MODULAR_PET_CHASSIS.is_match(unique_name) {
        return Some("modular-pet-chassis");
    }
    let n = name.to_lowercase();
    if n.ends_with(" blueprint") || n.ends_with(" component") {
        return Some("name-blueprint-component");
    }
    None
}

pub struct DisplayCategory {
    pub category: &'static str,
    pub source: String,
}

fn cat(category: &'static str, source: impl Into<String>) -> DisplayCategory {
    DisplayCategory {
        category,
        source: source.into(),
    }
}

pub fn display_category(item: &Item, unique_name: &str) -> DisplayCategory {
    // K-Drive boards are exported with Weapon/Pistols metadata; keep them in Misc.
    if HOVERBOARD.is_match(unique_name) || K_DRIVE.is_match(&item.kind) {
        return cat("Misc", "override:k-drive");
    }
    if PET_PATH.is_match(unique_name) || PET_TYPE.is_match(&item.kind) {
        return cat("Companions", "override:pets");
    }
    if OPERATOR_AMPLIFIER.is_match(unique_name) {
        return cat("Amps", "path:OperatorAmplifiers");
    }
    if ZAW_STRIKE.is_match(unique_name) {
        return cat("Melee", "override:zaw-strike");
    }
    if let Some(p) = item.product_category.as_deref()
        && let Some(c) = product_display(p)
    {
        return cat(c, format!("productCategory:{p}"));
    }
    for (pattern, category) in PATH_RULES.iter() {
        if pattern.is_match(unique_name) {
            // WFHelper prints the JS regex literal; the debug string is for humans only.
            return cat(
                category,
                format!("path:/{}/i", pattern.as_str().trim_start_matches("(?i)")),
            );
        }
    }
    match item.category.as_str() {
        "Warframe" => cat("Warframes", "db-category:Warframe"),
        "Companion" => cat("Companions", "db-category:Companion"),
        "Railjack" => cat("Railjack", "db-category:Railjack"),
        _ => cat("Other", "fallback:Other"),
    }
}
