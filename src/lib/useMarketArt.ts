import { useCallback, useMemo } from "react";
import { useItemDatabase } from "../stores/items";
import { useWfmCatalog } from "../stores/market";
import { gameRefKey } from "./marketNaming";

/** warframe.market slug -> the item DB's public art URL (via the catalog's gameRef), null when unknown. */
export function useMarketArt(): (slug: string | null | undefined) => string | null {
  const db = useItemDatabase();
  const { catalog } = useWfmCatalog();
  const byRef = useMemo(() => new Map(Object.entries(db).map(([u, e]) => [gameRefKey(u), e.imageUrl || e.fallbackImageUrl || null])), [db]);
  return useCallback(
    (slug) => {
      const ref = slug ? catalog?.bySlug(slug)?.gameRef : null;
      return (ref && byRef.get(gameRefKey(ref))) || null;
    },
    [catalog, byRef],
  );
}
