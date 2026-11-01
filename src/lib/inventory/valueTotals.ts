import { toFiniteNumber } from "../../../config/shared/numeric";
import { isWfmExcludedSlug } from "../../../config/shared/wfmExclusions";
import type { InventoryGroup, PartType } from "../../types/inventory";

/** "prime" is the sellable prime stock; "tradable" widens to every listed row. */
export type InventoryValueScope = "prime" | "tradable";

/** Structural row shape. `InventoryBaseItem` and `InventoryViewItem` satisfy it. */
interface InventoryValueEntry {
  inventoryGroup?: InventoryGroup | null;
  partType?: PartType | null;
  isPrime?: boolean;
  tradable?: boolean;
  amount?: number | null;
  marketSlug?: string | null;
  platinum?: number | null;
  ducats?: number | null;
}

export interface InventoryValueTotals {
  platinum: number;
  ducats: number;
  /** Counted rows with no snapshot median, so the platinum total is a floor. */
  platinumUnpriced: number;
  /** Counted prime rows with no ducat value, so the ducat total is a floor. */
  ducatsUnpriced: number;
  /** Counted rows missing either price, so the strip can hint once instead of twice. */
  unpriced: number;
  counted: number;
}

// Set rows aggregate parts that are already counted one by one, so including
// them would double the same stock.
const AGGREGATE_GROUPS: ReadonlySet<string> = new Set(["full_sets", "incomplete_sets"]);

function isPrimeRow(entry: InventoryValueEntry): boolean {
  return entry.partType === "prime" || entry.isPrime === true;
}

function isSellable(entry: InventoryValueEntry): boolean {
  if (entry.tradable === false) return false;
  if (AGGREGATE_GROUPS.has(entry.inventoryGroup ?? "")) return false;
  const slug = entry.marketSlug;
  if (typeof slug !== "string" || slug.length === 0) return false;
  return !isWfmExcludedSlug(slug);
}

function stackSize(entry: InventoryValueEntry): number {
  if (entry.amount == null) return 1;
  const amount = toFiniteNumber(entry.amount);
  if (amount == null || amount <= 0) return 0;
  // Stock the payload reports as a fraction is still stock, so the floor below
  // one copy must not drop the row out of the totals and out of the hint.
  return Math.max(1, Math.floor(amount));
}

/**
 * Scope + sellable gate. Exported so callers can prefilter cheap base rows
 * before paying for the priced view rows.
 */
function isCountedForValue(entry: InventoryValueEntry, scope: InventoryValueScope): boolean {
  if (!isSellable(entry)) return false;
  if (scope === "tradable") return true;
  // The default scope is the prime stock you actually dump: parts, not built gear.
  return entry.inventoryGroup === "all_parts" && isPrimeRow(entry);
}

/**
 * The floor is per-unit, so a huge stack of a 4p mod is still 4p. An unpriced
 * row is unknown rather than cheap and stays counted, feeding the ">=" hint.
 */
function passesPlatinumFloor(entry: InventoryValueEntry, minPlatinum: number): boolean {
  if (!(minPlatinum > 0)) return true;
  const median = toFiniteNumber(entry.platinum);
  if (median == null || median <= 0) return true;
  return median >= minPlatinum;
}

export function computeInventoryValueTotals(
  entries: readonly InventoryValueEntry[],
  scope: InventoryValueScope,
  minPlatinum = 0,
): InventoryValueTotals {
  let platinum = 0;
  let ducats = 0;
  let platinumUnpriced = 0;
  let ducatsUnpriced = 0;
  let unpriced = 0;
  let counted = 0;

  for (const entry of entries) {
    if (!isCountedForValue(entry, scope)) continue;
    // Below the floor the whole row drops, ducats included, so both figures
    // describe the same stock.
    if (!passesPlatinumFloor(entry, minPlatinum)) continue;
    const quantity = stackSize(entry);
    if (quantity <= 0) continue;
    counted++;
    let hole = false;

    const median = toFiniteNumber(entry.platinum);
    if (median != null && median > 0) platinum += median * quantity;
    else {
      platinumUnpriced++;
      hole = true;
    }

    // Only prime parts have a Baro price, so a null anywhere else is not a hole.
    if (isPrimeRow(entry)) {
      const ducatValue = toFiniteNumber(entry.ducats);
      if (ducatValue != null && ducatValue > 0) ducats += ducatValue * quantity;
      else {
        ducatsUnpriced++;
        hole = true;
      }
    }

    if (hole) unpriced++;
  }

  return {
    platinum: Math.round(platinum),
    ducats: Math.round(ducats),
    platinumUnpriced,
    ducatsUnpriced,
    unpriced,
    counted,
  };
}
