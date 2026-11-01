//! Inverts browse.wf/calamity `RivenParser.js` display values back to roll floats and grades
//! them (port of WFHelper's `rivenGrading`).

use serde::{Deserialize, Serialize};

use super::best_attributes::{GoodRollData, GoodRolls};
use super::data::{RivenData, UpgradeEntry};
use super::vocab::{
    BASE_DRAIN, NORMALIZED_DISPOSITION, NUM_BUFFS_ATTEN, NUM_BUFFS_CURSE_ATTEN, SPECIFIC_FIT_ATTEN,
    is_non_percentage, stat_name_to_tag,
};

/// Rivens are rank 8 (lvl 0..8).
pub const MAX_RIVEN_MOD_RANK: i32 = 8;

/// `lerp(-10, 10, rollFloat)` thresholds, as in RivenParser.js.
const GRADE_THRESHOLDS: &[(f64, &str)] = &[
    (9.5, "S"),
    (7.5, "A+"),
    (5.5, "A"),
    (3.5, "A-"),
    (1.5, "B+"),
    (-1.5, "B"),
    (-3.5, "B-"),
    (-5.5, "C+"),
    (-7.5, "C"),
    (-9.5, "C-"),
];

/// Display values round to 0.1%, so a legitimate min/max roll can sit a hair outside [0,1].
const FIT_TOLERANCE: f64 = 0.02;
const CORRECTION_MISFIT_THRESHOLD: f64 = 0.1;
/// A riven card carries at most three buffs and one curse.
const MAX_RIVEN_BUFFS: usize = 3;
const MAX_RIVEN_CURSES: usize = 1;

