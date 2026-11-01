import { makeItemRefResolver } from "./tradeAnalytics.js";
import { relicGroupForDisplayName } from "../relic/relicInventory.js";
import type { RelicDatabase } from "../relic/types.js";
import type { ItemDbLookup, WfmItemsLookup } from "../../types/stats.js";

export interface AnalyticsItemLink {
  resolve: (key: string, name: string) => string | null;
  open: (uniqueName: string) => void;
}

/** An analytics row's key may be a market slug, an internal name or a display
 *  name, so it goes to the resolver as all three. */
export function createAnalyticsItemResolver(
  itemDb: ItemDbLookup,
  wfmItems: WfmItemsLookup,
  relicDb: RelicDatabase | null,
): AnalyticsItemLink["resolve"] {
  const resolveRef = makeItemRefResolver(itemDb, wfmItems);
  return (key, name) => {
    const direct = resolveRef({
      internalName: (key ?? "").trim(),
      wfmSlug: (key ?? "").trim(),
      displayName: (name ?? "").trim(),
    });
    if (direct) return direct;
    // warframe.market sells one listing per relic while DE keys every refinement
    // with a metal suffix, so a relic gameRef is never an item database key.
    return relicGroupForDisplayName(relicDb, name ?? "")?.qualities.intact?.uniqueName ?? null;
  };
}
