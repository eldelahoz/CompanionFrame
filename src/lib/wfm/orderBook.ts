// Order book of one item for the Browse tab: the backend returns the raw public orders, the shared normaliser sorts and filters them.
import { normalizeWfmSlug } from "../../../config/shared/wfm";
import { normalizeSubtype, normalizeWfmOrderBookSide, type WfmOrderBookEntry } from "../../../config/shared/wfmOrders";
import { wfmOrdersItem } from "../ipc";

export type OrderBookEntry = WfmOrderBookEntry;

export interface ItemOrderBook {
  slug: string;
  sell: OrderBookEntry[];
  buy: OrderBookEntry[];
  timestamp: number;
}

export type ItemOrderBookResult =
  | { status: "ok"; data: ItemOrderBook }
  | { status: "not_found"; slug: string }
  | { status: "error"; slug: string };

/** Never throws; the backend's 400/404 means "no data", anything else is a retryable error. */
export async function fetchItemOrderBookBySlug(
  slug: string,
  opts: { rank?: number | null; subtype?: string | null } = {},
): Promise<ItemOrderBookResult> {
  const clean = normalizeWfmSlug(slug);
  if (!clean) return { status: "error", slug: "" };
  const rank = opts.rank ?? null;
  const subtype = normalizeSubtype(opts.subtype);
  try {
    const raw = await wfmOrdersItem(clean);
    // the full book is kept: the view pages through it
    const side = (s: "sell" | "buy") => normalizeWfmOrderBookSide(raw, s, rank, Number.MAX_SAFE_INTEGER, subtype);
    return { status: "ok", data: { slug: clean, sell: side("sell"), buy: side("buy"), timestamp: Date.now() } };
  } catch (e) {
    const { kind, status } = e as { kind?: string; status?: number };
    return kind === "api" && (status === 400 || status === 404) ? { status: "not_found", slug: clean } : { status: "error", slug: clean };
  }
}