/// A stat as read off a card. `value` is unsigned; `multiplier` marks "x1.05" style stats.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenStatInput {
    pub name: String,
    pub positive: bool,
    pub display_positive: Option<bool>,
    pub value: Option<f64>,
    pub multiplier: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenGradedStat {
    pub name: String,
    pub positive: bool,
    pub display_positive: Option<bool>,
    pub value: Option<f64>,
    pub multiplier: Option<bool>,
    pub grade: String,
    pub roll_float: f64,
    /// The card-unit value at roll 0 and roll 1 for this weapon, disposition, stat count and rank.
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenGradeResult {
    pub stats: Vec<RivenGradedStat>,
    pub overall_grade: String,
    /// Attribute-based quality: "Great" | "Good" | "OK" | "Bad" | "?".
    pub attribute_grade: String,
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn inverse_lerp(a: f64, b: f64, v: f64) -> f64 {
    if b == a { 0.0 } else { (v - a) / (b - a) }
}

pub fn float_to_grade(roll_float: f64, is_curse: bool) -> &'static str {
    let f = if is_curse {
        1.0 - roll_float
    } else {
        roll_float
    };
    let score = lerp(-10.0, 10.0, f);
    GRADE_THRESHOLDS
        .iter()
        .find(|(min, _)| score >= *min)
        .map_or("F", |(_, g)| g)
}

fn atten(table: &[f64; 6], n: usize) -> f64 {
    table[n.min(table.len() - 1)]
}

fn displayed_fraction(displayed: f64, tag: Option<&str>, signed: bool) -> f64 {
    let v = if signed { displayed } else { displayed.abs() };
    if tag.is_some_and(is_non_percentage) {
        v
    } else {
        v / 100.0
    }
}

/// Inverts the buff formula for percentage or raw special-stat values.
/// Forward: `base * (1.5*disp*10) * 1.25^curses * lerp(0.9,1.1,roll) * buffsAtten * (lvl+1)`.
pub fn unparse_buff(
    displayed: f64,
    base: f64,
    disposition: f64,
    num_buffs: usize,
    num_curses: usize,
    tag: Option<&str>,
    lvl: i32,
) -> f64 {
    let buffs_atten = atten(&NUM_BUFFS_ATTEN, num_buffs);
    let curse_atten = 1.25f64.powi(num_curses as i32);
    let attenuation = SPECIFIC_FIT_ATTEN * disposition * BASE_DRAIN;
    let mut value = displayed_fraction(displayed, tag, true);
    if base == 0.0 || attenuation == 0.0 || buffs_atten == 0.0 || curse_atten == 0.0 {
        return 0.5;
    }
    value /= f64::from(lvl + 1);
    value /= buffs_atten;
    value /= curse_atten;
    value /= attenuation;
    // OCR values are unsigned; abs(base) handles negative-base stats such as recoil.
    value /= base.abs();
    (value - 0.9) / 0.2
}

/// Inverts the curse formula after OCR has removed the displayed sign.
/// Forward: `-base * (1.5*disp*10) * lerp(0.9,1.1,roll) * curseAtten * buffsAtten * (lvl+1)`.
pub fn unparse_curse(
    displayed: f64,
    base: f64,
    disposition: f64,
    num_buffs: usize,
    num_curses: usize,
    tag: Option<&str>,
    lvl: i32,
) -> f64 {
    let attenuation = SPECIFIC_FIT_ATTEN * disposition * BASE_DRAIN;
    // Swapped indexing: the buffs table by curse count, the curse table by buff count.
    let curses_in_buff_table = atten(&NUM_BUFFS_ATTEN, num_curses);
    let buffs_in_curse_table = atten(&NUM_BUFFS_CURSE_ATTEN, num_buffs);
    let mut value = displayed_fraction(displayed, tag, false);
    if base == 0.0
        || attenuation == 0.0
        || curses_in_buff_table == 0.0
        || buffs_in_curse_table == 0.0
    {
        return 0.5;
    }
    value /= f64::from(lvl + 1);
    value /= curses_in_buff_table;
    value /= buffs_in_curse_table;
    value /= attenuation;
    value /= base.abs();
    (value - 0.9) / 0.2
}

/// Both counts scale every displayed value, so an OCR line the scan dropped reads the survivors
/// high; the scanned counts are a lower bound.
fn plausible_stat_counts(buffs: usize, curses: usize) -> Vec<(usize, usize)> {
    let mut counts = vec![];
    for b in buffs..=buffs.max(MAX_RIVEN_BUFFS) {
        for c in curses..=curses.max(MAX_RIVEN_CURSES) {
            counts.push((b, c));
        }
    }
    counts
}

/// Sibling tags an OCR-garbled stat name can be: "+190.2% Critical Damage" on a melee is
/// really Melee Damage.
fn confusion_siblings(tag: &str) -> &'static [&'static str] {
    match tag {
        "WeaponDamageAmountMod" => &["WeaponMeleeDamageMod", "WeaponCritDamageMod"],
        "WeaponMeleeDamageMod" => &["WeaponDamageAmountMod", "WeaponCritDamageMod"],
        "WeaponCritDamageMod" => &["WeaponMeleeDamageMod", "WeaponDamageAmountMod"],
        "WeaponCritChanceMod" => &["SlideAttackCritChanceMod", "WeaponStunChanceMod"],
        "SlideAttackCritChanceMod" => &["WeaponCritChanceMod"],
        "WeaponStunChanceMod" => &["WeaponCritChanceMod"],
        _ => &[],
    }
}

/// Riven type data lists both damage tags with identical bases; cards use one by class.
fn weapon_damage_tag(tag: &str, melee: bool) -> &str {
    match (melee, tag) {
        (true, "WeaponDamageAmountMod") => "WeaponMeleeDamageMod",
        (false, "WeaponMeleeDamageMod") => "WeaponDamageAmountMod",
        _ => tag,
    }
}

fn unparse(
    stat_positive: bool,
    displayed: f64,
    entry: &UpgradeEntry,
    (disp, buffs, curses, lvl): (f64, usize, usize, i32),
    tag: &str,
) -> f64 {
    if stat_positive {
        unparse_buff(
            displayed,
            entry.base_value,
            disp,
            buffs,
            curses,
            Some(tag),
            lvl,
        )
    } else {
        unparse_curse(
            displayed,
            entry.base_value,
            disp,
            buffs,
            curses,
            Some(tag),
            lvl,
        )
    }
}

