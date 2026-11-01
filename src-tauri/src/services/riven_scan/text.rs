//! Riven card OCR text -> stats (port of WFHelper's `rivenScanText`). The input is the lines the
//! recogniser read, joined by `\n`; the output feeds `rivens::grading` through [`RivenStat::to_input`].
//!
//! The repairs are lookahead-heavy regexes tuned on real field reads, so they run on `fancy-regex`.
//! `\d`, `\w` and `\b` are ASCII like in JS (see [`pat`]): OCR icon junk such as "Ô" must not count
//! as a word character. Indexes are byte offsets into ASCII-lowercased copies of the line, which
//! keeps them valid on the original (stat names are ASCII).

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use fancy_regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

use crate::services::reward_scan::fold::levenshtein;
use crate::services::rivens::grading::RivenStatInput;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RivenStat {
    pub name: String,
    pub positive: bool,
    /// The sign as printed, when it differs from `positive` (Recoil shows buffs with a minus).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_positive: Option<bool>,
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiplier: bool,
}

impl RivenStat {
    pub fn to_input(&self) -> RivenStatInput {
        RivenStatInput {
            name: self.name.clone(),
            positive: self.positive,
            display_positive: self.display_positive,
            value: self.value,
            multiplier: self.multiplier.then_some(true),
        }
    }
}

/// What the parse set aside, so the caller can tell a clean read from one that lost a line.
#[derive(Default, Debug)]
pub struct ParseDiagnostics {
    /// Preprocessed lines that carried a signed value but produced no stat.
    pub dropped_lines: Vec<String>,
    /// A name line (the riven title) sits above the first stat line.
    pub title_seen: Option<bool>,
}

const WORD: &str = "A-Za-z0-9_";

/// Rewrites JS-style `\d \w \W \b` (ASCII) into classes the Unicode-by-default engine reads the same.
fn pat(p: &str) -> String {
    let mut out = String::with_capacity(p.len() + 16);
    let mut in_class = false;
    let mut chars = p.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(n) = chars.next() else {
                    out.push('\\');
                    break;
                };
                match (n, in_class) {
                    ('d', false) => out.push_str("[0-9]"),
                    ('d', true) => out.push_str("0-9"),
                    ('w', false) => out.push_str(&format!("[{WORD}]")),
                    ('w', true) => out.push_str(WORD),
                    ('W', false) => out.push_str(&format!("[^{WORD}]")),
                    ('b', false) => out.push_str(&format!(
                        "(?:(?<![{WORD}])(?=[{WORD}])|(?<=[{WORD}])(?![{WORD}]))"
                    )),
                    _ => {
                        out.push('\\');
                        out.push(n);
                    }
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push(c);
            }
            ']' if in_class => {
                in_class = false;
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

fn rx(p: &str) -> Regex {
    Regex::new(&pat(p)).unwrap_or_else(|e| panic!("riven text pattern {p}: {e}"))
}

/// `$1`..`$9` in a JS-style replacement template.
fn expand(template: &str, caps: &Captures) -> String {
    let mut out = String::with_capacity(template.len() + 8);
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().and_then(|d| d.to_digit(10))) {
            ('$', Some(n)) => {
                chars.next();
                out.push_str(caps.get(n as usize).map_or("", |m| m.as_str()));
            }
            _ => out.push(c),
        }
    }
    out
}

/// `replace_all` with a closure; a regex runtime error (backtrack limit) leaves the text as is.
fn sub_with(re: &Regex, text: &str, f: impl Fn(&Captures) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in re.captures_iter(text) {
        let Ok(caps) = caps else {
            return text.to_owned();
        };
        let whole = caps.get(0).expect("group 0");
        out.push_str(&text[last..whole.start()]);
        out.push_str(&f(&caps));
        last = whole.end();
    }
    out.push_str(&text[last..]);
    out
}

fn is_match(re: &Regex, text: &str) -> bool {
    re.is_match(text).unwrap_or(false)
}

/// `$t = $t.replace(/pattern/g, template)`; the regex is built once per call site.
macro_rules! sub {
    ($t:ident, $p:expr, $r:expr) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| rx($p));
        $t = sub_with(&RE, &$t, |c| expand($r, c));
    }};
}

macro_rules! re {
    ($p:expr) => {{
        static RE: LazyLock<Regex> = LazyLock::new(|| rx($p));
        &*RE
    }};
}

/// Substring by byte range, empty when out of range or off a char boundary.
fn slice(s: &str, from: usize, to: usize) -> &str {
    s.get(from..to.min(s.len())).unwrap_or("")
}

