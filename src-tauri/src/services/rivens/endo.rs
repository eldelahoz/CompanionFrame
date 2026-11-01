//! Endo a riven dissolves for: 100*(MR-8) + floor(22.5 * 2^rank) + 200*rerolls - 7.
//! wiki.warframe.com documents no formula; this is the Fandom mirror's, which the community
//! calculators agree with: https://warframe.fandom.com/wiki/Riven_Mods

/// Inputs are clamped to the ranges the game can produce, so a hostile auction payload cannot
/// drive the result anywhere. NaN clamps to the minimum.
pub fn dissolve_endo(mastery_rank: f64, mod_rank: f64, rerolls: f64) -> i64 {
    let clamp = |v: f64, min: f64, max: f64| {
        if v.is_finite() {
            v.trunc().clamp(min, max)
        } else {
            min
        }
    };
    let mr = clamp(mastery_rank, 8.0, 16.0);
    let rank = clamp(mod_rank, 0.0, 8.0);
    let rolls = clamp(rerolls, 0.0, 10_000.0);
    let total = 100.0 * (mr - 8.0) + (22.5 * 2f64.powf(rank)).floor() + 200.0 * rolls - 7.0;
    total.max(0.0) as i64
}

/// Endo per platinum asked. A zero-or-less price has no ratio rather than an infinite one, so a
/// free listing can never satisfy a minimum.
pub fn endo_per_plat(mastery_rank: f64, mod_rank: f64, rerolls: f64, platinum: f64) -> Option<f64> {
    (platinum.is_finite() && platinum > 0.0)
        .then(|| dissolve_endo(mastery_rank, mod_rank, rerolls) as f64 / platinum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_community_formula() {
        assert_eq!(dissolve_endo(13.0, 0.0, 0.0), 515);
        assert_eq!(dissolve_endo(8.0, 0.0, 0.0), 15);
        assert_eq!(dissolve_endo(16.0, 8.0, 10.0), 800 + 5760 + 2000 - 7);
        assert_eq!(dissolve_endo(14.0, 3.0, 2.0), 600 + 180 + 400 - 7);
    }

    #[test]
    fn clamps_hostile_inputs_to_game_ranges() {
        assert_eq!(dissolve_endo(99.0, 0.0, 0.0), dissolve_endo(16.0, 0.0, 0.0));
        assert_eq!(dissolve_endo(-5.0, 0.0, 0.0), dissolve_endo(8.0, 0.0, 0.0));
        assert_eq!(dissolve_endo(8.0, 99.0, 0.0), dissolve_endo(8.0, 8.0, 0.0));
        assert_eq!(
            dissolve_endo(8.0, f64::NAN, f64::INFINITY),
            dissolve_endo(8.0, 0.0, 0.0)
        );
    }

    #[test]
    fn endo_per_plat_refuses_non_positive_prices() {
        assert!((endo_per_plat(13.0, 0.0, 0.0, 100.0).unwrap() - 5.15).abs() < 1e-9);
        assert_eq!(endo_per_plat(13.0, 0.0, 0.0, 0.0), None);
        assert_eq!(endo_per_plat(13.0, 0.0, 0.0, -3.0), None);
        assert_eq!(endo_per_plat(13.0, 0.0, 0.0, f64::NAN), None);
    }
}
