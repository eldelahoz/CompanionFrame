import { expect, test } from "vitest";
import type { ParsedItem } from "../../types/inventory";
import { buildCatalogIndex } from "../wfm/catalog";
import { buildRows, hasDucats } from "./inventoryRows";

const catalog = buildCatalogIndex(
  ["volt_prime_set", "ash_prime_systems_blueprint", "primed_flow", "magus_lockdown", "axi_a1_relic"].map((slug, i) => ({ id: String(i), slug, name: slug.replace(/_/g, " ") }) as never),
);
const mk = (name: string, inventoryGroup: ParsedItem["inventoryGroup"], extra: Partial<ParsedItem> = {}) =>
  ({ name, internalName: `/Lotus/${name}`, inventoryGroup, tradable: true, ...extra }) as ParsedItem;
const rowOf = (item: ParsedItem) => buildRows([item], catalog)[0]!;

test("built gear keeps its slug but cannot be sold; parts, mods, arcanes and relics can", () => {
  const frame = rowOf(mk("Volt Prime Set", "equipment"));
  expect(frame.slug).toBe("volt_prime_set");
  expect(frame.canTrade).toBe(false);
  expect(rowOf(mk("Ash Prime Systems Blueprint", "all_parts")).canTrade).toBe(true);
  expect(rowOf(mk("Primed Flow", "mods")).canTrade).toBe(true);
  expect(rowOf(mk("Magus Lockdown", "arcanes")).canTrade).toBe(true);
  expect(rowOf(mk("Axi A1 Relic", "relics")).canTrade).toBe(true);
});

test("without a slug nothing is tradable", () => {
  expect(rowOf(mk("Unknown", "all_parts")).canTrade).toBe(false);
});

test("ducats only count for unbuilt parts", () => {
  expect(hasDucats(rowOf(mk("Ash Prime Systems Blueprint", "all_parts", { ducats: 45 })))).toBe(true);
  expect(hasDucats(rowOf(mk("Volt Prime Set", "equipment", { ducats: 45 })))).toBe(false);
  expect(hasDucats(rowOf(mk("Ash Prime Systems Blueprint", "all_parts", { ducats: 0 })))).toBe(false);
});