/// Distance of a roll float outside [0, 1].
fn overshoot(f: f64) -> f64 {
    (f - 1.0).max(0.0) + (-f).max(0.0)
}

struct Measured {
    original: RivenStatInput,
    stat: RivenStatInput,
    renamed_from: Option<String>,
    tag: Option<String>,
    value: Option<f64>,
    misfits: bool,
}

/// Renames OCR-garbled stat names to the sibling whose range fits the value. Returns the stats
/// and how many were changed.
pub fn correct_scanned_stats(
    data: &RivenData,
    weapon_name: &str,
    stats: &[RivenStatInput],
) -> (Vec<RivenStatInput>, usize) {
    let base_disposition = data.weapon_disposition(weapon_name);
    let riven_type = data.resolve_riven_type(weapon_name);
    let (Some(base_disposition), Some(riven_type)) = (base_disposition, riven_type) else {
        return (stats.to_vec(), 0);
    };
    if stats.is_empty() {
        return (stats.to_vec(), 0);
    }

    let melee = data.is_melee_weapon(weapon_name);
    let counts = plausible_stat_counts(
        stats.iter().filter(|s| s.positive).count(),
        stats.iter().filter(|s| !s.positive).count(),
    );
    let dispositions: Vec<f64> = std::iter::once(base_disposition)
        .chain(data.family_variants(weapon_name).into_iter().map(|v| v.1))
        .chain([NORMALIZED_DISPOSITION])
        .collect();

    // None when the tag cannot roll on this weapon at all.
    let violation_for = |tag: &str, stat: &RivenStatInput, displayed: f64| -> Option<f64> {
        let entry = data.find_upgrade_entry(riven_type, tag)?;
        if if stat.positive {
            !entry.can_be_buff
        } else {
            !entry.can_be_curse
        } {
            return None;
        }
        let mut best = f64::INFINITY;
        for &disp in &dispositions {
            for &(buffs, curses) in &counts {
                // A chat-linked card shows its values at the mod's own rank, not max.
                for lvl in 0..=MAX_RIVEN_MOD_RANK {
                    let f = unparse(
                        stat.positive,
                        displayed,
                        entry,
                        (disp, buffs, curses, lvl),
                        tag,
                    );
                    best = best.min(overshoot(f));
                }
            }
        }
        Some(best)
    };

    let mut corrections = 0;
    let measured: Vec<Measured> = stats
        .iter()
        .map(|original| {
            let mut stat = original.clone();
            let mut tag = stat_name_to_tag(&original.name).map(str::to_owned);
            let mut renamed_from = None;
            if let Some(t) = &tag {
                let normalized = weapon_damage_tag(t, melee);
                if normalized != t {
                    renamed_from = Some(stat.name.clone());
                    stat.name = data.stat_display_name(normalized, melee);
                    tag = Some(normalized.to_owned());
                }
            }
            let value = stat.value.filter(|v| v.is_finite());
            let (Some(t), Some(v), false) = (&tag, value, stat.multiplier == Some(true)) else {
                return Measured {
                    original: original.clone(),
                    stat,
                    renamed_from,
                    tag,
                    value: None,
                    misfits: false,
                };
            };
            let violation = violation_for(t, &stat, v);
            Measured {
                original: original.clone(),
                misfits: violation.is_none_or(|x| x > CORRECTION_MISFIT_THRESHOLD),
                value: Some(v),
                stat,
                renamed_from,
                tag,
            }
        })
        .collect();

    let misfitting = measured.iter().filter(|m| m.misfits).count();
    if misfitting > 1 {
        tracing::warn!(
            "riven grade: {misfitting} of {} stats are out of range for \"{weapon_name}\" - weapon or disposition is wrong, keeping the scanned names",
            measured.len()
        );
        return (
            measured.into_iter().map(|m| m.original).collect(),
            corrections,
        );
    }

    for m in &measured {
        if let Some(from) = &m.renamed_from {
            tracing::info!(
                "riven grade: \"{from}\" cannot roll on \"{weapon_name}\" - renamed \"{}\"",
                m.stat.name
            );
            corrections += 1;
        }
    }

    let corrected = measured
        .into_iter()
        .map(|m| {
            let (Some(tag), Some(value), true) = (&m.tag, m.value, m.misfits) else {
                return m.stat;
            };
            let mut fit: Vec<&str> = vec![];
            for sibling in confusion_siblings(tag) {
                let s = weapon_damage_tag(sibling, melee);
                if s != tag
                    && !fit.contains(&s)
                    && violation_for(s, &m.stat, value).is_some_and(|v| v <= FIT_TOLERANCE)
                {
                    fit.push(s);
                }
            }
            if let [only] = fit.as_slice() {
                let name = data.stat_display_name(only, melee);
                tracing::info!(
                    "riven grade: \"{}\" misfits \"{weapon_name}\" - corrected to \"{name}\"",
                    m.stat.name
                );
                corrections += 1;
                return RivenStatInput { name, ..m.stat };
            }
            m.stat
        })
        .collect();
    (corrected, corrections)
}

