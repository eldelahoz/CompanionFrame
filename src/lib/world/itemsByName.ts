import type { ItemDatabase } from "../ipc";

type Entry = ItemDatabase[string] & { uniqueName: string };
const cache = new WeakMap<ItemDatabase, Map<string, Entry>>();

/** English-name index of the item database; vendor stock arrives as names. Built once per database. */
export function itemsByName(db: ItemDatabase): Map<string, Entry> {
  const hit = cache.get(db);
  if (hit) return hit;
  const index = new Map(Object.entries(db).map(([uniqueName, entry]) => [entry.name, { ...entry, uniqueName }] as const));
  cache.set(db, index);
  return index;
}
