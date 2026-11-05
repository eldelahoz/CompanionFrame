//! Matches a confirmed in-game trade to the user's own WFM listings and closes them (port of
//! WFHelper `tradeWfmMatcher.ts`). Matching is read-only; closing is a separate call that only a
//! user-confirmed action makes (`workbench_close_match`), never a timer or tracker.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::services::gdpr_import::{Direction, TradeType};
use crate::services::item_db::normalize_for_slug;
use crate::services::wfm::catalog::SetLookup;
use crate::services::wfm::errors::WfmError;
use crate::services::wfm::orders::{MyOrder, MyOrders};

const CLOSE_DEDUP: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchTradeItem {
    pub display_name: String,
    pub count: u32,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchTrade {
    pub partner: String,
    #[specta(type = f64)]
    pub plat_change: i64,
    #[serde(rename = "type")]
    pub kind: TradeType,
    pub items: Vec<WorkbenchTradeItem>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WorkbenchMatchKind {
    /// Regular market order.
    Order,
    /// Riven auction, closed through a different route.
    Contract,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchTradeMatch {
    pub kind: WorkbenchMatchKind,
    pub order_id: String,
    pub item_name: String,
    pub item_url_name: Option<String>,
    pub item_thumb: Option<String>,
    pub quantity: u32,
    pub platinum: u32,
    pub partner: String,
    #[serde(rename = "type")]
    pub trade_type: TradeType,
    /// The listing's bundle size, so closing sends a quantity WFM accepts.
    pub per_trade: Option<u32>,
}

/// One of the user's riven auctions (v1 `/profile/*/contracts`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatchContract {
    pub id: String,
    pub weapon_url_name: Option<String>,
    pub riven_suffix: Option<String>,
    pub item_thumb: Option<String>,
    pub platinum: u32,
}

/// Catalog details of one item; `has_ranks` is only `Some` when the live item says so.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemRankInfo {
    pub id: String,
    pub slug: String,
    pub has_ranks: Option<bool>,
}

/// What the matcher needs from WFM; tests inject fakes.
pub trait MatchHost: Send + Sync + 'static {
    fn logged_in(&self) -> bool;
    fn my_orders(&self) -> impl Future<Output = Result<MyOrders, WfmError>> + Send;
    /// Catalog slug of an item by exact (case-insensitive) name.
    fn slug_by_name(&self, name: &str) -> impl Future<Output = Option<String>> + Send;
    fn item_rank_info(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<Option<ItemRankInfo>, WfmError>> + Send;
    /// `Err` means the set's contents are unavailable, never a guess.
    fn set_lookup(&self, slug: &str) -> impl Future<Output = Result<SetLookup, WfmError>> + Send;
    fn my_contracts(&self) -> impl Future<Output = Result<Vec<MatchContract>, WfmError>> + Send;
    fn close_order(
        &self,
        id: &str,
        quantity: u32,
        per_trade: Option<u32>,
    ) -> impl Future<Output = Result<(), WfmError>> + Send;
    fn close_contract(&self, id: &str) -> impl Future<Output = Result<(), WfmError>> + Send;
}

pub struct Matcher<H> {
    host: H,
    /// Listings closed in the last 30 s: duplicate EE.log delivery must not close them twice.
    recently_closed: Mutex<HashMap<String, Instant>>,
}

pub(crate) struct ParsedName {
    pub(crate) base_name: String,
    rank: Option<u32>,
    riven: Option<(String, String)>,
}

macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}
re!(PAREN_SUFFIX, r"^(.*?)\s*\(([^()]*)\)\s*$");
re!(RANK_VALUE, r"\b(\d+)\b");
re!(RIVEN_RANK, r"(?i)riven\s+rank");
re!(VEILED_RIVEN, r"(?i)riven\s+mod$");
re!(BLUEPRINT_SUFFIX, r"(?i) Blueprint$");

