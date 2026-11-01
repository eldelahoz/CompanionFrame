import { useSyncExternalStore } from "react";
import { readStorage, writeStorage } from "../lib/persistence";
import { DEFAULT_STAT_RESOURCE_IDS, STAT_RESOURCES } from "../lib/stats/resources";

const STORAGE_KEY = "wf_stats_chart_resources";

/** Keeps catalog order and drops ids this build no longer records, so a pref from a newer build cannot break the grid. */
const normalize = (ids: readonly string[]): string[] => {
  const wanted = new Set(ids);
  return STAT_RESOURCES.filter((r) => wanted.has(r.id)).map((r) => r.id);
};

// No stored pref is different from an empty one: the user may have switched every chart off.
function load(): string[] {
  try {
    const parsed: unknown = JSON.parse(readStorage(STORAGE_KEY) ?? "null");
    if (Array.isArray(parsed)) return normalize(parsed.filter((v): v is string => typeof v === "string"));
  } catch {
    // corrupt pref: fall through to the defaults
  }
  return normalize(DEFAULT_STAT_RESOURCE_IDS);
}

let current = load();
const listeners = new Set<() => void>();

export function setChartResources(ids: readonly string[]): void {
  current = normalize(ids);
  writeStorage(STORAGE_KEY, JSON.stringify(current));
  listeners.forEach((l) => l());
}

export const getChartResources = (): string[] => current;

export const toggleChartResource = (id: string): void =>
  setChartResources(current.includes(id) ? current.filter((x) => x !== id) : [...current, id]);

export const resetChartResources = (): void => setChartResources(DEFAULT_STAT_RESOURCE_IDS);

/** Resource ids the Stats tab charts, in catalog order. Recording ignores this. */
export const useChartResources = (): string[] =>
  useSyncExternalStore(
    (cb) => (listeners.add(cb), () => void listeners.delete(cb)),
    getChartResources,
  );
