/** A blueprint stays in Recipes until its foundry build is claimed, so raw
 *  counts include copies the game already spent. */

interface InventorySlices {
  Recipes?: unknown;
  PendingRecipes?: unknown;
}

function entryItemType(entry: unknown): string {
  if (!entry || typeof entry !== "object") return "";
  const value = (entry as { ItemType?: unknown }).ItemType;
  return typeof value === "string" ? value : "";
}

function entryItemCount(entry: unknown): number {
  const value = (entry as { ItemCount?: unknown }).ItemCount;
  return typeof value === "number" && Number.isFinite(value) ? Math.floor(value) : 1;
}

/** uniqueName -> builds the foundry is running for it, reusable ones included. */
function pendingRecipeCounts(pendingRecipes: unknown): Map<string, number> {
  const counts = new Map<string, number>();
  if (!Array.isArray(pendingRecipes)) return counts;
  for (const entry of pendingRecipes) {
    const itemType = entryItemType(entry);
    if (itemType) counts.set(itemType, (counts.get(itemType) || 0) + 1);
  }
  return counts;
}

/** Same inventory minus the foundry-committed Recipes copies. isReusable spares
 *  consumeOnUse=false blueprints, which survive their own build. */
export function withoutFoundryPending<T extends InventorySlices>(
  data: T,
  isReusable?: (uniqueName: string) => boolean,
): T {
  const pending = pendingRecipeCounts(data?.PendingRecipes);
  if (pending.size === 0 || !Array.isArray(data.Recipes)) return data;

  const kept: unknown[] = [];
  for (const entry of data.Recipes) {
    const itemType = entryItemType(entry);
    const spent = itemType && !isReusable?.(itemType) ? pending.get(itemType) || 0 : 0;
    if (spent === 0) {
      kept.push(entry);
      continue;
    }
    const held = entryItemCount(entry);
    // Later stacks of the same blueprint absorb whatever this one could not.
    pending.set(itemType, Math.max(0, spent - held));
    if (held > spent) kept.push({ ...(entry as object), ItemCount: held - spent });
  }

  return { ...data, Recipes: kept } as T;
}