/// "Serration (RANK 10)" -> ("Serration", "(RANK 10)"); None without a parenthesised suffix.
pub(crate) fn split_paren_suffix(name: &str) -> Option<(&str, &str)> {
    let c = PAREN_SUFFIX.captures(name.trim())?;
    let (base, suffix) = (c.get(1)?, c.get(2)?);
    // Keep the suffix verbatim, parentheses included.
    Some((base.as_str().trim(), &name.trim()[suffix.start() - 1..]))
}

/// "Serration (RANK 10)" / "Rubico Visio-Critatis (RIVEN RANK 0)" -> base name, rank, riven parts.
pub(crate) fn parse_traded_name(display: &str) -> ParsedName {
    let name = display.trim();
    let plain = || ParsedName {
        base_name: name.to_owned(),
        rank: None,
        riven: None,
    };
    let Some(c) = PAREN_SUFFIX.captures(name) else {
        return plain();
    };
    let (base, suffix) = (c[1].trim(), c[2].trim());
    if base.is_empty() {
        return plain();
    }
    let rank = RANK_VALUE
        .captures(suffix)
        .and_then(|m| m[1].parse::<u32>().ok());
    let base_name = base.to_owned();
    if !RIVEN_RANK.is_match(suffix) {
        return ParsedName {
            base_name,
            rank,
            riven: None,
        };
    }
    // Veiled mods use normal listings; unveiled rivens use auctions.
    let cut = base.rfind(' ').filter(|c| *c > 0);
    let riven = match cut {
        Some(c) if !VEILED_RIVEN.is_match(base) => {
            Some((base[..c].trim().to_owned(), base[c + 1..].trim().to_owned()))
        }
        _ => None,
    };
    ParsedName {
        base_name,
        rank,
        riven,
    }
}

pub(crate) fn normalize_name(name: &str) -> String {
    BLUEPRINT_SUFFIX
        .replace(name, "")
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn closeable_quantity(order: &MyOrder, traded: u32) -> u32 {
    let available = traded.min(order.quantity.max(1));
    let per_trade = order.per_trade.max(1);
    if available.is_multiple_of(per_trade) {
        available
    } else {
        0
    }
}

struct Prepared<'a> {
    item: &'a WorkbenchTradeItem,
    parsed: ParsedName,
    slug: Option<String>,
    remaining: u32,
}

struct SetResult {
    matches: Vec<WorkbenchTradeMatch>,
    consumed: HashMap<String, u32>,
    blocked: HashSet<String>,
}

