import { describe, expect, it } from "vitest";

import {
  buildRepriceRows,
  priceRepriceRow,
  repriceRowsToSend,
  repriceTotals,
} from "./repriceOrders";
import {
  DEFAULT_DAMPING_RULE,
  type PricingListing,
} from "../tradeWorkbench/pricingStrategies";
import type { MyOrder } from "../ipc";

function order(overrides: Partial<MyOrder> = {}): MyOrder {
  return {
    id: "a".repeat(24),
    orderType: "sell",
    platinum: 50,
    quantity: 1,
    visible: true,
    modRank: null,
    itemId: "item-1",
    itemName: "Fixture Item",
    itemSlug: "fixture_item",
    itemThumb: null,
    ...overrides,
  } as MyOrder;
}

function listing(platinum: number, userName = "someone"): PricingListing {
  return { platinum, quantity: 1, status: "ingame", userName };
}

describe("buildRepriceRows", () => {
  it("drops orders with no slug to price against", () => {
    const rows = buildRepriceRows([order(), order({ id: "b".repeat(24), itemSlug: null })]);
    expect(rows).toHaveLength(1);
    expect(rows[0].slug).toBe("fixture_item");
    expect(rows[0].currentPrice).toBe(50);
  });

  it("drops an order with no price, which cannot be repriced from", () => {
    expect(buildRepriceRows([order({ platinum: 0 })])).toHaveLength(0);
  });

  it("drops buy orders, which the sell-side strategies would misprice", () => {
    const rows = buildRepriceRows([
      order({ id: "b".repeat(24), orderType: "buy", platinum: 30 }),
      order(),
    ]);
    expect(rows).toHaveLength(1);
    expect(rows[0].order.orderType).toBe("sell");
  });

  it("selects every row, so an untouched modal still reprices everything", () => {
    const rows = buildRepriceRows([order(), order({ id: "b".repeat(24) })]);
    expect(rows.map((row) => row.selected)).toEqual([true, true]);
  });

  it("carries the listing subtype so the book is filtered to that refinement", () => {
    const rows = buildRepriceRows([order({ subtype: "radiant" }), order({ id: "c".repeat(24) })]);
    expect(rows[0].subtype).toBe("radiant");
    expect(rows[1].subtype).toBeNull();
  });
});

describe("priceRepriceRow", () => {
  const base = buildRepriceRows([order()])[0];

  it("refuses a buy order handed in from elsewhere", () => {
    const row = {
      ...base,
      order: order({ orderType: "buy" }),
      sellBook: [listing(175, "rival")],
    };
    const priced = priceRepriceRow(row, { id: "cheapest-minus-one" }, null);
    expect(priced.skipReason).toBe("not-sell");
    expect(priced.nextPrice).toBeNull();
    expect(repriceRowsToSend([priced])).toHaveLength(0);
  });

  it("marks a row with no book instead of pricing it blind", () => {
    const priced = priceRepriceRow(base, { id: "match-cheapest" }, null);
    expect(priced.skipReason).toBe("no-book");
    expect(priced.nextPrice).toBeNull();
  });

  it("never counts our own listing as the competition", () => {
    const cheap = buildRepriceRows([order({ platinum: 30 })])[0];
    const row = { ...cheap, sellBook: [listing(10, "me"), listing(44, "rival")] };
    const priced = priceRepriceRow(row, { id: "match-cheapest" }, "ME");
    expect(priced.nextPrice).toBe(44);
  });

  it("skips a row the strategy leaves where it is", () => {
    const row = { ...base, sellBook: [listing(50, "rival")] };
    const priced = priceRepriceRow(row, { id: "match-cheapest" }, null);
    expect(priced.skipReason).toBe("unchanged");
    expect(repriceRowsToSend([priced])).toHaveLength(0);
  });

  it("holds the price when too few listings sit below it", () => {
    const row = { ...base, sellBook: [listing(20, "rival")] };
    const priced = priceRepriceRow(row, { id: "match-cheapest" }, null);
    expect(priced.nextPrice).toBe(50);
    expect(priced.skipReason).toBe("unchanged");
  });

  it("follows the market down once enough listings are below", () => {
    const row = {
      ...base,
      sellBook: [listing(46), listing(47), listing(48), listing(49)],
    };
    const priced = priceRepriceRow(row, { id: "match-cheapest" }, null);
    expect(priced.nextPrice).toBe(46);
    expect(priced.skipReason).toBeNull();
  });

  it("prices a median strategy from its reference, not a lone 1p seller", () => {
    const row = { ...base, sellBook: [listing(1, "undercutter"), listing(58), listing(61)] };
    const config = { id: "median-90d", offsetPlat: -1 } as const;
    const priced = priceRepriceRow(row, config, null, DEFAULT_DAMPING_RULE, {
      median: 60,
      days: 40,
    });
    expect(priced.nextPrice).toBe(59);
    expect(priced.skipReason).toBeNull();
    expect(priceRepriceRow(row, config, null).skipReason).toBe("no-price");
  });
});

describe("repriceRowsToSend", () => {
  it("leaves an unselected row alone even when the strategy moved its price", () => {
    const base = buildRepriceRows([order()])[0];
    const priced = { ...base, nextPrice: 40, skipReason: null };

    expect(repriceRowsToSend([priced])).toHaveLength(1);
    expect(repriceRowsToSend([{ ...priced, selected: false }])).toHaveLength(0);
  });
});

describe("repriceTotals", () => {
  it("counts the rows a run would send and which way they move", () => {
    const base = buildRepriceRows([order()])[0];
    const up = { ...base, nextPrice: 60, skipReason: null };
    const down = { ...base, rowId: "down", nextPrice: 40, skipReason: null };
    const held = { ...base, rowId: "held", nextPrice: 50, skipReason: "unchanged" as const };

    expect(repriceTotals([up, down, held])).toEqual({
      rows: 3,
      sending: 2,
      raised: 1,
      lowered: 1,
      platinumDelta: 0,
    });
  });

  it("summarises the selection only, while the row count stays the whole list", () => {
    const base = buildRepriceRows([order()])[0];
    const up = { ...base, nextPrice: 60, skipReason: null };
    const down = { ...base, rowId: "down", nextPrice: 40, skipReason: null, selected: false };

    expect(repriceTotals([up, down])).toEqual({
      rows: 2,
      sending: 1,
      raised: 1,
      lowered: 0,
      platinumDelta: 10,
    });
  });
});
