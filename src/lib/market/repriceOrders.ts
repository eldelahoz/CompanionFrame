import {
  DEFAULT_DAMPING_RULE,
  suggestPrice,
  type DampingRule,
  type MedianReference,
  type PricingListing,
  type StrategyConfig,
} from "../tradeWorkbench/pricingStrategies";
import type { MyOrder } from "../ipc";

export interface RepriceRow {
  rowId: string;
  order: MyOrder;
  label: string;
  slug: string;
  rank: number | null;
  subtype: string | null;
  currentPrice: number;
  sellBook: readonly PricingListing[] | null;
  nextPrice: number | null;
  skipReason: RepriceSkipReason | null;
  selected: boolean;
}

export type RepriceSkipReason = "no-book" | "no-price" | "unchanged" | "not-sell";

export function buildRepriceRows(orders: readonly MyOrder[]): RepriceRow[] {
  const rows: RepriceRow[] = [];
  for (const order of orders) {
    const slug = order.itemSlug;
    if (!slug || order.platinum <= 0 || order.orderType !== "sell") continue;
    rows.push({
      rowId: order.id,
      order,
      label: order.itemName || slug,
      slug,
      rank: order.modRank ?? null,
      subtype: order.subtype ?? null,
      currentPrice: order.platinum,
      sellBook: null,
      nextPrice: null,
      skipReason: null,
      selected: true,
    });
  }
  return rows;
}

export function priceRepriceRow(
  row: RepriceRow,
  config: StrategyConfig,
  ownUserName: string | null,
  rule: DampingRule = DEFAULT_DAMPING_RULE,
  median: MedianReference | null = null,
): RepriceRow {
  if (row.order.orderType !== "sell") {
    return { ...row, nextPrice: null, skipReason: "not-sell" };
  }
  if (!row.sellBook) return { ...row, nextPrice: null, skipReason: "no-book" };

  const suggestion = suggestPrice(
    config,
    {
      sellListings: row.sellBook,
      currentPrice: row.currentPrice,
      ownPerTrade: row.order.perTrade ?? 1,
      ownUserName,
      median,
    },
    rule,
  );
  if (suggestion.price === null) return { ...row, nextPrice: null, skipReason: "no-price" };
  if (suggestion.price === row.currentPrice) {
    return { ...row, nextPrice: suggestion.price, skipReason: "unchanged" };
  }
  return { ...row, nextPrice: suggestion.price, skipReason: null };
}

export function repriceRowsToSend(rows: readonly RepriceRow[]): RepriceRow[] {
  return rows.filter((row) => row.selected && row.skipReason === null && row.nextPrice !== null);
}

interface RepriceTotals {
  rows: number;
  sending: number;
  raised: number;
  lowered: number;
  platinumDelta: number;
}

export function repriceTotals(rows: readonly RepriceRow[]): RepriceTotals {
  const sending = repriceRowsToSend(rows);
  let raised = 0;
  let lowered = 0;
  let platinumDelta = 0;
  for (const row of sending) {
    const next = row.nextPrice;
    if (next === null) continue;
    if (next > row.currentPrice) raised += 1;
    else lowered += 1;
    platinumDelta += next - row.currentPrice;
  }
  return { rows: rows.length, sending: sending.length, raised, lowered, platinumDelta };
}
