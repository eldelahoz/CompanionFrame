//! Riven value analyzer: how well each warframe.market listing matches a card, the match-weighted
//! price estimate, and the keep / take / close verdict between two cards. Pure functions; the
//! overlay does the searching.
use std::collections::HashSet;

use serde::Serialize;

use crate::services::wfm::contracts::WfmContract;

/// Listings under this share of matching stats say little about the card and are left out.
pub const MIN_MATCH_PCT: u32 = 25;
/// Fewer weighted listings than this, or a best match under `LOW_BEST_MATCH`, is a weak estimate.
const MIN_LISTINGS: usize = 5;
const LOW_BEST_MATCH: u32 = 50;
/// Close call: the delta is within this many plat or this share of the larger median.
const CLOSE_PLAT: f64 = 25.0;
const CLOSE_SHARE: f64 = 0.10;
/// Listings shown under the single card.
pub const SHOWN_LISTINGS: usize = 5;

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ListingStat {
    /// warframe.market stat url name; the frontend maps it to an abbreviation.
    pub key: String,
    pub positive: bool,
    /// The card has this stat with the same sign.
    pub matched: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenListing {
    pub match_pct: u32,
    pub platinum: u32,
    pub stats: Vec<ListingStat>,
    /// "ingame" | "online" | "offline"
    pub seller_status: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BestMatch {
    pub match_pct: u32,
    pub platinum: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EstimateSource {
    Listings,
    /// No usable listing: the median of the weapon's whole pool.
    WeaponMedian,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenEstimate {
    pub source: EstimateSource,
    pub p25: u32,
    pub median: u32,
    pub p75: u32,
    /// Listings the estimate used.
    pub n: u32,
    pub low_confidence: bool,
    pub best: Option<BestMatch>,
}

#[derive(Clone, Debug, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenAnalysis {
    pub estimate: Option<RivenEstimate>,
    /// Best matches first, at most `SHOWN_LISTINGS`.
    pub listings: Vec<RivenListing>,
}

/// A card stat as the listings name it: wfm url name and sign.
pub type CardStat = (String, bool);

fn price(l: &WfmContract) -> u32 {
    l.buyout_platinum.unwrap_or(l.platinum)
}

/// Share of stat names in common, with sign, out of the larger of the two stat sets.
pub fn match_pct(card: &[CardStat], listing: &[CardStat]) -> u32 {
    let card: HashSet<_> = card.iter().collect();
    let common = listing.iter().filter(|s| card.contains(s)).count();
    let total = card.len().max(listing.len());
    if total == 0 {
        0
    } else {
        (common as f64 / total as f64 * 100.0).round() as u32
    }
}

/// The value below which `q` of the total weight lies; `rows` are (price, weight).
fn weighted_percentile(rows: &[(u32, f64)], q: f64) -> u32 {
    let total: f64 = rows.iter().map(|r| r.1).sum();
    let mut seen = 0.0;
    for (p, w) in rows {
        seen += w;
        if seen >= total * q - 1e-9 {
            return *p;
        }
    }
    rows.last().map_or(0, |r| r.0)
}

fn median(prices: &mut [u32]) -> Option<u32> {
    prices.sort_unstable();
    prices.get(prices.len() / 2).copied()
}

/// Scores `pool` against the card, and estimates its value from the listings that match at least
/// `MIN_MATCH_PCT`; without any, from the whole pool's median.
pub fn analyze(card: &[CardStat], pool: &[WfmContract]) -> RivenAnalysis {
    let mut rows: Vec<RivenListing> = pool
        .iter()
        .map(|l| {
            let theirs: Vec<CardStat> = l
                .stats
                .iter()
                .map(|a| (a.url_name.clone(), a.positive != Some(false)))
                .collect();
            RivenListing {
                match_pct: match_pct(card, &theirs),
                platinum: price(l),
                stats: theirs
                    .into_iter()
                    .map(|(key, positive)| ListingStat {
                        matched: card.iter().any(|(k, p)| *k == key && *p == positive),
                        key,
                        positive,
                    })
                    .collect(),
                seller_status: l.seller_status.clone(),
                url: l.listing_url.clone(),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.match_pct
            .cmp(&a.match_pct)
            .then(a.platinum.cmp(&b.platinum))
    });
    let used: Vec<&RivenListing> = rows
        .iter()
        .filter(|r| r.match_pct >= MIN_MATCH_PCT)
        .collect();
    let best = used.first().map(|r| BestMatch {
        match_pct: r.match_pct,
        platinum: r.platinum,
    });
    let estimate = if used.is_empty() {
        let mut prices: Vec<u32> = rows.iter().map(|r| r.platinum).collect();
        median(&mut prices).map(|m| RivenEstimate {
            source: EstimateSource::WeaponMedian,
            p25: m,
            median: m,
            p75: m,
            n: prices.len() as u32,
            low_confidence: true,
            best: None,
        })
    } else {
        let mut by_price: Vec<(u32, f64)> = used
            .iter()
            .map(|r| (r.platinum, f64::from(r.match_pct) / 100.0))
            .collect();
        by_price.sort_by_key(|r| r.0);
        Some(RivenEstimate {
            source: EstimateSource::Listings,
            p25: weighted_percentile(&by_price, 0.25),
            median: weighted_percentile(&by_price, 0.5),
            p75: weighted_percentile(&by_price, 0.75),
            n: by_price.len() as u32,
            low_confidence: by_price.len() < MIN_LISTINGS
                || best.as_ref().is_some_and(|b| b.match_pct < LOW_BEST_MATCH),
            best,
        })
    };
    rows.truncate(SHOWN_LISTINGS);
    RivenAnalysis {
        estimate,
        listings: rows,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum VerdictKind {
    Keep,
    Take,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum GradeCmp {
    Better,
    Same,
    Worse,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RivenVerdict {
    pub kind: VerdictKind,
    /// New median minus current median, in plat.
    pub delta: i32,
    pub grade_cmp: GradeCmp,
}

/// Grade letters from worst to best, as `float_to_grade` hands them out.
const GRADE_ORDER: [&str; 11] = ["F", "C-", "C", "C+", "B-", "B", "B+", "A-", "A", "A+", "S"];

pub fn compare_grades(current: Option<&str>, new: Option<&str>) -> GradeCmp {
    let rank = |g: Option<&str>| g.and_then(|g| GRADE_ORDER.iter().position(|x| *x == g));
    match (rank(current), rank(new)) {
        (Some(c), Some(n)) => match n.cmp(&c) {
            std::cmp::Ordering::Greater => GradeCmp::Better,
            std::cmp::Ordering::Equal => GradeCmp::Same,
            std::cmp::Ordering::Less => GradeCmp::Worse,
        },
        _ => GradeCmp::Unknown,
    }
}

/// delta = new - current. Within max(25p, 10% of the larger median) with an equal grade it is a
/// close call; otherwise the higher median wins, and a better or worse grade settles a near-tie.
pub fn verdict(current: &RivenEstimate, new: &RivenEstimate, grade_cmp: GradeCmp) -> RivenVerdict {
    let delta = i64::from(new.median) - i64::from(current.median);
    let band = CLOSE_PLAT.max(f64::from(current.median.max(new.median)) * CLOSE_SHARE);
    let near = (delta as f64).abs() <= band;
    let kind = match (near, grade_cmp) {
        (true, GradeCmp::Same | GradeCmp::Unknown) => VerdictKind::Close,
        (true, GradeCmp::Better) => VerdictKind::Take,
        (true, GradeCmp::Worse) => VerdictKind::Keep,
        (false, _) if delta > 0 => VerdictKind::Take,
        (false, _) => VerdictKind::Keep,
    };
    RivenVerdict {
        kind,
        delta: delta as i32,
        grade_cmp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::wfm::contracts::WfmContractAttribute;

    fn s(k: &str, positive: bool) -> CardStat {
        (k.to_owned(), positive)
    }

    fn listing(plat: u32, stats: &[CardStat]) -> WfmContract {
        WfmContract {
            id: plat.to_string(),
            item_name: String::new(),
            item_id: None,
            item_url_name: None,
            weapon_url_name: None,
            riven_suffix: None,
            item_thumb: None,
            platinum: plat,
            buyout_platinum: None,
            starting_platinum: None,
            quantity: 1,
            visible: true,
            mod_rank: None,
            rerolls: None,
            mastery_level: None,
            polarity: None,
            minimal_reputation: None,
            is_direct_sell: true,
            listed_at: None,
            updated_at: None,
            note: None,
            stats: stats
                .iter()
                .map(|(k, p)| WfmContractAttribute {
                    url_name: k.clone(),
                    label: k.clone(),
                    value: None,
                    positive: Some(*p),
                })
                .collect(),
            listing_url: format!("https://warframe.market/auction/{plat}"),
            source_type: None,
            seller: None,
            seller_status: Some("online".into()),
        }
    }

    fn est(median: u32) -> RivenEstimate {
        RivenEstimate {
            source: EstimateSource::Listings,
            p25: median,
            median,
            p75: median,
            n: 9,
            low_confidence: false,
            best: None,
        }
    }

    #[test]
    fn match_counts_shared_stats_with_sign() {
        let card = [s("cd", true), s("ms", true), s("zoom", false)];
        assert_eq!(match_pct(&card, &card), 100);
        assert_eq!(match_pct(&card, &[s("cd", true), s("ms", true)]), 67);
        // The sign counts: a buff is not the same stat as the curse.
        assert_eq!(match_pct(&card, &[s("zoom", true)]), 0);
        // A listing with extra stats dilutes the share.
        let four = [s("cd", true), s("ms", true), s("x", true), s("y", true)];
        assert_eq!(match_pct(&card, &four), 50);
        assert_eq!(match_pct(&[], &[]), 0);
    }

    #[test]
    fn weighted_percentiles_follow_the_weights() {
        // 10p listing carries 3x the weight of the 100p one.
        let rows = [(10, 0.75), (100, 0.25)];
        assert_eq!(weighted_percentile(&rows, 0.5), 10);
        assert_eq!(weighted_percentile(&rows, 0.75), 10);
        assert_eq!(weighted_percentile(&rows, 0.76), 100);
        let even = [(10, 1.0), (20, 1.0), (30, 1.0), (40, 1.0)];
        assert_eq!(
            (
                weighted_percentile(&even, 0.25),
                weighted_percentile(&even, 0.5),
                weighted_percentile(&even, 0.75)
            ),
            (10, 20, 30)
        );
    }

    #[test]
    fn listings_under_the_floor_do_not_move_the_estimate() {
        let card = [s("a", true), s("b", true), s("c", true), s("d", false)];
        let pool = [
            listing(50, &card),
            listing(60, &card),
            // 1 of 4 shared: 25% is kept; a stranger at 0% is dropped.
            listing(500, &[s("a", true)]),
            listing(9999, &[s("z", true)]),
        ];
        let a = analyze(&card, &pool);
        let e = a.estimate.unwrap();
        assert_eq!(e.source, EstimateSource::Listings);
        assert_eq!(e.n, 3);
        assert!(e.median <= 60 && e.p75 <= 500);
        assert_eq!(
            e.best,
            Some(BestMatch {
                match_pct: 100,
                platinum: 50
            })
        );
        assert_eq!(a.listings[0].platinum, 50);
        assert!(a.listings[0].stats.iter().all(|x| x.matched));
        assert!(!a.listings.last().unwrap().stats[0].matched);
    }

    #[test]
    fn confidence_needs_enough_listings_and_a_close_match() {
        let card = [s("a", true), s("b", true)];
        let few: Vec<_> = (0..4).map(|i| listing(10 + i, &card)).collect();
        assert!(analyze(&card, &few).estimate.unwrap().low_confidence);
        let enough: Vec<_> = (0..5).map(|i| listing(10 + i, &card)).collect();
        assert!(!analyze(&card, &enough).estimate.unwrap().low_confidence);
        // Five listings, none above 50%.
        let weak: Vec<_> = (0..5)
            .map(|i| listing(10 + i, &[s("a", true), s("x", true), s("y", true)]))
            .collect();
        let e = analyze(&card, &weak).estimate.unwrap();
        assert_eq!(e.best.unwrap().match_pct, 33);
        assert!(e.low_confidence);
    }

    #[test]
    fn no_matching_listing_falls_back_to_the_weapon_median() {
        let card = [s("a", true), s("b", true)];
        let pool = [
            listing(10, &[s("z", true)]),
            listing(30, &[s("y", true)]),
            listing(20, &[s("x", true)]),
        ];
        let e = analyze(&card, &pool).estimate.unwrap();
        assert_eq!(
            (e.source, e.median, e.n, e.low_confidence),
            (EstimateSource::WeaponMedian, 20, 3, true)
        );
        assert!(e.best.is_none());
        assert_eq!(analyze(&card, &[]).estimate, None);
    }

    #[test]
    fn grades_compare_by_letter_order() {
        assert_eq!(compare_grades(Some("B+"), Some("A")), GradeCmp::Better);
        assert_eq!(compare_grades(Some("A"), Some("A")), GradeCmp::Same);
        assert_eq!(compare_grades(Some("S"), Some("A+")), GradeCmp::Worse);
        assert_eq!(compare_grades(None, Some("A")), GradeCmp::Unknown);
    }

    #[test]
    fn verdict_thresholds() {
        let v = |c, n, g| verdict(&est(c), &est(n), g);
        // Within 25p and the same grade: close.
        assert_eq!(v(100, 120, GradeCmp::Same).kind, VerdictKind::Close);
        // 10% of 1000 is 100p, so 90p apart is still close.
        assert_eq!(v(1000, 910, GradeCmp::Same).kind, VerdictKind::Close);
        assert_eq!(v(1000, 880, GradeCmp::Same).kind, VerdictKind::Keep);
        // Beyond the band the higher median wins whatever the grade.
        assert_eq!(v(420, 35, GradeCmp::Better).kind, VerdictKind::Keep);
        assert_eq!(v(35, 420, GradeCmp::Worse).kind, VerdictKind::Take);
        // A near-tie goes to the better grade.
        assert_eq!(v(100, 110, GradeCmp::Better).kind, VerdictKind::Take);
        assert_eq!(v(100, 110, GradeCmp::Worse).kind, VerdictKind::Keep);
        assert_eq!(v(420, 35, GradeCmp::Better).delta, -385);
    }
}
