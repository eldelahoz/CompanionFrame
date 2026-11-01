import { resolveDrops } from "./resolveDrops";
import { relicNameFromLabel } from "./relic/relicInventory";
import type { DropsItemDb } from "./relic/relicDropSources";
import type { RelicDatabase } from "./relic/types";
import type { ComponentInfo } from "../types/inventory";

export type ComponentState = "missing" | "partial" | "build" | "ready";

/** Pending first: Missing, then some-of-N, then blueprint held, then Ready. */
const ORDER: Record<ComponentState, number> = { missing: 0, partial: 1, build: 2, ready: 3 };

export const componentNeeded = (c: ComponentInfo) => c.itemCount || 1;
export const componentOwned = (c: ComponentInfo) => c.built ?? c.ownedCount ?? 0;

export function componentState(c: ComponentInfo): ComponentState {
  const owned = componentOwned(c);
  if (owned >= componentNeeded(c)) return "ready";
  if (c.blueprintHeld) return "build";
  return owned > 0 ? "partial" : "missing";
}

/** Stable sort, pending parts first. */
export function sortByState<T extends ComponentInfo>(comps: T[]): T[] {
  return comps.map((c, i) => ({ c, i, o: ORDER[componentState(c)] })).sort((a, b) => a.o - b.o || a.i - b.i).map((x) => x.c);
}

export const readyCount = (comps: ComponentInfo[]) => comps.filter((c) => componentState(c) === "ready").length;

export interface SourceHint {
  kind: "relic" | "drop" | "market";
  text: string;
}

/** Cheap source for a missing part from data already loaded (no drop-table lookup). */
export function sourceHint(c: ComponentInfo, db: DropsItemDb, relicDb: RelicDatabase | null): SourceHint | null {
  const first = resolveDrops({ drops: c.drops as never, uniqueName: c.uniqueName }, db, relicDb)[0];
  if (first) {
    const isRelic = /\bRelic\b/i.test(first.location);
    return { kind: isRelic ? "relic" : "drop", text: isRelic ? relicNameFromLabel(first.location) : first.location };
  }
  return c.tradable ? { kind: "market", text: "" } : null;
}
