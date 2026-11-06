import { describe, expect, it } from "vitest";
import type { BookOrder } from "../ipc";
import { rowBookOf } from "./rowBook";

const order = (type: string, platinum: number, rank: number | null, subtype: string | null = null): BookOrder =>
  ({ id: `${type}${platinum}`, type, platinum, quantity: 1, rank, subtype, visible: true, user: { ingameName: "Tenno", status: "ingame" } }) as unknown as BookOrder;

describe("rowBookOf", () => {
  it("splits one item's raw orders into the row's two sides", () => {
    const raw = [order("sell", 10, null), order("sell", 8, null), order("buy", 5, null)];
    const book = rowBookOf(raw, { rank: null, subtype: null });
    expect(book.sell.map((o) => o.platinum).sort((a, b) => a - b)).toEqual([8, 10]);
    expect(book.buy.map((o) => o.platinum)).toEqual([5]);
  });

  it("keeps a relic row to its own refinement", () => {
    const raw = [order("sell", 10, null, "radiant"), order("sell", 3, null, "intact")];
    const book = rowBookOf(raw, { rank: null, subtype: "radiant" });
    expect(book.sell.map((o) => o.platinum)).toEqual([10]);
  });
});