static ALIASES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        ("(?i)Dannage", "Damage"),
        ("(?i)Darnage", "Damage"),
        ("(?i)Darnoge", "Damage"),
        (r"(?i)\bDamage\s+to\s+Corpu\b", "Damage to Corpus"),
        (r"(?i)\bDamage\s+to\s+Infeste\b", "Damage to Infested"),
        (r"(?i)Crit\s*ical", "Critical"),
        ("(?i)Cri tical", "Critical"),
        (r"(?i)Critica\b", "Critical"),
        (r"(?i)Multi\s*shot", "Multishot"),
        ("(?i)Sta tus", "Status"),
        ("(?i)Statuc", "Status"),
        ("(?i)Re load", "Reload"),
        ("(?i)Elec tricity", "Electricity"),
        ("(?i)Punc ture", "Puncture"),
        ("(?i)Pi[uo]ncture", "Puncture"),
        ("(?i)Puincture", "Puncture"),
        (r"(?i)\bReload\s+Spe[de]\b", "Reload Speed"),
        ("(?i)Maga zine", "Magazine"),
        ("(?i)Capaclty", "Capacity"),
        ("(?i)Maxinnunn", "Maximum"),
        ("(?i)Maximu[rn]", "Maximum"),
        ("(?i)Annnno", "Ammo"),
        ("(?i)Mel[ae]e", "Melee"),
        ("(?i)Fini sher", "Finisher"),
        ("(?i)Finlsher", "Finisher"),
        ("(?i)[>]?[lh]mpact", "Impact"),
        (r"(?i)\bG[Ll]ash\b", "Slash"),
        (r"(?i)\b\(Glash\b", "Slash"),
        (r"(?i)\bY\s*Puncture\b", "Puncture"),
        (r"(?i)\bA\s*Slash\b", "Slash"),
        (r"(?i)\bO\s*Cold\b", "Cold"),
        (r"(?i)\bO\s*Heat\b", "Heat"),
        (r"(?i)\bl\s*eat\b", "Heat"),
        (r"(?i)\bQ\s*Toxin\b", "Toxin"),
        (r"(?i)\bQ\s*Electricity\b", "Electricity"),
        // The unique tail recovers "Additional" when the recogniser garbles or drops it.
        (
            r"(?i)\b(?:[A-Za-z]{2,12}\s+)?Combo\s+Count\s+Chance\b",
            "Additional Combo Count Chance",
        ),
    ]
    .into_iter()
    .map(|(p, r)| (rx(p), r))
    .collect()
});

/// Longest names first within a shared start only matters for ties; order is the reference's.
const KNOWN_STATS: &[&str] = &[
    "Additional Combo Count Chance",
    "Chance to Gain Combo Count",
    "Critical Chance for Slide Attack",
    "Heavy Attack Efficiency",
    "Magazine Capacity",
    "Damage to Grineer",
    "Damage to Corpus",
    "Damage to Infested",
    "Critical Chance",
    "Critical Damage",
    "Finisher Damage",
    "Melee Damage",
    "Weapon Recoil",
    "Status Duration",
    "Status Chance",
    "Projectile Speed",
    "Reload Speed",
    "Attack Speed",
    "Flight Speed",
    "Fire Rate",
    "Punch Through",
    "Combo Duration",
    "Initial Combo",
    "Ammo Maximum",
    "Heavy Attack",
    "Channeling Damage",
    "Channeling Efficiency",
    "Multishot",
    "Electricity",
    "Corrosive",
    "Radiation",
    "Magnetic",
    "Cold",
    "Heat",
    "Toxin",
    "Viral",
    "Blast",
    "Gas",
    "Impact",
    "Puncture",
    "Slash",
    "Magazine",
    "Recoil",
    "Damage",
    "Range",
    "Slide",
    "Zoom",
];

static KNOWN_LOWER: LazyLock<Vec<String>> =
    LazyLock::new(|| KNOWN_STATS.iter().map(|s| s.to_ascii_lowercase()).collect());

const MAX_REASONABLE_VALUE: f64 = 500.0;

/// Recoil alone displays buffs with a minus sign, so its parsed polarity must flip.
const INVERTED_POLARITY_STATS: &[&str] = &["weapon recoil", "recoil"];

const DAMAGE_TYPES: &str = "Slash|Cold|Heat|Electricity|Toxin|Impact|Puncture|Radiation|Viral|Corrosive|Blast|Magnetic|Gas";

/// Combined damage types share one displayed value, so the second may inherit the first.
const DAMAGE_TYPE_STATS: &[&str] = &[
    "electricity",
    "corrosive",
    "radiation",
    "magnetic",
    "cold",
    "heat",
    "toxin",
    "viral",
    "blast",
    "gas",
    "impact",
    "puncture",
    "slash",
];

/// Words a wrapped stat or qualifier spills onto its own line ("Bows)", "Capacity").
static STAT_NAME_WORDS: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut set: HashSet<String> = KNOWN_LOWER
        .iter()
        .flat_map(|s| s.split(' ').map(str::to_owned))
        .collect();
    set.insert("bows".into());
    set.insert("attacks".into());
    set
});

/// Heads of wrapped names; no riven stat is called only this.
const WRAP_HEAD_NAMES: &[&str] = &["Magazine", "Heavy Attack"];

/// Stats whose name ends another stat's name, keyed by that shorter (lowercase) name.
static LONGER_BY_TAIL: LazyLock<HashMap<&'static str, Vec<&'static str>>> = LazyLock::new(|| {
    KNOWN_STATS
        .iter()
        .map(|tail| {
            let suffix = format!(" {}", tail.to_ascii_lowercase());
            let longer = KNOWN_STATS
                .iter()
                .copied()
                .filter(|s| s.to_ascii_lowercase().ends_with(&suffix))
                .collect();
            (*tail, longer)
        })
        .collect()
});

const DIGIT_LOOKALIKES: &[(char, char)] = &[
    ('g', '9'),
    ('q', '9'),
    ('l', '1'),
    ('I', '1'),
    ('O', '0'),
    ('S', '5'),
    ('B', '8'),
];

fn digit_lookalikes(s: &str) -> String {
    s.chars()
        .map(|c| {
            DIGIT_LOOKALIKES
                .iter()
                .find(|(from, _)| *from == c)
                .map_or(c, |(_, to)| *to)
        })
        .collect()
}

/// A number must not end on the digit before an unread glyph ("x1.4k" is not x1.4).
const NUMBER_END: &str = r"(?!\.?\d|[A-Za-z](?![A-Za-z]))";

