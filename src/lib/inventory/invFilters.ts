import { useState } from "react";
import { rowPrice, type InvRow } from "./inventoryRows";
import type { PriceInfo } from "./itemPrices";

/** A bound left null is open. */
export interface PlatRange {
  min: number | null;
  max: number | null;
}

export type Goal = "all" | "sell" | "baro" | "mastery";
export const GOALS: readonly Goal[] = ["all", "sell", "baro", "mastery"];

/** What each goal turns on; everything else goes off. */
export interface GoalFilters {
  tradableOnly: boolean;
  ducatsOnly: boolean;
  masteryWant: "all" | "missing";
  range: PlatRange;
}
export const GOAL_FILTERS: Record<Goal, GoalFilters> = {
  all: { tradableOnly: false, ducatsOnly: false, masteryWant: "all", range: { min: null, max: null } },
  sell: { tradableOnly: true, ducatsOnly: false, masteryWant: "all", range: { min: 5, max: null } },
  baro: { tradableOnly: false, ducatsOnly: true, masteryWant: "all", range: { min: null, max: null } },
  mastery: { tradableOnly: false, ducatsOnly: false, masteryWant: "missing", range: { min: null, max: null } },
};

/** Select presets by key; labels are plain numbers, so they need no translation. */
export const PLAT_PRESETS: ReadonlyArray<readonly [string, string, PlatRange]> = [
  ["ge5", "≥ 5", { min: 5, max: null }],
  ["ge15", "≥ 15", { min: 15, max: null }],
  ["le5", "≤ 5", { min: null, max: 5 }],
  ["5-15", "5 - 15", { min: 5, max: 15 }],
  ["15-50", "15 - 50", { min: 15, max: 50 }],
  ["50+", "50+", { min: 50, max: null }],
];

/** "any" or a preset key, or null when the range is a custom one. */
export function presetKey({ min, max }: PlatRange): string | null {
  if (min == null && max == null) return "any";
  return PLAT_PRESETS.find(([, , r]) => r.min === min && r.max === max)?.[0] ?? null;
}

/** Empty or invalid text is an open bound. */
export function parseBound(text: string | null): number | null {
  if (text == null || text.trim() === "") return null;
  const n = Number(text);
  return Number.isFinite(n) && n >= 0 ? n : null;
}

/**
 * Whether the row's per-rank price sits in the range. With a bound set, rows with no market price
 * are out; rows whose price is still loading stay until it lands.
 */
export function passesPlatRange(row: InvRow, info: PriceInfo | undefined, { min, max }: PlatRange): boolean {
  if (min == null && max == null) return true;
  if (!row.slug) return false;
  if (!info || info.state === "loading") return true;
  const { price } = rowPrice(row, info);
  if (price == null) return false;
  return (min == null || price >= min) && (max == null || price <= max);
}

const MIN_KEY = "wf_inventory_plat_min";
const MAX_KEY = "wf_inventory_plat_max";
const OLD_FLOOR_KEY = "wf_inventory_value_floor";

function store(key: string, value: number | null) {
  try {
    if (value == null) localStorage.removeItem(key);
    else localStorage.setItem(key, String(value));
  } catch {
    // the preference is best-effort
  }
}

/** Persisted min and max; the old single floor is read as the min until a range is saved. */
export function usePersistedRange(): [PlatRange, (range: PlatRange) => void] {
  const [range, setRange] = useState<PlatRange>(() => {
    try {
      const min = parseBound(localStorage.getItem(MIN_KEY) ?? localStorage.getItem(OLD_FLOOR_KEY));
      return { min: min && min > 0 ? min : null, max: parseBound(localStorage.getItem(MAX_KEY)) };
    } catch {
      return { min: null, max: null };
    }
  });
  return [
    range,
    (next) => {
      setRange(next);
      store(MIN_KEY, next.min);
      store(MAX_KEY, next.max);
      try {
        localStorage.removeItem(OLD_FLOOR_KEY);
      } catch {
        // see above
      }
    },
  ];
}
