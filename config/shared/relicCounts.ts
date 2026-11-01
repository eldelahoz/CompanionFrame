const RELIC_COLLECTIONS = ["LevelKeys", "MiscItems", "Recipes"] as const;

function normalizedCount(value: unknown): number {
  if (value === undefined) return 1;
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return 0;
  return Math.floor(value);
}

function addCollection(
  counts: Map<string, number>,
  entries: unknown,
  includesItemType: (itemType: string) => boolean,
): void {
  if (!Array.isArray(entries)) return;

  const collectionCounts = new Map<string, number>();
  for (const entry of entries) {
    if (!entry || typeof entry !== "object") continue;
    const { ItemType, ItemCount } = entry as { ItemType?: unknown; ItemCount?: unknown };
    if (typeof ItemType !== "string" || !ItemType || !includesItemType(ItemType)) continue;
    collectionCounts.set(
      ItemType,
      (collectionCounts.get(ItemType) ?? 0) + normalizedCount(ItemCount),
    );
  }
  for (const [itemType, count] of collectionCounts) {
    counts.set(itemType, Math.max(counts.get(itemType) ?? 0, count));
  }
}

function isRelicInventoryItemType(itemType: string): boolean {
  return /VoidProjection/i.test(itemType) || /\/Lotus\/Relics\//i.test(itemType);
}

export function collectRelicInventoryCounts(
  inventoryData: Record<string, unknown>,
  includesItemType: (itemType: string) => boolean = isRelicInventoryItemType,
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const collection of RELIC_COLLECTIONS) {
    addCollection(counts, inventoryData[collection], includesItemType);
  }

  if (counts.size === 0) {
    for (const entries of Object.values(inventoryData)) {
      addCollection(counts, entries, includesItemType);
    }
  }
  return counts;
}

export function totalRelicInventoryCount(counts: ReadonlyMap<string, number>): number {
  let total = 0;
  for (const count of counts.values()) total += count;
  return total;
}
