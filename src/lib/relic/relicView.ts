import { buildParsedItemFromDb } from "../parsedItemFromDb";
import { QUALITY_MODES } from "./relicConstants";
import type { ItemDbEntry, ParsedItem } from "../../types/inventory";
import type { RelicGroup, RelicQuality } from "./types";

export type RelicView = "simple" | "detailed";
/** "auto" keeps the popup each tab opens until the switch has been used once. */
export type RelicViewPreference = RelicView | "auto";
const RELIC_VIEW_PREFERENCES: readonly RelicViewPreference[] = ["auto", "simple", "detailed"];
const RELIC_VIEW_KEY = "relic.view";

/** A relic DE ships before its drops are listed has nothing to break down. */
export const relicHasRewards = (group: RelicGroup): boolean => QUALITY_MODES.some((q) => (group.qualities[q]?.rewards.length ?? 0) > 0);

/** The item popup for a relic: the preferred refinement, then the game's tier order. */
export function simpleRelicFor(group: RelicGroup, itemDb: Record<string, ItemDbEntry>, ownership: Map<string, number>, preferred?: RelicQuality): ParsedItem | null {
  for (const quality of preferred ? [preferred, ...QUALITY_MODES] : QUALITY_MODES) {
    const uniqueName = group.qualities[quality]?.uniqueName;
    const entry = uniqueName ? itemDb[uniqueName] : undefined;
    if (uniqueName && entry) return buildParsedItemFromDb(uniqueName, entry, ownership);
  }
  return null;
}

export function readRelicViewPreference(): RelicViewPreference {
  try {
    const stored = localStorage.getItem(RELIC_VIEW_KEY);
    return (RELIC_VIEW_PREFERENCES as readonly (string | null)[]).includes(stored) ? (stored as RelicViewPreference) : "auto";
  } catch {
    return "auto";
  }
}

export function writeRelicViewPreference(view: RelicView): void {
  try {
    localStorage.setItem(RELIC_VIEW_KEY, view);
  } catch {
    // the preference is best-effort
  }
}