static X_VALUE: LazyLock<Regex> =
    LazyLock::new(|| rx(&format!(r"(?i)x\s*(\d+\.?\d*){NUMBER_END}")));
static SIGNED_VALUE: LazyLock<Regex> =
    LazyLock::new(|| rx(&format!(r"[+\-–]\s*(\d+\.?\d*){NUMBER_END}")));

/// The padlock closing a trait-locked stat reads as one glued character ("Chancee").
static TRAIT_LOCK_TAIL: LazyLock<Regex> = LazyLock::new(|| {
    let mut ends: Vec<&str> = Vec::new();
    for stat in KNOWN_STATS {
        let last = stat.rsplit(' ').next().unwrap_or(stat);
        if !ends.contains(&last) {
            ends.push(last);
        }
    }
    rx(&format!(r"(?im)\b({})[^\s%)]$", ends.join("|")))
});

/// A signed or x value read with a digit as a letter ("x1,4g", "+l,7").
static VALUE_WITH_LOOKALIKE: LazyLock<Regex> = LazyLock::new(|| {
    rx(r"(?m)([+\-–]\s*|\bx\s*)((?:\d[\dgqlIOSB]*|[gqlIOSB](?=\.))(?:\.[\dgqlIOSB]+)?)(?=[\s%]|$)")
});

/// Colored stat icons make the recogniser split two-word names; rejoin before other repairs.
static WRAP_JOINS: LazyLock<Vec<(Regex, String)>> = LazyLock::new(|| {
    [
        (r"(?i)\bFinisher\s*\n+\s*(?=Damage\b)", "Finisher "),
        (r"(?i)\bMelee\s*\n+\s*(?=Damage\b)", "Melee "),
        (
            r"(?i)\bCritical?\s*\n+\s*(?=(?:Chance|Damage)\b)",
            "Critical ",
        ),
        (r"(?i)\bStatus\s*\n+\s*(?=(?:Chance|Duration)\b)", "Status "),
        (r"(?i)\bAttack\s*\n+\s*(?=Speed\b)", "Attack "),
        (r"(?i)\bReload\s*\n+\s*(?=Speed\b)", "Reload "),
        (r"(?i)\bFlight\s*\n+\s*(?=Speed\b)", "Flight "),
        (r"(?i)\bProjectile\s*\n+\s*(?=Speed\b)", "Projectile "),
        (r"(?i)\bFire\s*\n+\s*(?=Rate\b)", "Fire "),
        // The second line of "Additional Combo / Count Chance" can arrive as ". Count Chance".
        (
            r"(?i)\bCombo\s*\n+[^\w\n]*(?=(?:Duration|Count)\b)",
            "Combo ",
        ),
        (r"(?i)\bAdditional\s*\n+\s*(?=Combo\b)", "Additional "),
        (r"(?i)\bAmmo\s*\n+\s*(?=Maximum\b)", "Ammo "),
        (r"(?i)\bPunch\s*\n+\s*(?=Through\b)", "Punch "),
        (
            r"(?i)\bChanneling\s*\n+\s*(?=(?:Damage|Efficiency)\b)",
            "Channeling ",
        ),
        (r"(?i)\bWeapon\s*\n+\s*(?=Recoil\b)", "Weapon "),
        (r"(?i)\bHeavy\s*\n+\s*(?=Attack\b)", "Heavy "),
        (r"(?i)\bInitial\s*\n+\s*(?=Combo\b)", "Initial "),
        (r"(?i)\bMagazine\s*\n+\s*(?=Capacity\b)", "Magazine "),
        (
            r"(?i)\bDamage\s*\n+\s*(?=to\s+(?:Grineer|Corpus|Infested)\b)",
            "Damage ",
        ),
        (r"(?i)\bto\s*\n+\s*(?=(?:Grineer|Corpus|Infested)\b)", "to "),
    ]
    .into_iter()
    .map(|(p, r)| (rx(p), r.to_owned()))
    .collect()
});

