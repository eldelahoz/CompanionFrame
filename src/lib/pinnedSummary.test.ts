import { expect, test } from "vitest";
import { computePinnedTotals } from "./pinnedSummary";

const ing = (uniqueName: string, count: number) => ({ uniqueName, count });
const bp = (uniqueName: string, ingredients: ReturnType<typeof ing>[], buildPrice = 0) => ({ source: "blueprint", uniqueName, ingredients, buildPrice });

test("sums needs across pinned blueprints once each and lists the shortfall", () => {
  const entries = [bp("a", [ing("x", 5), ing("y", 1)], 100), bp("b", [ing("x", 3)], 50), bp("a", [ing("x", 99)]), bp("c", [ing("z", 9)]), { ...bp("d", [ing("z", 1)]), source: "building" }];
  const totals = computePinnedTotals(entries, new Set(["a", "b", "d"]), (u) => (u === "x" ? 4 : 1));
  expect(totals.count).toBe(2);
  expect(totals.credits).toBe(150);
  expect(totals.resources).toEqual([{ uniqueName: "x", needed: 8, owned: 4 }, { uniqueName: "y", needed: 1, owned: 1 }]);
  expect(totals.missing.map((r) => r.uniqueName)).toEqual(["x"]);
});
