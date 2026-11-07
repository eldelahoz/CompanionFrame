import { useMemo } from "react";
import { itemPrice, useItemPrices } from "../inventory/itemPrices";

/** Platinum per warframe.market slug from the shared price store, so the planner, the detail modal and the overlay share one lookup. */
export const relicPrice = itemPrice;

/** Asks the store for `slugs`; the returned value changes as prices land so callers recompute. */
export function useRelicPrices(slugs: readonly string[]): unknown {
  const key = slugs.join("|");
  // eslint-disable-next-line react-hooks/exhaustive-deps -- `key` stands for the list
  const stable = useMemo(() => slugs, [key]);
  return useItemPrices(stable);
}