fn preprocess(raw: &str) -> String {
    let mut text = TRAIT_LOCK_TAIL.replace_all(raw, "$1").into_owned();
    // The padlock leading a locked stat can read as a digit ("8 +99,5%"); glued to a
    // multiplier, "8x1,51" collapsed to "81.51" and the stat lost its value.
    sub!(text, r"(?im)^\d(?=x\s*\d)", "");

    for (re, rep) in WRAP_JOINS.iter() {
        text = sub_with(re, &text, |_| rep.clone());
    }
    sub!(text, r"0/0", "%");
    sub!(text, r"(?i)O/O", "%");
    sub!(text, r"o/o", "%");
    sub!(text, r"(\d)\s*Z\b", "$1%");
    // The x multiplier glyph can be read as a leading angle bracket:
    // "<1,32 Damage to Infeste" -> "x1.32 Damage to Infested".
    sub!(
        text,
        r"(?i)[<‹]\s*(\d+[.,]\d+)\s+(?=Damage\s+to\s+)",
        "x$1 "
    );
    sub!(text, r"(?i)\bx\s*O([.,]\d)", "x0$1");
    // x1 read as xl or xI, possibly with the glyphs separated.
    sub!(text, r"(?i)\bx\s+[lI1]\s*[,.]\s*(\d+)", "x1.$1");
    sub!(text, r"(?i)\bx\s+[lI1]\b", "x1");
    sub!(text, r"\bx[lI]([,.]?\d)", "x1$1");
    sub!(text, r"\bx[lI]\b", "x1");
    // Spaced decimal on a multiplier: "x1 , 44" or "x1 ,44" -> "x1.44".
    sub!(text, r"\bx(\d)\s*,\s*(\d+)", "x$1.$2");
    // Spaced decimal commas, before the general comma conversion.
    sub!(text, r"([+\-–]?\d+),\s+(\d+)\s*%", "$1.$2%");
    sub!(text, r",(\d)", ".$1");
    // Multiplier decimals split across a line boundary.
    sub!(text, r"(x\d+)\n(\.\d+)", "$1$2");
    sub!(text, r"\b(x\d+)\s+\.(\d+)", "$1.$2");
    // A trailing orphan decimal belongs to the preceding multiplier.
    sub!(
        text,
        r"\b(x\d+)(\s+(?:Damage\s+to\s+\w+|[A-Z][a-z]+))\n\.(\d+)",
        "$1.$3$2"
    );
    // "x1\n3 Damage" -> "x1.3 Damage".
    sub!(text, r"(x\d+)\n([1-9]\d?\s+(?:Damage|[A-Z]))", "$1.$2");
    // "+151 .4%" -> "+151.4%".
    sub!(text, r"(\d)\s+\.(\d)", "$1.$2");
    sub!(text, r"(\d)\s([1-9])\s*%", "$1.$2%");

    for _ in 0..5 {
        sub!(text, r"([+\-–]\s*\d+)\s+(\d)", "$1$2");
    }
    for _ in 0..5 {
        sub!(text, r"(\d)\s+(\d)", "$1$2");
    }

    // Combo Duration prints a seconds unit, so its S is that unit, not a 5.
    sub!(text, r"(\d)S(?=\s+Combo\s+Dur)", "$1s");
    text = sub_with(&VALUE_WITH_LOOKALIKE, &text, |c| {
        let (lead, value) = (&c[1], &c[2]);
        if value.chars().any(|ch| ch.is_ascii_digit()) {
            format!("{lead}{}", digit_lookalikes(value))
        } else {
            c[0].to_owned()
        }
    });
    sub!(text, r"(\d)[A-Za-z](\d)", "$1$2");
    for _ in 0..3 {
        sub!(text, r"(\d)\s+(\d)", "$1$2");
    }

    for (re, rep) in ALIASES.iter() {
        text = sub_with(re, &text, |_| (*rep).to_owned());
    }

    sub!(text, r"(?i)\(x\d+\s*(?:for\s*)?Heavy\s*Attack[a-z]*\)", "");
    // A clipped band cuts the wrapped qualifier to "(x2 for Hea)" + "Attacks)".
    sub!(
        text,
        r"(?i)\(\s*x\d+\s*fo[a-z]*\s*Hea[a-z]*\s*\)?(?:\s*Attack[a-z]*\s*\)?)?",
        ""
    );
    // Fire-rate qualifier "(x2 for Bows)": wrapped across lines with clipped letters.
    sub!(text, r"(?i)\(\s*x\d+\s*fo[a-z]*\s*Bows?\s*\)?", "");
    sub!(text, r"(?im)\(\s*x\d+\s*fo[a-z]*\s*$", "");
    sub!(text, r"\(\s*(\d+[.,]\d+)", "x$1");
    sub!(text, r"[*()\[\]{}|\\<>^~°©®™•→←↑↓↗↘►◄▸▾▲▼■□●○]+\s*", " ");
    sub!(text, r"(?i)\bx\d+\s*(?:for\s*)?Heavy\s*Attack[a-z]*\b", "");
    sub!(text, r"%\s+[A-Z0-9]\s+(?=[A-Z])", "% ");
    // An isolated uppercase letter (element-icon artifact) between sign and digits.
    sub!(text, r"([+\-–]\s*)[A-Z]\s*(\d)", "$1$2");
    // 1-2 letters of element-icon junk before the sign at line start.
    sub!(text, r"(?m)^[A-Za-z]{1,2}\s*(?=[+\-–]\s*\d)", "");
    text = sub_with(re_icon_digit(), &text, |_| String::new());
    text = sub_with(re_icon_letter(), &text, |_| String::new());
    text = sub_with(re_icon_glyphs(), &text, |_| String::new());
    sub!(
        text,
        r"(?i)Critical\s+Chance[^a-zA-Z]{0,20}for\s+Slide\s+Attack",
        "Critical Chance for Slide Attack"
    );
    sub!(text, r"(\d)s(?=\s|$)", "$1");
    sub!(text, r"\s+([+\-–]\d)", "\n$1");
    sub!(text, r"(?i)\s+(x\d)", "\n$1");
    text
}

fn re_icon_digit() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| rx(&format!(r#"(?i)[0-9'"`]\s*(?=(?:{DAMAGE_TYPES})\b)"#)));
    &RE
}

fn re_icon_letter() -> &'static Regex {
    static RE: LazyLock<Regex> =
        LazyLock::new(|| rx(&format!(r"\b[A-Z]\s+(?=(?:{DAMAGE_TYPES})\b)")));
    &RE
}

fn re_icon_glyphs() -> &'static Regex {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        rx(&format!(
            r"(?i)[^\w\s+.%\-x]{{1,3}}\s+(?=(?:{DAMAGE_TYPES})\b)"
        ))
    });
    &RE
}

