import { useEffect, useState } from "react";
import { wfmPrice } from "./ipc";
import type { MessageKey } from "./i18n";
import type { CatalogIndex } from "./wfm/catalog";

export interface PriceState {
  messageKey: MessageKey | null;
  messageParams?: Record<string, string | number>;
  slug: string | null;
}

export interface PricePlan {
  name: string;
  isTradable: boolean;
  fallbackName?: string;
  fallbackTradable?: boolean;
}

const LOADING: PriceState = { messageKey: "market.loadingPrice", slug: null };

async function priceOfSlug(slug: string): Promise<PriceState> {
  try {
    const entry = await wfmPrice(slug);
    if (entry.status === "ok" && entry.price != null) return { messageKey: "market.priceAverage48h", messageParams: { plat: entry.price }, slug };
    return { messageKey: "market.noRecentPriceData", slug };
  } catch {
    return { messageKey: "market.priceLoadFailed", slug: null };
  }
}

/** WFM price for one name; the catalog maps the English name to the slug. */
async function loadItemPrice(catalog: CatalogIndex, name: string, isTradable: boolean): Promise<PriceState> {
  if (!isTradable) return { messageKey: "market.itemNotTradable", slug: null };
  const hit = catalog.byName(name);
  return hit ? priceOfSlug(hit.slug) : { messageKey: "market.noListingFound", slug: null };
}

/** Resolves a plan to a price line; the latest plan wins, an unloaded catalog shows "loading". */
export function useItemPrice(plan: PricePlan | null, catalog: CatalogIndex | null): PriceState {
  const [state, setState] = useState<PriceState>(LOADING);
  const key = plan ? `${plan.name}|${plan.isTradable}|${plan.fallbackName ?? ""}|${plan.fallbackTradable ?? ""}` : "";
  useEffect(() => {
    if (!plan || !catalog) return;
    let live = true;
    setState(LOADING);
    void (async () => {
      let result = await loadItemPrice(catalog, plan.name, plan.isTradable);
      if (!result.slug && plan.fallbackName) result = await loadItemPrice(catalog, plan.fallbackName, plan.fallbackTradable ?? plan.isTradable);
      if (live) setState(result);
    })();
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `key` stands for the plan's fields
  }, [key, catalog]);
  return state;
}
