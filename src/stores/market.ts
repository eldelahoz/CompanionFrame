import { useEffect, useState } from "react";
import { getWfmItems } from "../lib/ipc";
import { buildCatalogIndex, type CatalogIndex } from "../lib/wfm/catalog";

let loading: Promise<CatalogIndex> | null = null;

/** One shared load; a failure is dropped so the next caller retries (the backend keeps the stale copy). */
export function loadCatalog(): Promise<CatalogIndex> {
  loading ??= getWfmItems()
    .then(buildCatalogIndex)
    .catch((e) => {
      loading = null;
      throw e;
    });
  return loading;
}

/** The warframe.market catalog index; null while loading or when it could not be loaded. */
export function useWfmCatalog(): { catalog: CatalogIndex | null; error: string | null } {
  const [state, setState] = useState<{ catalog: CatalogIndex | null; error: string | null }>({
    catalog: null,
    error: null,
  });
  useEffect(() => {
    let live = true;
    loadCatalog().then(
      (catalog) => live && setState({ catalog, error: null }),
      (e) => live && setState({ catalog: null, error: String(e?.message ?? e) }),
    );
    return () => {
      live = false;
    };
  }, []);
  return state;
}

export const resetCatalogForTest = (): void => {
  loading = null;
};
