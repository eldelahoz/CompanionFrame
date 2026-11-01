import type { ItemDbEntry } from "../types/inventory";

/** Names the item-DB shape at the call site so tests need no `Record<string, ItemDbEntry>` annotation. */
export const itemDbOf = (entries: Record<string, ItemDbEntry>): Record<string, ItemDbEntry> => entries;

/** A bare item entry with no art, optionally carrying a recipe. */
export function entry(name: string, recipe?: ItemDbEntry["recipe"]): ItemDbEntry {
  return { name, imageUrl: null, ...(recipe ? { recipe } : {}) };
}
