import { useEffect, useMemo, useState } from "react";
import { wfmOrdersMy, type MyOrder } from "../ipc";
import { onSaleByItem } from "../marketOrderInventory";
import { wfmLookup } from "../market/wfmLookup";
import { useWfmCatalog } from "../../stores/market";
import type { ParsedItem } from "../../types/inventory";

/** My warframe.market sell orders joined to inventory rows. Fetched on mount and when the session changes; empty when signed out. */
export function useOnSale(loggedIn: boolean, parsed: ParsedItem[]): ReadonlyMap<ParsedItem, MyOrder> {
  const { catalog } = useWfmCatalog();
  const [sell, setSell] = useState<MyOrder[]>([]);
  useEffect(() => {
    if (!loggedIn) return setSell([]);
    let dead = false;
    wfmOrdersMy().then((o) => !dead && setSell(o.sell), () => !dead && setSell([]));
    return () => {
      dead = true;
    };
  }, [loggedIn]);
  return useMemo(() => (sell.length ? onSaleByItem(sell, parsed, wfmLookup(catalog)) : new Map()), [sell, parsed, catalog]);
}