impl<H: MatchHost> Matcher<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            recently_closed: Mutex::default(),
        }
    }

    fn is_recent(&self, id: &str) -> bool {
        self.recently_closed.lock().unwrap().contains_key(id)
    }

    async fn traded_slug(&self, base: &str) -> Option<String> {
        if base.is_empty() {
            return None;
        }
        match self.host.slug_by_name(base).await {
            Some(s) => Some(s),
            None => {
                self.host
                    .slug_by_name(&BLUEPRINT_SUFFIX.replace(base, ""))
                    .await
            }
        }
    }

    async fn rank_matches(&self, order: &MyOrder, rank: u32) -> bool {
        if order.mod_rank == Some(rank) {
            return true;
        }
        if rank != 0 || order.mod_rank.is_some() {
            return false;
        }
        let (Some(id), Some(slug)) = (&order.item_id, &order.item_slug) else {
            return false;
        };
        // Rankless mods still log RANK 0.
        match self.host.item_rank_info(slug).await {
            Ok(Some(d)) => &d.id == id && &d.slug == slug && d.has_ranks == Some(false),
            Ok(None) => false,
            Err(e) => {
                tracing::warn!("[Matcher] Could not verify the item's rank policy: {e}");
                false
            }
        }
    }

    pub async fn match_trade(&self, trade: &WorkbenchTrade) -> Vec<WorkbenchTradeMatch> {
        if !self.host.logged_in() {
            tracing::info!("[Matcher] Skipping - not logged in to WFM");
            return vec![];
        }
        let sale = trade.kind == TradeType::Sale;
        let relevant: Vec<&WorkbenchTradeItem> = trade
            .items
            .iter()
            .filter(|i| {
                i.direction
                    == if sale {
                        Direction::Given
                    } else {
                        Direction::Received
                    }
            })
            .collect();
        if relevant.is_empty() {
            tracing::info!("[Matcher] No relevant items to match");
            return vec![];
        }
        self.recently_closed
            .lock()
            .unwrap()
            .retain(|_, t| t.elapsed() <= CLOSE_DEDUP);

        let mut prepared = vec![];
        for item in &relevant {
            let parsed = parse_traded_name(&item.display_name);
            let slug = self.traded_slug(&parsed.base_name).await;
            prepared.push(Prepared {
                item,
                parsed,
                slug,
                remaining: item.count,
            });
        }

        let mut matches = self.match_rivens(&mut prepared, trade).await;
        let orders = match self.host.my_orders().await {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("[Matcher] Failed to fetch orders: {e}");
                return matches;
            }
        };
        let candidates = if sale { orders.sell } else { orders.buy };
        if candidates.is_empty() {
            return matches;
        }

        // Insertion-ordered: the order of the closed listings follows the order of the trade.
        let mut traded: Vec<(String, u32)> = vec![];
        for p in prepared.iter().filter(|p| p.remaining >= 1) {
            let Some(slug) = &p.slug else { continue };
            match traded.iter_mut().find(|(s, _)| s == slug) {
                Some((_, n)) => *n += p.remaining,
                None => traded.push((slug.clone(), p.remaining)),
            }
        }
        let sets = self.match_sets(&candidates, &traded, trade).await;
        let mut used: HashSet<String> = sets.matches.iter().map(|m| m.order_id.clone()).collect();
        matches.extend(sets.matches);
        for (slug, mut left) in sets.consumed {
            for p in prepared.iter_mut() {
                if p.slug.as_ref() != Some(&slug) || left < 1 {
                    continue;
                }
                let take = p.remaining.min(left);
                p.remaining -= take;
                left -= take;
            }
        }

        for p in &prepared {
            if p.remaining < 1 || p.slug.as_ref().is_some_and(|s| sets.blocked.contains(s)) {
                continue;
            }
            let wanted = normalize_name(&p.parsed.base_name);
            if wanted.is_empty() {
                continue;
            }
            let mut matching: Vec<&MyOrder> = candidates
                .iter()
                .filter(|o| {
                    !self.is_recent(&o.id)
                        && !used.contains(&o.id)
                        && (normalize_name(&o.item_name) == wanted
                            || (p.slug.is_some() && o.item_slug == p.slug))
                })
                .collect();
            if let Some(rank) = p.parsed.rank {
                let mut kept = vec![];
                for o in matching {
                    if self.rank_matches(o, rank).await {
                        kept.push(o);
                    }
                }
                matching = kept;
            }
            matching.sort_by(|a, b| {
                if relevant.len() == 1 {
                    let diff = |o: &MyOrder| (i64::from(o.platinum) - trade.plat_change).abs();
                    let by_plat = diff(a).cmp(&diff(b));
                    if by_plat.is_ne() {
                        return by_plat;
                    }
                }
                a.quantity.max(1).cmp(&b.quantity.max(1))
            });
            let Some(best) = matching.first() else {
                continue;
            };
            let qty = closeable_quantity(best, p.remaining);
            if qty < 1 {
                tracing::warn!(
                    "[Matcher] Order {} cannot close {} unit(s)",
                    best.id,
                    p.remaining
                );
                continue;
            }
            used.insert(best.id.clone());
            matches.push(order_match(
                best,
                qty,
                || p.item.display_name.clone(),
                trade,
            ));
        }
        matches
    }

    async fn match_rivens(
        &self,
        items: &mut [Prepared<'_>],
        trade: &WorkbenchTrade,
    ) -> Vec<WorkbenchTradeMatch> {
        if trade.kind != TradeType::Sale || items.iter().all(|i| i.parsed.riven.is_none()) {
            return vec![];
        }
        let contracts = match self.host.my_contracts().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[Matcher] Failed to fetch riven auctions: {e}");
                return vec![];
            }
        };
        let riven_count = items.iter().filter(|i| i.parsed.riven.is_some()).count();
        let (mut out, mut used) = (vec![], HashSet::new());
        for item in items.iter_mut() {
            let Some((weapon, suffix)) = &item.parsed.riven else {
                continue;
            };
            let weapon_slug = normalize_for_slug(weapon);
            let same_weapon: Vec<&MatchContract> = contracts
                .iter()
                .filter(|c| {
                    !self.is_recent(&c.id)
                        && !used.contains(&c.id)
                        && c.weapon_url_name
                            .as_deref()
                            .is_some_and(|w| !w.is_empty() && normalize_for_slug(w) == weapon_slug)
                })
                .collect();
            let suffix_slug = normalize_for_slug(suffix);
            let exact = same_weapon.iter().find(|c| {
                c.riven_suffix
                    .as_deref()
                    .is_some_and(|s| !s.is_empty() && normalize_for_slug(s) == suffix_slug)
            });
            let best = exact.or_else(|| (same_weapon.len() == 1).then(|| &same_weapon[0]));
            let Some(best) = best else {
                tracing::info!(
                    "[Matcher] No riven auction matched \"{}\" ({} for weapon)",
                    item.parsed.base_name,
                    same_weapon.len()
                );
                continue;
            };
            used.insert(best.id.clone());
            item.remaining = item.remaining.saturating_sub(1);
            let traded_plat = u32::try_from(trade.plat_change).ok().filter(|p| *p > 0);
            out.push(WorkbenchTradeMatch {
                kind: WorkbenchMatchKind::Contract,
                order_id: best.id.clone(),
                item_name: item.parsed.base_name.clone(),
                item_url_name: best.weapon_url_name.clone(),
                item_thumb: best.item_thumb.clone(),
                quantity: 1,
                platinum: if riven_count == 1 {
                    traded_plat.unwrap_or(best.platinum)
                } else {
                    best.platinum
                },
                partner: trade.partner.clone(),
                trade_type: trade.kind,
                per_trade: None,
            });
        }
        out
    }

    /// A set listing closes when the trade delivers every part (times each part's quantity).
    async fn match_sets(
        &self,
        candidates: &[MyOrder],
        traded: &[(String, u32)],
        trade: &WorkbenchTrade,
    ) -> SetResult {
        let mut res = SetResult {
            matches: vec![],
            consumed: HashMap::new(),
            blocked: HashSet::new(),
        };
        if traded.len() < 2 {
            return res;
        }
        let mut set_orders: Vec<(&str, Vec<&MyOrder>)> = vec![];
        for o in candidates.iter().filter(|o| !self.is_recent(&o.id)) {
            let slug = o.item_slug.as_deref().unwrap_or("");
            if !slug.ends_with("_set") {
                continue;
            }
            match set_orders.iter_mut().find(|(s, _)| *s == slug) {
                Some((_, v)) => v.push(o),
                None => set_orders.push((slug, vec![o])),
            }
        }
        if set_orders.is_empty() {
            return res;
        }

        let mut definitions: Vec<(String, Vec<(String, u32)>)> = vec![];
        for (slug, _) in traded {
            match self.host.set_lookup(slug).await {
                Err(_) => {
                    res.blocked.insert(slug.clone());
                }
                Ok(SetLookup::Set { set_slug, parts }) => {
                    for p in &parts {
                        res.blocked.remove(&p.slug);
                    }
                    if set_orders.iter().any(|(s, _)| *s == set_slug) {
                        let parts = parts
                            .into_iter()
                            .map(|p| (p.slug, p.quantity_in_set))
                            .collect();
                        match definitions.iter_mut().find(|(s, _)| *s == set_slug) {
                            Some(d) => d.1 = parts,
                            None => definitions.push((set_slug, parts)),
                        }
                    }
                }
                Ok(SetLookup::NotSet) => {}
            }
        }

        let mut available: HashMap<&str, u32> =
            traded.iter().map(|(s, n)| (s.as_str(), *n)).collect();
        for (set_slug, parts) in &definitions {
            let covered = parts
                .iter()
                .map(|(s, q)| available.get(s.as_str()).copied().unwrap_or(0) / (*q).max(1))
                .min()
                .unwrap_or(0);
            if covered < 1 {
                continue;
            }
            let mut orders = set_orders
                .iter()
                .find(|(s, _)| s == set_slug)
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            orders.sort_by_key(|o| o.quantity.max(1));
            let Some(best) = orders.first() else { continue };
            for (s, q) in parts {
                let used = covered * q;
                if let Some(n) = available.get_mut(s.as_str()) {
                    *n = n.saturating_sub(used);
                }
                *res.consumed.entry(s.clone()).or_default() += used;
            }
            let qty = closeable_quantity(best, covered);
            if qty < 1 {
                res.blocked.extend(parts.iter().map(|(s, _)| s.clone()));
                continue;
            }
            res.matches
                .push(order_match(best, qty, || "(set)".into(), trade));
        }
        res
    }

    /// Closes every match, calling `on_closed` for each one WFM accepted; returns their order ids.
    /// A refused one is skipped so the others still close.
    pub async fn close_all(
        &self,
        matches: &[WorkbenchTradeMatch],
        mut on_closed: impl FnMut(&WorkbenchTradeMatch),
    ) -> Vec<String> {
        let mut closed = Vec::new();
        for m in matches {
            if self.close_match(m).await {
                on_closed(m);
                closed.push(m.order_id.clone());
            }
        }
        closed
    }

    /// Closes a match once despite duplicate EE.log delivery. `true` when WFM accepted it.
    pub async fn close_match(&self, m: &WorkbenchTradeMatch) -> bool {
        tracing::info!(
            "[Matcher] Closing {:?} {} ({})",
            m.kind,
            m.order_id,
            m.item_name
        );
        let res = match m.kind {
            WorkbenchMatchKind::Contract => self.host.close_contract(&m.order_id).await,
            WorkbenchMatchKind::Order => {
                self.host
                    .close_order(&m.order_id, m.quantity, m.per_trade)
                    .await
            }
        };
        match res {
            Ok(()) => {
                self.recently_closed
                    .lock()
                    .unwrap()
                    .insert(m.order_id.clone(), Instant::now());
                true
            }
            Err(e) => {
                tracing::warn!("[Matcher] Failed to close {}: {e}", m.order_id);
                false
            }
        }
    }
}

fn order_match(
    o: &MyOrder,
    quantity: u32,
    fallback_name: impl FnOnce() -> String,
    trade: &WorkbenchTrade,
) -> WorkbenchTradeMatch {
    WorkbenchTradeMatch {
        kind: WorkbenchMatchKind::Order,
        order_id: o.id.clone(),
        item_name: if o.item_name.is_empty() {
            fallback_name()
        } else {
            o.item_name.clone()
        },
        item_url_name: o.item_slug.clone(),
        item_thumb: o.item_thumb.clone(),
        quantity,
        platinum: o.platinum,
        partner: trade.partner.clone(),
        trade_type: trade.kind,
        per_trade: Some(o.per_trade.max(1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_rank_suffix_parses_rank_and_base() {
        let p = parse_traded_name("Pistol Mutation (RARE RANG 0)");
        assert_eq!((p.base_name.as_str(), p.rank), ("Pistol Mutation", Some(0)));
        assert_eq!(
            split_paren_suffix("Mutation de Pistolet (RARE RANG 0)"),
            Some(("Mutation de Pistolet", "(RARE RANG 0)"))
        );
    }
}
