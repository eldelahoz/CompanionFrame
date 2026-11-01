// The game lists drops common-first, @wfcd hands them rare-first. The planner
// overlay wants rare-first because its slot n is named after the rarity it shows.
type RelicRewardOrder = "common-first" | "rare-first";

const RELIC_RARITY_RANK: Readonly<Record<string, number>> = Object.freeze({
  common: 0,
  uncommon: 1,
  rare: 2,
});

// Unknown rarities rank past rare in either order, so they never displace a real drop.
function relicRarityRank(rarity: string | null | undefined, order: RelicRewardOrder): number {
  const rank = RELIC_RARITY_RANK[String(rarity ?? "").toLowerCase()];
  if (rank === undefined) return 3;
  return order === "rare-first" ? 2 - rank : rank;
}

export function sortRelicRewards<T extends { rarity?: string | null; chance?: number | null }>(
  rewards: readonly T[],
  order: RelicRewardOrder = "common-first",
): T[] {
  return rewards.slice().sort((a, b) => {
    const rank = relicRarityRank(a.rarity, order) - relicRarityRank(b.rarity, order);
    if (rank !== 0) return rank;
    return (b.chance ?? 0) - (a.chance ?? 0);
  });
}
