import { rankCap, type InvRow } from "./inventoryRows";
import { isUpgrade } from "./sellable";

/** The three Mods and Arcanes chips; they AND with each other and with the other filters. */
export interface ModFilters {
  duplicates: boolean;
  notMax: boolean;
  notEquipped: boolean;
}

export const NO_MOD_FILTERS: ModFilters = { duplicates: false, notMax: false, notEquipped: false };

export const modFiltersOn = (f: ModFilters): boolean => f.duplicates || f.notMax || f.notEquipped;

/** Copies of each mod or arcane across all its rank rows, by base item. */
export function modCopies(rows: readonly InvRow[]): Map<string, number> {
  const copies = new Map<string, number>();
  for (const row of rows) {
    if (isUpgrade(row.item)) copies.set(row.item.internalName, (copies.get(row.item.internalName) ?? 0) + Math.max(1, Math.floor(row.item.amount ?? 1)));
  }
  return copies;
}

/** Rows that are neither a mod nor an arcane always pass. A row with no known rank cap counts as not max, so an unranked stack of a rankable mod stays. */
export function passesModFilters(row: InvRow, copies: ReadonlyMap<string, number>, f: ModFilters): boolean {
  if (!isUpgrade(row.item)) return true;
  if (f.duplicates && (copies.get(row.item.internalName) ?? 0) < 2) return false;
  if (f.notMax) {
    const cap = rankCap(row);
    if (cap > 0 && (row.item.rank || 0) >= cap) return false;
  }
  return !(f.notEquipped && row.item.equipped === true);
}
