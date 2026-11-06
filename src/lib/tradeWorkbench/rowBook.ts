import { normalizeSubtype, normalizeWfmOrderBookSide } from "../../../config/shared/wfmOrders";
import type { BookOrder } from "../ipc";

interface BookTarget {
  rank: number | null;
  subtype: string | null;
}

/** The books for one row out of its item's raw orders; a relic row reads only its own refinement's orders. */
export function rowBookOf(raw: BookOrder[], target: BookTarget) {
  const subtype = normalizeSubtype(target.subtype);
  const side = (s: "sell" | "buy") => normalizeWfmOrderBookSide(raw, s, target.rank, Number.MAX_SAFE_INTEGER, subtype);
  return { sell: side("sell"), buy: side("buy") };
}