fn sanitise_value(value: f64) -> f64 {
    if value > MAX_REASONABLE_VALUE && value.fract() == 0.0 && value >= 100.0 {
        let s = format!("{}", value as u64);
        let (head, last) = s.split_at(s.len() - 1);
        if let Ok(corrected) = format!("{head}.{last}").parse::<f64>() {
            return corrected;
        }
    }
    // Merged OCR strips can prepend a digit; strip it only when the result is plausible.
    if value > 1000.0 && value.fract() != 0.0 {
        let s = format!("{}", (value * 10.0).round() / 10.0);
        let (int_part, dec_part) = s.split_once('.').unwrap_or((&s, ""));
        if int_part.len() > 3 {
            let rest = &int_part[1..];
            let corrected = if dec_part.is_empty() {
                rest.to_owned()
            } else {
                format!("{rest}.{dec_part}")
            };
            if let Ok(c) = corrected.parse::<f64>()
                && c > 0.0
                && c <= MAX_REASONABLE_VALUE
            {
                return c;
            }
        }
    }
    value
}

struct Signed {
    positive: bool,
    value: Option<f64>,
    multiplier: bool,
}

fn last_group(re: &Regex, fragment: &str) -> Option<f64> {
    let mut last = None;
    for caps in re.captures_iter(fragment).flatten() {
        last = caps.get(1).and_then(|m| m.as_str().parse::<f64>().ok());
    }
    last
}

fn extract_sign_and_value(fragment: &str) -> Option<Signed> {
    let sign_re = re!(r"[+\-–](?=\s*\d)");
    let last_sign = sign_re
        .find_iter(fragment)
        .flatten()
        .last()
        .map(|m| m.as_str().to_owned());
    let positive = last_sign.as_deref().is_none_or(|s| s != "-" && s != "–");

    if let Some(parsed) = last_group(re!(r"(\d+\.?\d*)\s*%"), fragment) {
        return Some(Signed {
            positive,
            value: Some(sanitise_value(parsed)),
            multiplier: false,
        });
    }
    let x_multiplier = last_group(&X_VALUE, fragment);
    if let Some(parsed) = x_multiplier {
        return Some(Signed {
            positive: parsed >= 1.0,
            value: Some(parsed),
            multiplier: true,
        });
    }
    if let Some(parsed) = last_group(&SIGNED_VALUE, fragment) {
        return Some(Signed {
            positive,
            value: Some(sanitise_value(parsed)),
            multiplier: false,
        });
    }
    if last_sign.is_some() {
        return Some(Signed {
            positive,
            value: None,
            multiplier: false,
        });
    }
    None
}

fn extracted_value(fragment: &str) -> Option<Option<f64>> {
    extract_sign_and_value(fragment).map(|s| s.value)
}

/// A dropped line is worth reporting when it plainly carried a stat value.
fn looks_stat_like(line: &str) -> bool {
    is_match(re!(r"[+\-–]\s*\d"), line) || is_match(re!(r"(?i)\bx\s*\d"), line)
}

pub fn looks_like_whole_stat_line(line: &str) -> bool {
    if !looks_stat_like(line) {
        return false;
    }
    // "x2 for Hea Attacks" is a wrapped qualifier, not a stat of its own.
    if is_match(re!(r"(?i)\bx\d+\s*fo"), line) && !is_match(re!(r"[+\-–]\s*\d"), line) {
        return false;
    }
    // A value with no name is a glare-split piece of a line the read kept.
    line.chars().filter(char::is_ascii_alphabetic).count() >= 3
}

fn has_title_above_stats(cleaned: &str) -> bool {
    for line in cleaned.lines() {
        if looks_stat_like(line) {
            return false;
        }
        let lower = line.to_ascii_lowercase();
        let words = re!(r"[a-z]{4,}").find_iter(&lower).flatten();
        if words
            .into_iter()
            .any(|w| !STAT_NAME_WORDS.contains(w.as_str()))
        {
            return true;
        }
    }
    false
}

pub fn parse_riven_stats(
    text: &str,
    mut diagnostics: Option<&mut ParseDiagnostics>,
) -> Vec<RivenStat> {
    if text.is_empty() {
        return vec![];
    }
    let cleaned = preprocess(&text.replace("\r\n", "\n"));
    if let Some(d) = diagnostics.as_deref_mut() {
        d.title_seen = Some(has_title_above_stats(&cleaned));
    }
    let mut line_dropped = Vec::new();
    let line_results = parse_stats_from_lines(&cleaned, &mut line_dropped);
    if !line_results.is_empty() && line_results.iter().any(|s| s.value.is_some()) {
        if let Some(d) = diagnostics {
            d.dropped_lines.extend(line_dropped);
        }
        return line_results;
    }

    let blob = cleaned.replace('\n', " ");
    let mut blob_dropped = Vec::new();
    let blob_results = parse_stats_from_lines(&blob, &mut blob_dropped);
    let score = |stats: &[RivenStat]| -> u32 {
        stats
            .iter()
            .map(|s| if s.value.is_some() { 10 } else { 3 })
            .sum()
    };
    let use_blob = score(&blob_results) > score(&line_results);
    if let Some(d) = diagnostics {
        d.dropped_lines
            .extend(if use_blob { blob_dropped } else { line_dropped });
    }
    if use_blob { blob_results } else { line_results }
}

fn line_contains_known_stat(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    KNOWN_LOWER.iter().any(|s| lower.contains(s.as_str()))
}

/// Edit distance from the fragment to the closest prefix of the stat name.
fn prefix_distance(fragment: &str, stat: &str) -> usize {
    let stat: Vec<char> = stat.chars().collect();
    let mut prev: Vec<usize> = (0..=stat.len()).collect();
    for (i, fc) in fragment.chars().enumerate() {
        let mut cur = vec![i + 1];
        for j in 1..=stat.len() {
            let substitution = prev[j - 1] + usize::from(fc != stat[j - 1]);
            cur.push((prev[j] + 1).min(cur[j - 1] + 1).min(substitution));
        }
        prev = cur;
    }
    prev.into_iter().min().unwrap_or(0)
}