#[derive(Clone, Copy, PartialEq)]
enum AttrGrade {
    Decisive,
    Good,
    NotHelping,
    Bad,
}

fn grade_from_good_rolls(data: &GoodRollData, good: &[String], bad: &[String]) -> &'static str {
    let in_any = |tag: &String, f: fn(&super::best_attributes::GoodRoll) -> &Vec<String>| {
        data.good_attrs.iter().any(|g| f(g).contains(tag))
    };
    let negative: Vec<AttrGrade> = bad
        .iter()
        .map(|tag| {
            if data.accepted_bad_attrs.contains(tag) {
                AttrGrade::Good
            } else if in_any(tag, |g| &g.mandatory) || in_any(tag, |g| &g.optional) {
                AttrGrade::Bad
            } else {
                AttrGrade::NotHelping
            }
        })
        .collect();
    let positive: Vec<AttrGrade> = good
        .iter()
        .map(|tag| {
            if in_any(tag, |g| &g.mandatory) {
                AttrGrade::Decisive
            } else if in_any(tag, |g| &g.optional) {
                AttrGrade::Good
            } else {
                AttrGrade::NotHelping
            }
        })
        .collect();

    let flag = data.good_attrs.iter().any(|g| {
        g.mandatory.iter().all(|m| good.contains(m))
            && good
                .iter()
                .all(|t| g.mandatory.contains(t) || g.optional.contains(t))
    });
    let num = positive
        .iter()
        .filter(|p| matches!(p, AttrGrade::Decisive | AttrGrade::Good))
        .count();
    let has = |g| negative.contains(&g);

    if has(AttrGrade::Bad) {
        if (flag && num >= 2) || num >= 3 {
            "OK"
        } else {
            "Bad"
        }
    } else if has(AttrGrade::NotHelping) {
        if flag || num >= 2 {
            "Good"
        } else if num >= 1 {
            "OK"
        } else {
            "Bad"
        }
    } else if flag {
        if num >= 2 && !negative.is_empty() {
            "Great"
        } else {
            "Good"
        }
    } else if num >= 2 {
        "Good"
    } else if num >= 1 {
        "OK"
    } else {
        "Bad"
    }
}

/// Scores 44bananas' per-weapon good-roll data; unknown weapons return "?".
pub fn compute_attribute_grade(
    data: &RivenData,
    rolls: &GoodRolls,
    stats: &[RivenStatInput],
    weapon_name: &str,
) -> String {
    let Some(sheet) = rolls.get(data, weapon_name) else {
        return "?".into();
    };
    let tags = |positive: bool| -> Vec<String> {
        stats
            .iter()
            .filter(|s| s.positive == positive)
            .map(|s| stat_name_to_tag(&s.name).map_or_else(|| s.name.clone(), str::to_owned))
            .collect()
    };
    grade_from_good_rolls(sheet, &tags(true), &tags(false)).into()
}

