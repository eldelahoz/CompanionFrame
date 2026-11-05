import type { WfmItemsLookup } from "../../types/stats";
import type { useWfmCatalog } from "../../stores/market";

/** The catalog as the name-keyed lookup the inventory joins read. */
export function wfmLookup(catalog: ReturnType<typeof useWfmCatalog>["catalog"]): WfmItemsLookup {
  const lookup: WfmItemsLookup = {};
  for (const it of catalog?.items ?? []) lookup[it.name.toLowerCase()] = { url_name: it.slug, item_name: it.name, thumb: it.thumb, icon: it.icon, maxRank: it.maxRank, gameRef: it.gameRef };
  return lookup;
}
