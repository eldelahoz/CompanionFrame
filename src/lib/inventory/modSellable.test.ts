import { expect, test } from "vitest";
import type { ParsedItem } from "../../types/inventory";
import type { ItemDbEntry } from "../../types/inventory";
import type { InvRow } from "./inventoryRows";
import { modSellable } from "./modSellable";
import { buildSafetyContext } from "./safetyRules";

const U = "/Lotus/Upgrades/Mods/Test/";
const db = { [`${U}A`]: { name: "A" }, [`${U}B`]: { name: "B" } } as unknown as Record<string, ItemDbEntry>;
const ctx = (locks: string[] = []) =>
  buildSafetyContext({ itemDb: db, masteredUniqueNames: new Set(), pinnedRequirements: new Map(), settings: { spareDefault: 0, spares: {}, locks, setKeep: [] } });

const row = (id: string, rank: number, amount: number, over: Partial<ParsedItem> = {}): InvRow => {
  const internalName = `${U}${id}`;
  const key = `${internalName}#r${rank}m10`;
  return { item: { internalName, uniqueName: internalName, inventoryGroup: "mods", rank, maxRank: 10, amount, tradable: true, ...over } as ParsedItem, key, slug: null, canTrade: true, artSlug: null, maxRank: 0, label: id };
};

test("two copies of a lone mod keep one", () => {
  const r = row("A", 0, 2);
  expect(modSellable([r], ctx()).get(r.key)).toBe(1);
});

test("a lone R3 copy is kept", () => {
  const r = row("A", 3, 1);
  expect(modSellable([r], ctx()).get(r.key)).toBe(0);
});

test("an equipped row of three sells two, since one copy is fitted", () => {
  const r = row("A", 0, 3, { equipped: true });
  expect(modSellable([r], ctx()).get(r.key)).toBe(2);
});

test("an equipped higher rank is the kept copy, so the whole unranked stack sells", () => {
  const r0 = row("A", 0, 4);
  const r5 = row("A", 5, 1, { equipped: true });
  const out = modSellable([r0, r5], ctx());
  expect([out.get(r0.key), out.get(r5.key)]).toEqual([4, 0]);
});

test("with no other kept copy, the kept copy sits on the highest rank only", () => {
  const r0 = row("A", 0, 2);
  const r5 = row("A", 5, 1);
  const out = modSellable([r0, r5], ctx());
  expect([out.get(r0.key), out.get(r5.key)]).toEqual([2, 0]);
});

test("mods are judged apart and other groups are skipped", () => {
  const a = row("A", 0, 1);
  const b = row("B", 0, 3);
  const arcane = row("A", 0, 2, { inventoryGroup: "arcanes" });
  const out = modSellable([a, b, arcane], ctx());
  expect([out.get(a.key), out.get(b.key), out.size]).toEqual([0, 2, 2]);
});

test("a lock is per mod, so it holds every rank row", () => {
  const r0 = row("A", 0, 3);
  const r5 = row("A", 5, 1);
  const out = modSellable([r0, r5], ctx([r0.item.internalName]));
  expect([out.get(r0.key), out.get(r5.key)]).toEqual([0, 0]);
});