fn shortest_of_chain(mut candidates: Vec<&'static str>) -> Option<&'static str> {
    candidates.sort_by_key(|s| s.len());
    let shortest = candidates.first()?.to_ascii_lowercase();
    candidates
        .iter()
        .all(|s| s.to_ascii_lowercase().starts_with(&shortest))
        .then_some(candidates[0])
}

fn normalize_stat_fragment(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let trimmed = lower.trim_end_matches(|c: char| !c.is_ascii_lowercase());
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Completes crop-truncated stat names only when every candidate shares one prefix.
fn complete_truncated_stat_name(fragment: &str, allow_slip: bool) -> Option<&'static str> {
    let frag = normalize_stat_fragment(fragment);
    if frag.chars().count() < 5 {
        return None;
    }
    if let Some(i) = KNOWN_LOWER.iter().position(|s| *s == frag) {
        return Some(KNOWN_STATS[i]);
    }
    // A lone word that ends other stats ("Chance") lost its head, not its tail.
    if !frag.contains(' ') && KNOWN_LOWER.iter().any(|s| s.ends_with(&format!(" {frag}"))) {
        return None;
    }
    let exact: Vec<&'static str> = KNOWN_STATS
        .iter()
        .zip(KNOWN_LOWER.iter())
        .filter(|(_, lower)| lower.len() > frag.len() && lower.starts_with(&frag))
        .map(|(s, _)| *s)
        .collect();
    if !exact.is_empty() {
        return shortest_of_chain(exact);
    }
    // One slip on a long fragment: a half-clipped last glyph ("Status Char") or a lost
    // first letter ("tatus Chan").
    if !allow_slip || frag.chars().count() < 8 {
        return None;
    }
    shortest_of_chain(
        KNOWN_STATS
            .iter()
            .zip(KNOWN_LOWER.iter())
            .filter(|(_, lower)| prefix_distance(&frag, lower) <= 1)
            .map(|(s, _)| *s)
            .collect(),
    )
}

/// Only faction damage takes a multiplier, so its faction word alone names the stat.
fn faction_multiplier_stat(line: &str, name_part: &str) -> Option<String> {
    let faction = re!(r"(?i)\b(grineer|corpus|infested)\W*$")
        .captures(name_part)
        .ok()
        .flatten()?;
    if !extract_sign_and_value(line)?.multiplier {
        return None;
    }
    let word = faction[1].to_ascii_lowercase();
    let mut chars = word.chars();
    let head = chars.next()?.to_ascii_uppercase();
    Some(format!("Damage to {head}{}", chars.as_str()))
}

#[derive(Clone, Debug)]
struct Hit {
    stat: String,
    idx: usize,
}

enum Head {
    /// No head word to resolve.
    Absent,
    /// The head word names no longer stat.
    NoMatch,
    Found(Hit),
}

/// Resolves a garbled head word before a short stat ("C...ical Damage").
fn resolve_garbled_head(line: &str, hit: &Hit, prefix_start: usize) -> Head {
    let Some(longer) = LONGER_BY_TAIL
        .iter()
        .find(|(tail, _)| tail.eq_ignore_ascii_case(&hit.stat))
        .map(|(_, l)| l)
        .filter(|l| !l.is_empty())
    else {
        return Head::Absent;
    };
    let before = slice(line, prefix_start, hit.idx);
    let Some(head) = re!(r"([A-Za-z]{3,})[^A-Za-z\d]*$")
        .captures(before)
        .ok()
        .flatten()
    else {
        return Head::Absent;
    };
    let frag = head[1].to_ascii_lowercase();
    let hit_words = hit.stat.split(' ').count();
    let matches: Vec<&&str> = longer
        .iter()
        .filter(|stat| {
            let lower = stat.to_ascii_lowercase();
            let words: Vec<&str> = lower.split(' ').collect();
            let word = (words.len() as isize - hit_words as isize - 1)
                .try_into()
                .ok()
                .and_then(|i: usize| words.get(i).copied())
                .unwrap_or("");
            if word.ends_with(&frag) {
                return true;
            }
            let frag_len = frag.chars().count();
            let tail: String = word
                .chars()
                .skip(word.chars().count().saturating_sub(frag_len))
                .collect();
            frag_len >= 4 && levenshtein(&frag, &tail) <= 1
        })
        .collect();
    if matches.len() != 1 {
        return Head::NoMatch;
    }
    Head::Found(Hit {
        stat: (*matches[0]).to_owned(),
        idx: prefix_start + head.get(1).map_or(0, |m| m.start()),
    })
}

fn collapse_orphan_value_lines(lines: &[&str]) -> Vec<String> {
    let mut collapsed = Vec::new();
    // Queue orphan values so interleaved name noise cannot consume their matching stat.
    let mut pending: Vec<String> = Vec::new();

    for line in lines {
        let current = line.trim();
        if current.is_empty() {
            continue;
        }
        let extracted = extract_sign_and_value(current);
        let looks_value_only = extracted.as_ref().is_some_and(|e| e.value.is_some())
            && !line_contains_known_stat(current)
            // A trailing comma is allowed: "+62," is the integer part of a split "+62.2%".
            && is_match(re!(r"(?i)^[+\-–x\d\s.,% ]+$"), current)
            // Bare integers are edge UI artifacts, not stat values.
            && !is_match(re!(r"^\d{1,4}$"), current);
        if looks_value_only {
            pending.push(current.to_owned());
            continue;
        }
        if !pending.is_empty() && line_contains_known_stat(current) {
            // Do not prepend an orphan when this stat line already carries its own value.
            if extracted_value(current).flatten().is_none() {
                let prefix = pending.remove(0);
                collapsed.push(format!("{prefix} {current}").trim().to_owned());
            } else {
                collapsed.push(current.to_owned());
            }
        } else {
            collapsed.push(current.to_owned());
        }
    }
    // Leftover orphans stay visible to the blob-parse fallback.
    collapsed.extend(pending);
    collapsed
}

fn parse_stats_from_lines(text: &str, dropped: &mut Vec<String>) -> Vec<RivenStat> {
    let raw_lines: Vec<&str> = text.split('\n').collect();
    let lines = collapse_orphan_value_lines(&raw_lines);
    let mut results: Vec<RivenStat> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for raw_line in &lines {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let line_lower = line.to_ascii_lowercase();

        let mut hits: Vec<Hit> = KNOWN_STATS
            .iter()
            .zip(KNOWN_LOWER.iter())
            .filter_map(|(stat, lower)| {
                line_lower.find(lower.as_str()).map(|idx| Hit {
                    stat: (*stat).to_owned(),
                    idx,
                })
            })
            .collect();

        // Fuzzy-match signed lines to recover small OCR errors in known stat names.
        if hits.is_empty() {
            if is_match(re!(r"(?i)^[+\-–x×]"), line) {
                // Strip the sign+value prefix to isolate the stat name.
                let name_part = re!(r"^[+\-–x×]?[\d.,\s%]*")
                    .replace(line, "")
                    .trim()
                    .to_owned();
                // Right-truncation first: "+152.3% Critical Cha" is beyond the fuzzy budget but
                // is a clean prefix of "Critical Chance".
                let completed = faction_multiplier_stat(line, &name_part)
                    .or_else(|| complete_truncated_stat_name(&name_part, true).map(str::to_owned));
                if let Some(stat) = completed {
                    let head: String = name_part.to_ascii_lowercase().chars().take(4).collect();
                    if let Some(idx) = line_lower.find(&head) {
                        hits.push(Hit { stat, idx });
                    }
                }
                // The padlock or an element icon leaves glyph junk before the name (":Toxir").
                let fuzzy_name = re!(r"^[^A-Za-z]+")
                    .replace(&name_part, "")
                    .to_ascii_lowercase();
                if hits.is_empty() && fuzzy_name.chars().count() >= 3 {
                    // Two edits on a short word turn "+5pact" into Impact; keep them for long names.
                    let max_dist = if fuzzy_name.chars().count() >= 7 {
                        2
                    } else {
                        1
                    };
                    let mut best: Option<(&str, usize)> = None;
                    for (stat, lower) in KNOWN_STATS.iter().zip(KNOWN_LOWER.iter()) {
                        let window: String =
                            fuzzy_name.chars().take(lower.chars().count() + 2).collect();
                        let dist = levenshtein(&window, lower);
                        if dist <= max_dist && best.is_none_or(|(_, d)| dist < d) {
                            best = Some((stat, dist));
                        }
                    }
                    if let Some((stat, _)) = best
                        && let Some(idx) = line_lower.find(&name_part.to_ascii_lowercase())
                    {
                        hits.push(Hit {
                            stat: stat.to_owned(),
                            idx,
                        });
                    }
                }
            }
            if hits.is_empty() {
                if looks_stat_like(line) {
                    dropped.push(line.to_owned());
                }
                continue;
            }
        }

        hits.sort_by(|a, b| a.idx.cmp(&b.idx).then(b.stat.len().cmp(&a.stat.len())));
        let mut filtered: Vec<Hit> = Vec::new();
        let mut last_end: isize = -1;
        for hit in hits {
            if hit.idx as isize >= last_end {
                last_end = (hit.idx + hit.stat.len()) as isize;
                filtered.push(hit);
            }
        }

        // Extend short matches only when the cleaned tail is an unambiguous longer stat.
        for index in 0..filtered.len() {
            let hit = filtered[index].clone();
            let tail_end = filtered.get(index + 1).map_or(line.len(), |h| h.idx);
            let clean_tail = normalize_stat_fragment(slice(line, hit.idx, tail_end));
            if clean_tail.len() <= hit.stat.len() {
                continue;
            }
            // A lone junk letter past a whole name ("Critical Chance f") starts no longer stat;
            // a wrap head is no stat of its own, so one letter decides there.
            let extension = clean_tail
                .get(hit.stat.len()..)
                .unwrap_or("")
                .chars()
                .filter(char::is_ascii_lowercase)
                .count();
            if extension < 2 && !WRAP_HEAD_NAMES.contains(&hit.stat.as_str()) {
                continue;
            }
            let completed =
                complete_truncated_stat_name(&clean_tail, clean_tail.len() - hit.stat.len() > 3);
            if let Some(stat) = completed
                && stat
                    .to_ascii_lowercase()
                    .starts_with(&hit.stat.to_ascii_lowercase())
            {
                filtered[index] = Hit {
                    stat: stat.to_owned(),
                    idx: hit.idx,
                };
            }
        }

        // Base Damage and the four *-Damage stats differ, so a head word that names none of them
        // voids the hit, as does any letter between the value and a base Damage, which has no
        // icon; Recoil and Weapon Recoil are one stat.
        for index in (0..filtered.len()).rev() {
            let hit = filtered[index].clone();
            let prefix_start = if index > 0 {
                filtered[index - 1].idx + filtered[index - 1].stat.len()
            } else {
                0
            };
            match resolve_garbled_head(line, &hit, prefix_start) {
                Head::Found(found) => filtered[index] = found,
                head => {
                    let void = hit.stat == "Damage"
                        && (matches!(head, Head::NoMatch)
                            || is_match(
                                re!(r"[A-Za-z][^\d%]*$"),
                                slice(line, prefix_start, hit.idx),
                            ));
                    if void {
                        filtered.remove(index);
                    }
                }
            }
        }
        if filtered.is_empty() {
            if looks_stat_like(line) {
                dropped.push(line.to_owned());
            }
            continue;
        }

        for index in 0..filtered.len() {
            let Hit { stat, idx } = filtered[index].clone();
            let key = stat.to_ascii_lowercase();

            // Prefix/value come before the seen-check so a duplicate can be compared by value.
            let prefix_start = if index > 0 {
                filtered[index - 1].idx + filtered[index - 1].stat.len()
            } else {
                0
            };
            let prefix = slice(line, prefix_start, idx);
            let mut extracted = extract_sign_and_value(prefix);

            if extracted.as_ref().is_none_or(|e| e.value.is_none()) {
                let suffix_end = filtered.get(index + 1).map_or(line.len(), |h| h.idx);
                let suffix = slice(line, idx + stat.len(), suffix_end);
                if let Some(s) = extract_sign_and_value(suffix)
                    && s.value.is_some()
                {
                    extracted = Some(s);
                }
            }

            let positive = extracted.as_ref().is_none_or(|e| e.positive);
            let mut value = extracted.as_ref().and_then(|e| e.value);
            let mut effective_positive = positive;
            let display_positive = positive;
            let multiplier = extracted.as_ref().is_some_and(|e| e.multiplier);

            // Multipliers belong only to faction damage; elsewhere they are qualifier OCR junk.
            if multiplier && !is_match(re!(r"^Damage\b"), &stat) {
                continue;
            }

            if seen.contains(&key) {
                let existing_idx = value.and_then(|_| {
                    results
                        .iter()
                        .position(|r| r.name.eq_ignore_ascii_case(&key))
                });
                let existing_value = existing_idx.map(|i| results[i].value);
                // A riven title can spell a stat ("Hexa-toxinok"); its valueless hit must not
                // shadow the stat line, which keeps its own place in card order.
                if value.is_some() && existing_value == Some(None) {
                    results.remove(existing_idx.expect("existing index"));
                } else {
                    // Prefer a duplicate with decimal precision when its integer part still matches.
                    if let (Some(v), Some(Some(old)), Some(i)) =
                        (value, existing_value, existing_idx)
                        && old.fract() == 0.0
                        && v.fract() != 0.0
                        && v.floor() == old
                    {
                        results[i] = RivenStat {
                            name: stat.clone(),
                            positive: effective_positive,
                            display_positive: (display_positive != effective_positive)
                                .then_some(display_positive),
                            value,
                            multiplier,
                        };
                    }
                    continue;
                }
            }
            seen.insert(key.clone());

            // Carry values only across adjacent damage types; noisy signs mark separate OCR rows.
            let noisy_sign = is_match(re!(r"[+\-–]\s*\S"), prefix);
            if value.is_none()
                && index > 0
                && DAMAGE_TYPE_STATS.contains(&key.as_str())
                && !noisy_sign
                && let Some(prev) = results.last()
                && prev.value.is_some()
                && !prev.multiplier
                && DAMAGE_TYPE_STATS.contains(&prev.name.to_ascii_lowercase().as_str())
            {
                value = prev.value;
                effective_positive = prev.positive;
            }

            // Recoil displays buffs with a minus sign, opposite the parsed polarity.
            if INVERTED_POLARITY_STATS.contains(&key.as_str()) {
                effective_positive = !effective_positive;
            }

            results.push(RivenStat {
                name: stat,
                positive: effective_positive,
                display_positive: (display_positive != effective_positive)
                    .then_some(display_positive),
                value,
                multiplier,
            });
        }
    }
    results
}

fn count_exact_value_matches(scanned: &[RivenStat], known: &[RivenStat]) -> usize {
    scanned
        .iter()
        .filter(|stat| {
            let Some(m) = known
                .iter()
                .find(|k| k.name.eq_ignore_ascii_case(&stat.name))
            else {
                return false;
            };
            if stat.positive != m.positive {
                return false;
            }
            let (Some(a), Some(b)) = (stat.value, m.value) else {
                return false;
            };
            // Multipliers print two decimals: x1.21 is a different roll from x1.22.
            let tolerance = if stat.multiplier || m.multiplier {
                0.005
            } else {
                0.05
            };
            (a - b).abs() <= tolerance
        })
        .count()
}

/// A reroll never repeats exact stat values; two value-exact matches against a known card mean the
/// scan caught that card (mid-animation), not the new roll.
pub fn looks_like_stale_card_read(scanned: &[RivenStat], known_cards: &[Vec<RivenStat>]) -> bool {
    scanned.len() >= 2
        && known_cards
            .iter()
            .any(|card| !card.is_empty() && count_exact_value_matches(scanned, card) >= 2)
}

#[cfg(test)]
pub fn roll_rescan_reason(
    scanned: &[RivenStat],
    known_cards: &[Vec<RivenStat>],
) -> Option<&'static str> {
    if scanned.is_empty() {
        return Some("read nothing");
    }
    looks_like_stale_card_read(scanned, known_cards).then_some("matches a pre-roll card")
}