struct Prepared<'a> {
    stat: &'a RivenStatInput,
    tag: Option<&'static str>,
    entry: Option<&'a UpgradeEntry>,
    displayed: Option<f64>,
    half_step: f64,
}

impl Prepared<'_> {
    fn raw_float(&self, ctx: (f64, usize, usize, i32), value: f64) -> f64 {
        unparse(
            self.stat.positive,
            value,
            self.entry.unwrap(),
            ctx,
            self.tag.unwrap(),
        )
    }

    /// Wolf Sledge Range spans 0.2 to 0.3 metres at rank 0, so half a display step is half the
    /// roll: ask whether the rounding interval can reach [0,1].
    /// The card value (unsigned, or the x-multiplier) a roll of 0 and of 1 would show. The unparse
    /// maps a displayed value to a roll linearly, so two probes pin both ends.
    fn display_range(&self, ctx: (f64, usize, usize, i32)) -> Option<(f64, f64)> {
        let (f0, f1) = (self.raw_float(ctx, 0.0), self.raw_float(ctx, 1.0));
        if (f1 - f0).abs() < 1e-12 {
            return None;
        }
        let shown = |roll: f64| (roll - f0) / (f1 - f0);
        let card = |d: f64| {
            if self.stat.multiplier != Some(true) {
                return d;
            }
            let frac = if self.tag.is_some_and(is_non_percentage) {
                d
            } else {
                d / 100.0
            };
            if self.stat.positive {
                1.0 + frac
            } else {
                1.0 - frac
            }
        };
        let (a, b) = (card(shown(0.0)), card(shown(1.0)));
        Some((a.min(b), a.max(b)))
    }

    fn fits_at(&self, ctx: (f64, usize, usize, i32)) -> bool {
        let d = self.displayed.unwrap();
        let a = self.raw_float(ctx, d - self.half_step);
        let b = self.raw_float(ctx, d + self.half_step);
        a.min(b) <= 1.0 + FIT_TOLERANCE && a.max(b) >= -FIT_TOLERANCE
    }
}

#[derive(Clone)]
struct Refit {
    name: String,
    disposition: f64,
    lvl: i32,
    buffs: usize,
    curses: usize,
}

