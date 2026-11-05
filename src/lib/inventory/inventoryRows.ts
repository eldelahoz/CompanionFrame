import { isWfmExcludedSlug } from "../../../config/shared/wfmExclusions";
import type { ParsedItem } from "../../types/inventory";
import { itemLabel } from "../itemLabel";
import type { CatalogIndex } from "../wfm/catalog";
import { resolveQueueSlug, selectionKeyFor } from "../tradeWorkbench/queueModel";
import type { PriceInfo } from "./itemPrices";

/** An owned item with the market identity the price columns need. */
export interface InvRow {
  item: ParsedItem;
  key: string;
  /** Catalog-confirmed warframe.market slug, null when the item cannot be traded. */
  slug: string | null;
  /** Can be sold: has a slug and is not built equipment (the game binds a built frame or weapon). Price and slug stay for information. */
  canTrade: boolean;
  /** `slug` only when the catalog has art for it: the backend answers 404 for the rest, so the UI must not ask. */
  artSlug: string | null;
  /** Mod rank cap from the catalog; 0 for items without ranks (a weapon's level is not one). */
  maxRank: number;
  label: string;
}

/** The mod/arcane rank cap, 0 for items without ranks (a weapon or warframe level is not one). */
export const rankCap = (row: InvRow): number =>
  Math.max(row.maxRank || 0, row.item.inventoryGroup === "mods" || row.item.inventoryGroup === "arcanes" ? row.item.maxRank || 0 : 0);

/**
 * The 48 h price for this row's rank. warframe.market quotes only rank 0 and max rank: a maxed copy gets the max-rank
 * price (when it has one), every other rank the rank-0 price (`basis` "rank0" for the ranks in between).
 */
export function rowPrice(row: InvRow, info: PriceInfo | undefined): { price: number | null; basis: "plain" | "max" | "rank0" } {
  const cap = rankCap(row);
  const rank = row.item.rank || 0;
  if (cap > 0 && rank >= cap && info?.maxPrice != null) return { price: info.maxPrice, basis: "max" };
  return { price: info?.price ?? null, basis: cap > 0 && rank > 0 ? "rank0" : "plain" };
}

/** The 7-day median for the row's rank: the max-rank one for a max-rank copy, else rank 0's. */
export function rowAvg(row: InvRow, info: PriceInfo | undefined): number | null {
  const cap = rankCap(row);
  if (cap > 0 && (row.item.rank || 0) >= cap && info?.maxAvg != null) return info.maxAvg;
  return info?.avg ?? null;
}

/** Baro takes unbuilt Prime parts and blueprints for ducats, never built gear or sets. */
export const hasDucats = (row: InvRow): boolean => (row.item.ducats ?? 0) > 0 && row.item.inventoryGroup === "all_parts";

export type SortKey = "name" | "platinum" | "ducats" | "average" | "amount";
export const SORT_KEYS: readonly SortKey[] = ["name", "platinum", "ducats", "average", "amount"];

export function buildRows(items: readonly ParsedItem[], catalog: CatalogIndex | null): InvRow[] {
  return items.map((item) => {
    const slug = item.tradable === false || !catalog ? null : resolveQueueSlug(item, catalog);
    const ok = slug && !isWfmExcludedSlug(slug) ? slug : null;
    const entry = ok ? catalog?.bySlug(ok) : null;
    return { item, key: selectionKeyFor(item), slug: ok, canTrade: !!ok && item.inventoryGroup !== "equipment", artSlug: entry && (entry.thumb || entry.icon) ? ok : null, maxRank: entry?.maxRank || 0, label: itemLabel(item.internalName, item) };
  });
}

const stack = (row: InvRow): number => Math.max(1, Math.floor(row.item.amount ?? 1));

/** What each sort key orders by; null sorts last in either direction. */
function sortValue(row: InvRow, prices: ReadonlyMap<string, PriceInfo>, key: SortKey): number | string | null {
  const p = row.slug ? prices.get(row.slug) : undefined;
  switch (key) {
    case "name":
      return row.label;
    case "platinum":
      return rowPrice(row, p).price;
    case "average":
      return rowAvg(row, p);
    case "ducats":
      return row.item.ducats ?? null;
    case "amount":
      return stack(row);
  }
}

/** Null sorts last in either direction. */
function compareValues(x: number | string | null, y: number | string | null, dir: number): number {
  if (x == null || y == null) return x == null && y == null ? 0 : x == null ? 1 : -1;
  return (typeof x === "string" ? x.localeCompare(y as string) : x - (y as number)) * dir;
}

/** The rank copies of one mod stay adjacent, lowest rank first: a group sorts by its best row, then by name. */
let sortCalls = 0;
/** How many times `sortRows` has run; the timing logs diff two readings. */
export const sortRowsCalls = (): number => sortCalls;

export function sortRows(rows: readonly InvRow[], prices: ReadonlyMap<string, PriceInfo>, key: SortKey, desc: boolean): InvRow[] {
  sortCalls++;
  const dir = desc ? -1 : 1;
  const own = new Map(rows.map((r) => [r, sortValue(r, prices, key)] as const));
  const lead = new Map<string, number | string | null>();
  for (const r of rows) {
    const v = own.get(r)!;
    if (!lead.has(r.label) || compareValues(v, lead.get(r.label)!, dir) < 0) lead.set(r.label, v);
  }
  return [...rows].sort(
    (a, b) => compareValues(lead.get(a.label)!, lead.get(b.label)!, dir) || a.label.localeCompare(b.label) || (a.item.rank || 0) - (b.item.rank || 0),
  );
}

/** Rows shaped for `computeInventoryValueTotals`, with the live platinum price attached. */
export const valueEntries = (rows: readonly InvRow[], prices: ReadonlyMap<string, PriceInfo>) =>
  rows.map((row) => {
    const { item, slug } = row;
    return {
      inventoryGroup: item.inventoryGroup,
      partType: item.partType,
      isPrime: item.isPrime,
      tradable: item.tradable,
      amount: item.amount,
      marketSlug: slug,
      platinum: slug ? rowPrice(row, prices.get(slug)).price : null,
      ducats: item.ducats,
    };
  });
