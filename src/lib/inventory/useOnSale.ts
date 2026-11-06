import { useMemo } from "react";
import { type MyOrder } from "../ipc";
import { onSaleByItem } from "../marketOrderInventory";
import { wfmLookup } from "../market/wfmLookup";
import { useWfmCatalog } from "../../stores/market";
import { useMyOrders } from "../../stores/myOrders";
import type { ParsedItem } from "../../types/inventory";

/** My warframe.market sell orders joined to inventory rows, from the shared orders store; empty when signed out. */
export function useOnSale(loggedIn: boolean, parsed: ParsedItem[]): ReadonlyMap<ParsedItem, MyOrder> {
  const { catalog } = useWfmCatalog();
  const { orders } = useMyOrders();
  const sell = loggedIn ? orders.sell : undefined;
  return useMemo(() => (sell?.length ? onSaleByItem(sell, parsed, wfmLookup(catalog)) : new Map()), [sell, parsed, catalog]);
}
