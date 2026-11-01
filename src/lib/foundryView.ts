import type { FoundryBuildingItem, FoundryRecipeItem, ItemDbEntry, MasteryStatus, RecipeIngredient } from "../types/inventory";
import { isFoundryBuildClaimable, isFoundryRecipeReady } from "./inventory/foundryResources";

export type ItemStatus = "claimable" | "in-progress" | "ready-to-build" | "not-ready";
export type FoundryState = "claimable" | "building" | "buildable" | "missing";
type ClaimMode = "all" | "claimable" | "not_ready" | "buildable" | "buildable_sets";
type YesNo = "all" | "yes" | "no";
export type SortKey = "count" | "time" | "name";

export interface FoundryEntry {
  source: "building" | "blueprint";
  name: string;
  displayName?: string;
  imageUrl: string | null;
  uniqueName: string | null;
  productUniqueName: string | null;
  category: string;
  ingredients: RecipeIngredient[];
  buildPrice: number;
  buildTime: number;
  count: number;
  endDate: Date | null;
  isIngredient: boolean;
}

export interface FoundryFilters {
  search: string;
  prime: "all" | "prime" | "non_prime";
  mastery: "all" | "mastered" | "not_mastered";
  claim: ClaimMode;
  vaulted: YesNo;
  subsumed: YesNo;
  sortBy: SortKey;
  sortDir: "asc" | "desc";
}

export const DEFAULT_FILTERS: FoundryFilters = { search: "", prime: "all", mastery: "all", claim: "all", vaulted: "all", subsumed: "all", sortBy: "count", sortDir: "desc" };
export const defaultSortDir = (key: SortKey): "asc" | "desc" => (key === "name" || key === "time" ? "asc" : "desc");

export const STATE_BY_STATUS: Record<ItemStatus, FoundryState> = { claimable: "claimable", "in-progress": "building", "ready-to-build": "buildable", "not-ready": "missing" };
const STATUS_RANK: Record<ItemStatus, number> = { claimable: 0, "in-progress": 1, "ready-to-build": 2, "not-ready": 3 };

export function toEntries(building: FoundryBuildingItem[], recipes: FoundryRecipeItem[]): FoundryEntry[] {
  const common = (i: FoundryBuildingItem | FoundryRecipeItem) => ({
    name: i.name,
    ...(i.displayName ? { displayName: i.displayName } : {}),
    imageUrl: i.imageUrl,
    uniqueName: i.uniqueName,
    productUniqueName: i.productUniqueName,
    category: (i.category || "").trim() || "Misc",
    ingredients: i.ingredients,
    buildPrice: i.buildPrice,
  });
  return [
    ...building.map((b): FoundryEntry => ({ ...common(b), source: "building", buildTime: 0, count: 0, endDate: b.endDate, isIngredient: false })),
    ...recipes.map((r): FoundryEntry => ({ ...common(r), source: "blueprint", buildTime: r.buildTime, count: r.count, endDate: null, isIngredient: r.isIngredient ?? false })),
  ];
}

export function statusOf(e: FoundryEntry, now: number, owned: ReadonlyMap<string, number>, chain: ReadonlySet<string>): ItemStatus {
  if (e.source === "building") return isFoundryBuildClaimable(e, now) ? "claimable" : "in-progress";
  return isFoundryRecipeReady(e, owned, chain) ? "ready-to-build" : "not-ready";
}

export interface Row {
  e: FoundryEntry;
  status: ItemStatus;
  mastery: MasteryStatus | "unknown";
  /** undefined = not a base Warframe, so it can never be subsumed. */
  subsumed: boolean | undefined;
  keywords: string[];
}

const has = (v: string | undefined, q: string) => typeof v === "string" && v.toLowerCase().includes(q);

export function matches(row: Row, f: FoundryFilters, db: Record<string, ItemDbEntry>): boolean {
  const { e } = row;
  const q = f.search.trim().toLowerCase();
  if (q && !(has(e.name, q) || has(e.displayName, q) || has(e.category, q) || row.keywords.some((k) => has(k, q)))) return false;
  const entry = e.productUniqueName ? db[e.productUniqueName] : undefined;
  const isPrime = entry?.isPrime === true || /\bprime\b/i.test(e.name);
  if (f.prime === "prime" && !isPrime) return false;
  if (f.prime === "non_prime" && isPrime) return false;
  if (f.mastery === "mastered" && row.mastery !== "mastered") return false;
  if (f.mastery === "not_mastered" && row.mastery === "mastered") return false;
  if (f.vaulted !== "all" && (entry?.vaulted === true) !== (f.vaulted === "yes")) return false;
  const state = STATE_BY_STATUS[row.status];
  if (f.claim === "claimable" && state !== "claimable") return false;
  if (f.claim === "buildable" && state !== "buildable") return false;
  if (f.claim === "buildable_sets" && (state !== "buildable" || Boolean(entry?.componentOf))) return false;
  if (f.claim === "not_ready" && (state === "claimable" || state === "buildable")) return false;
  if (f.subsumed !== "all" && row.subsumed !== (f.subsumed === "yes")) return false;
  return true;
}

const COLLATOR = new Intl.Collator();

export function sortRows(rows: Row[], f: FoundryFilters, pinned: ReadonlySet<string>, now: number): Row[] {
  const isPinned = (r: Row) => r.e.source === "blueprint" && !!r.e.uniqueName && pinned.has(r.e.uniqueName);
  const dir = f.sortDir === "asc" ? 1 : -1;
  const metric = (r: Row): number | null => (f.sortBy === "count" ? (r.e.source === "blueprint" ? r.e.count : null) : r.e.source === "building" && r.e.endDate ? Math.max(r.e.endDate.getTime() - now, 0) : null);
  return [...rows].sort((a, b) => {
    const pin = Number(isPinned(b)) - Number(isPinned(a));
    if (pin) return pin;
    const rank = STATUS_RANK[a.status] - STATUS_RANK[b.status];
    if (rank) return rank;
    if (f.sortBy !== "name") {
      const ma = metric(a);
      const mb = metric(b);
      if (ma !== mb) return ma === null ? 1 : mb === null ? -1 : dir * (ma - mb);
    }
    return (f.sortBy === "name" ? dir : 1) * COLLATOR.compare(a.e.name, b.e.name);
  });
}
