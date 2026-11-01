import { expect, test } from "vitest";
import { GOAL_FILTERS, parseBound, passesPlatRange, presetKey } from "./invFilters";
import type { InvRow } from "./inventoryRows";
import type { PriceInfo } from "./itemPrices";

const row = (slug: string | null = "x") => ({ slug, item: { rank: 0 } }) as unknown as InvRow;
const info = (price: number | null, state: PriceInfo["state"] = "ok"): PriceInfo => ({ state, price, maxPrice: null, avg: null, maxAvg: null, avgState: "ok" });
const pass = (price: number | null, min: number | null, max: number | null) => passesPlatRange(row(), info(price), { min, max });

test("at most keeps the cheap ones and the bound itself", () => {
  expect([3, 5, 6].map((p) => pass(p, null, 5))).toEqual([true, true, false]);
});

test("between is inclusive on both ends", () => {
  expect([4, 5, 15, 16].map((p) => pass(p, 5, 15))).toEqual([false, true, true, false]);
});

test("a custom range with only a max leaves the min open", () => {
  expect([0.5, 40].map((p) => pass(p, null, 10))).toEqual([true, false]);
});

test("with a bound set, rows with no market price or no slug are hidden, and loading ones stay", () => {
  expect(passesPlatRange(row(), info(null, "none"), { min: 5, max: null })).toBe(false);
  expect(passesPlatRange(row(null), info(9), { min: null, max: 20 })).toBe(false);
  expect(passesPlatRange(row(), info(null, "loading"), { min: 5, max: null })).toBe(true);
  expect(passesPlatRange(row(), undefined, { min: null, max: 5 })).toBe(true);
  expect(passesPlatRange(row(null), undefined, { min: null, max: null })).toBe(true);
});

test("presets are found by range and anything else is custom", () => {
  expect(presetKey({ min: null, max: null })).toBe("any");
  expect(presetKey({ min: 5, max: 15 })).toBe("5-15");
  expect(presetKey({ min: null, max: 5 })).toBe("le5");
  expect(presetKey({ min: 7, max: null })).toBeNull();
  expect(GOAL_FILTERS.sell.range).toEqual({ min: 5, max: null });
});

test("bounds parse to numbers, and blanks or junk are open", () => {
  expect([parseBound("12"), parseBound(""), parseBound(null), parseBound("-3"), parseBound("abc")]).toEqual([12, null, null, null, null]);
});
