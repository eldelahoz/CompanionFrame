import { expect, test } from "vitest";
import type { ParsedItem } from "../../types/inventory";
import type { InvRow } from "./inventoryRows";
import { NO_MOD_FILTERS, modCopies, modFiltersOn, passesModFilters } from "./modFilters";

const row = (internalName: string, over: Partial<ParsedItem> = {}, group: ParsedItem["inventoryGroup"] = "mods"): InvRow =>
  ({ item: { internalName, inventoryGroup: group, rank: 0, maxRank: 10, amount: 1, ...over } as ParsedItem, key: internalName, slug: null, canTrade: false, artSlug: null, maxRank: 0, label: internalName });

const on = (over: Partial<typeof NO_MOD_FILTERS>) => ({ ...NO_MOD_FILTERS, ...over });

test("copies add up across rank rows of one mod and count arcanes too, but ignore other groups", () => {
  const copies = modCopies([row("a", { amount: 2 }), row("a", { rank: 3 }), row("b"), row("c", {}, "arcanes"), row("d", {}, "misc")]);
  expect([copies.get("a"), copies.get("b"), copies.get("c"), copies.has("d")]).toEqual([3, 1, 1, false]);
});

test("duplicates needs two copies in total, not two in one row", () => {
  const rows = [row("a"), row("a", { rank: 4 }), row("b")];
  const copies = modCopies(rows);
  expect(rows.map((r) => passesModFilters(r, copies, on({ duplicates: true })))).toEqual([true, true, false]);
});

test("not max rank hides a row at its cap and keeps an unranked stack", () => {
  const copies = modCopies([]);
  const f = on({ notMax: true });
  expect(passesModFilters(row("a", { rank: 10 }), copies, f)).toBe(false);
  expect(passesModFilters(row("a", { rank: 9 }), copies, f)).toBe(true);
  expect(passesModFilters(row("a", { rank: 0, amount: 5 }), copies, f)).toBe(true);
  expect(passesModFilters({ ...row("a", { rank: 3, maxRank: 0 }), maxRank: 3 }, copies, f)).toBe(false);
});

test("not equipped hides equipped rows only", () => {
  const f = on({ notEquipped: true });
  expect(passesModFilters(row("a", { equipped: true }), new Map(), f)).toBe(false);
  expect(passesModFilters(row("a"), new Map(), f)).toBe(true);
});

test("arcanes follow the chips like mods", () => {
  const a = row("x", { equipped: true, rank: 5 }, "arcanes");
  expect([passesModFilters(a, new Map([["x", 1]]), on({ duplicates: true })), passesModFilters(a, new Map(), on({ notEquipped: true }))]).toEqual([false, false]);
});

test("non-mod rows always pass, and no chip passes everything", () => {
  const all = on({ duplicates: true, notMax: true, notEquipped: true });
  expect(passesModFilters(row("x", { equipped: true, rank: 5 }, "misc"), new Map(), all)).toBe(true);
  expect(passesModFilters(row("a", { equipped: true }), new Map(), NO_MOD_FILTERS)).toBe(true);
  expect([modFiltersOn(NO_MOD_FILTERS), modFiltersOn(on({ notMax: true }))]).toEqual([false, true]);
});