/// Grades a card. `mod_rank` is the rank a warframe.market listing states; it is dropped again
/// when the values contradict it.
pub fn grade_riven(
    data: &RivenData,
    rolls: &GoodRolls,
    weapon_name: &str,
    stats: &[RivenStatInput],
    mod_rank: Option<i32>,
) -> Option<RivenGradeResult> {
    if stats.is_empty() {
        return None;
    }
    let Some(base_disposition) = data.weapon_disposition(weapon_name) else {
        tracing::warn!("riven grade: weapon not found: \"{weapon_name}\"");
        return None;
    };
    let Some(riven_type) = data.resolve_riven_type(weapon_name) else {
        tracing::warn!("riven grade: no riven type for weapon: \"{weapon_name}\"");
        return None;
    };

    let num_buffs = stats.iter().filter(|s| s.positive).count();
    let num_curses = stats.len() - num_buffs;
    if num_buffs > MAX_RIVEN_BUFFS || num_curses > MAX_RIVEN_CURSES {
        tracing::warn!(
            "riven grade: impossible stat shape ({num_buffs} buffs / {num_curses} curses) - skipping grade"
        );
        return None;
    }
    let stated_rank = mod_rank.filter(|r| (0..=MAX_RIVEN_MOD_RANK).contains(r));
    let mut level = stated_rank.unwrap_or(MAX_RIVEN_MOD_RANK);
    let (mut buffs, mut curses) = (num_buffs, num_curses);

    let prepared: Vec<Prepared> = stats
        .iter()
        .map(|stat| {
            let tag = stat_name_to_tag(&stat.name);
            let entry = tag.and_then(|t| data.find_upgrade_entry(riven_type, t));
            let is_fraction = tag.is_some_and(is_non_percentage);
            let mut displayed = None;
            // Half a display step: the card shows one decimal, two for an x-multiplier.
            let mut half_step = 0.05;
            if let Some(v) = stat.value.filter(|v| v.is_finite()) {
                if stat.multiplier == Some(true) {
                    // "x1.05" is a +0.05 multiplier; faction damage is a non-percentage tag whose
                    // displayed value IS that fraction.
                    let fraction = if stat.positive { v - 1.0 } else { 1.0 - v };
                    displayed = Some(if is_fraction {
                        fraction
                    } else {
                        fraction * 100.0
                    });
                    half_step = if is_fraction { 0.005 } else { 0.5 };
                } else {
                    displayed = Some(v);
                }
            }
            Prepared {
                stat,
                tag,
                entry,
                displayed,
                half_step,
            }
        })
        .collect();

    // The roll screen names the family but uses the linked variant's disposition, and a
    // chat-linked mod shows its values at its own rank: an unranked card reads 1/9 of max.
    let mut disposition = base_disposition;
    let gradeable: Vec<&Prepared> = prepared
        .iter()
        .filter(|p| p.tag.is_some() && p.entry.is_some() && p.displayed.is_some())
        .collect();
    if !gradeable.is_empty() {
        let violation_at = |ctx: (f64, usize, usize, i32)| -> f64 {
            gradeable
                .iter()
                .map(|p| overshoot(p.raw_float(ctx, p.displayed.unwrap())))
                .sum()
        };
        let all_fit_at = |ctx: (f64, usize, usize, i32)| gradeable.iter().all(|p| p.fits_at(ctx));

        if !all_fit_at((disposition, buffs, curses, level)) {
            let candidates: Vec<(String, f64)> =
                std::iter::once((weapon_name.to_owned(), base_disposition))
                    .chain(data.family_variants(weapon_name))
                    .collect();
            // Two stats are needed to pin a rank; a lone value fits several.
            let search_lvls: Vec<i32> = if gradeable.len() >= 2 {
                (0..=MAX_RIVEN_MOD_RANK).rev().collect()
            } else {
                vec![]
            };
            let refit_lvls: Vec<i32> = match stated_rank {
                Some(r) => std::iter::once(r)
                    .chain(search_lvls.into_iter().filter(|l| *l != r))
                    .collect(),
                None => search_lvls,
            };
            let refit_at = |b: usize, c: usize| -> Option<Refit> {
                for &lvl in &refit_lvls {
                    let mut best: Option<Refit> = None;
                    for (name, disp) in &candidates {
                        if !all_fit_at((*disp, b, c, lvl)) {
                            continue;
                        }
                        let closer = best.as_ref().is_none_or(|x| {
                            (disp - base_disposition).abs()
                                < (x.disposition - base_disposition).abs()
                        });
                        if closer {
                            best = Some(Refit {
                                name: name.clone(),
                                disposition: *disp,
                                lvl,
                                buffs: b,
                                curses: c,
                            });
                        }
                    }
                    if best.is_some() {
                        return best;
                    }
                }
                None
            };

            let refit = plausible_stat_counts(num_buffs, num_curses)
                .into_iter()
                .find_map(|(b, c)| refit_at(b, c));
            if let Some(refit) = refit {
                if refit.disposition != disposition {
                    tracing::info!(
                        "riven grade: \"{weapon_name}\" dispo misfits the rolled values - grading as \"{}\"",
                        refit.name
                    );
                    disposition = refit.disposition;
                }
                if refit.lvl != level {
                    tracing::info!(
                        "riven grade: \"{weapon_name}\" values match rank {} - grading at that rank",
                        refit.lvl
                    );
                    level = refit.lvl;
                }
                if (refit.buffs, refit.curses) != (buffs, curses) {
                    tracing::info!(
                        "riven grade: \"{weapon_name}\" values match {} buffs / {} curses - grading as a card the scan read short",
                        refit.buffs,
                        refit.curses
                    );
                    (buffs, curses) = (refit.buffs, refit.curses);
                }
            } else {
                let mut best = (
                    weapon_name.to_owned(),
                    disposition,
                    buffs,
                    curses,
                    violation_at((disposition, buffs, curses, level)),
                );
                for (b, c) in plausible_stat_counts(num_buffs, num_curses) {
                    for (name, disp) in &candidates {
                        let violation = violation_at((*disp, b, c, level));
                        let closer =
                            (disp - base_disposition).abs() < (best.1 - base_disposition).abs();
                        if violation < best.4 - 1e-9 || (violation < best.4 + 1e-9 && closer) {
                            best = (name.clone(), *disp, b, c, violation);
                        }
                    }
                }
                // No per-weapon disposition explains the card: the normalized one, at the card's own shape.
                if best.4 > CORRECTION_MISFIT_THRESHOLD
                    && all_fit_at((NORMALIZED_DISPOSITION, buffs, curses, level))
                {
                    best = (
                        "normalized".to_owned(),
                        NORMALIZED_DISPOSITION,
                        buffs,
                        curses,
                        0.0,
                    );
                }
                if best.1 != disposition {
                    tracing::info!(
                        "riven grade: \"{weapon_name}\" dispo misfits the rolled values - grading as \"{}\"",
                        best.0
                    );
                    disposition = best.1;
                }
                if (best.2, best.3) != (buffs, curses) {
                    tracing::info!(
                        "riven grade: \"{weapon_name}\" sits closest to {} buffs / {} curses - grading as a card the scan read short",
                        best.2,
                        best.3
                    );
                    (buffs, curses) = (best.2, best.3);
                }
            }
        }
    }

    let mut graded = Vec::with_capacity(prepared.len());
    let (mut score_sum, mut scored, mut scored_buffs, mut clamped_buffs) = (0.0, 0usize, 0, 0);
    let ctx = (disposition, buffs, curses, level);
    let as_graded = |p: &Prepared, grade: &str, roll_float: f64| {
        let range = if p.entry.is_some() && p.tag.is_some() {
            p.display_range(ctx)
        } else {
            None
        };
        RivenGradedStat {
            min: range.map(|r| r.0),
            max: range.map(|r| r.1),
            name: p.stat.name.clone(),
            positive: p.stat.positive,
            display_positive: p.stat.display_positive,
            value: p.stat.value,
            multiplier: p.stat.multiplier,
            grade: grade.into(),
            roll_float,
        }
    };
    for p in &prepared {
        if p.tag.is_none() || p.entry.is_none() {
            // An unknown stat is neutral: B grade, score 0.
            graded.push(as_graded(p, "B", 0.5));
            scored += 1;
        } else if let Some(d) = p.displayed {
            let roll = p
                .raw_float((disposition, buffs, curses, level), d)
                .clamp(0.0, 1.0);
            let positive = p.stat.positive;
            graded.push(as_graded(p, float_to_grade(roll, !positive), roll));
            if positive {
                scored_buffs += 1;
                if roll == 0.0 || roll == 1.0 {
                    clamped_buffs += 1;
                }
            }
            score_sum += lerp(-10.0, 10.0, if positive { roll } else { 1.0 - roll });
            scored += 1;
        } else {
            graded.push(as_graded(p, "?", 0.5));
        }
    }

    // Every buff pinned to the edge of its range contradicts the graded rank; a quarter of the
    // unranked listings carry max-rank rolls.
    if stated_rank.is_some()
        && scored_buffs > 0
        && clamped_buffs == scored_buffs
        && let Some(searched) = grade_riven(data, rolls, weapon_name, stats, None)
    {
        return Some(searched);
    }

    let overall_grade = if scored > 0 {
        let avg = score_sum / scored as f64;
        float_to_grade(inverse_lerp(-10.0, 10.0, avg), false).into()
    } else {
        "?".into()
    };
    Some(RivenGradeResult {
        stats: graded,
        overall_grade,
        attribute_grade: compute_attribute_grade(data, rolls, stats, weapon_name),
    })
}
