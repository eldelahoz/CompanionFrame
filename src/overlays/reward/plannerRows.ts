import { DEFAULT_RELIC_PLANNER_FILTERS, relicOwnedCountForMode, relicQualityForMode, selectRelicPlannerRows, type RelicOverlayFilterPush } from "../../../config/shared/relicPlannerView";
import { relicGroupMatchesSearch } from "../../../config/shared/relicSearch";
import { evDataFor, type PriceLookup } from "../../lib/relic/relicEv";
import type { OwnedCounts, RelicDatabase, RelicGroup, RelicQuality } from "../../lib/relic/types";

export interface PlannerRow {
  group: RelicGroup;
  name: string;
  tier: string;
  vaulted: boolean;
  quality: RelicQuality;
  /** Copies of the refinement the EV is for. */
  count: number;
  ownedCount: number;
  ownedTotal: number;
  plat: number | null;
  ducat: number | null;
  ratio: number | null;
}

/** warframe.market slugs of every reward of the era’s relics, so their plat EV can be priced. */
export function plannerPriceSlugs(db: RelicDatabase, era: string | null): string[] {
  const slugs = new Set<string>();
  for (const g of Object.values(db.groups)) {
    const tier = g.tier.toLowerCase();
    if (era === "omnia" ? tier === "requiem" : era && tier !== era) continue;
    for (const q of Object.values(g.qualities)) for (const r of q?.rewards ?? []) if (r.urlName) slugs.add(r.urlName);
  }
  return [...slugs];
}

/**
 * The owned relics worth opening in a fissure of `era`, best ducat EV first. `omnia` takes every
 * era but Requiem (those open only in a Requiem fissure); no era (not detected) lists them all.
 * Never lists a relic the player does not own, unless `allRelics` (no inventory loaded): then every relic
 * of the era is ranked at Intact (or the pushed grade). Plat EV needs `price` (warframe.market medians); null without it.
 * `push` is what the Relics view's "Push to Overlay" sent: its filters, sort and pinned refinements; the squad size stays the overlay's own.
 */
export function plannerRows(
  db: RelicDatabase,
  owned: OwnedCounts,
  era: string | null,
  squadSize: number,
  push: RelicOverlayFilterPush | null = null,
  allRelics = false,
  price?: PriceLookup,
): PlannerRow[] {
  const pushed = push?.qualityMode ?? "owned";
  const qualityMode = allRelics && pushed === "owned" ? "intact" : pushed;
  const rows: PlannerRow[] = [];
  for (const group of Object.values(db.groups)) {
    const tier = group.tier.toLowerCase();
    if (era === "omnia" ? tier === "requiem" : era && tier !== era) continue;
    const counts = owned[group.key];
    const quality = relicQualityForMode(qualityMode, counts, push?.pinnedQualities?.[group.key]);
    if (!quality) continue;
    const ev = evDataFor(group, squadSize, qualityMode, quality, price);
    rows.push({
      group,
      name: group.name,
      tier: group.tier,
      vaulted: group.vaulted,
      quality,
      count: counts?.[quality] ?? 0,
      ownedCount: relicOwnedCountForMode(counts, qualityMode),
      ownedTotal: relicOwnedCountForMode(counts, "owned"),
      plat: ev.plat,
      ducat: ev.ducat,
      ratio: ev.ratio,
    });
  }
  if (!push) {
    const filters = { ...DEFAULT_RELIC_PLANNER_FILTERS, squadSize, sortMode: "ducat" as const, sortDirection: "desc" as const };
    return selectRelicPlannerRows(rows, filters, { matchesSearch: () => true, hasNeededReward: () => false });
  }
  const needed = push.neededRewardKeys ? new Set(push.neededRewardKeys) : null;
  return selectRelicPlannerRows(rows, { ...push, squadSize, qualityMode, ...(allRelics ? { ownedAbove: 0 } : {}) }, {
    matchesSearch: (row) => relicGroupMatchesSearch(row.group, push.search, { ownedCounts: owned[row.group.key] }),
    hasNeededReward: (row) => needed?.has(row.group.key) ?? true,
  });
}
