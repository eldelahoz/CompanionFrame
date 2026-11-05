import { expect, test } from "vitest";
import type { ParsedItem } from "../../types/inventory";
import type { ItemDbEntry } from "../../types/inventory";
import type { InvRow } from "./inventoryRows";
import { toSellable, upgradeVerdicts, verdictResolver } from "./sellable";
import { buildSafetyContext } from "./safetyRules";

const U = "/Lotus/Upgrades/Mods/Test/";
const db = { [`${U}A`]: { name: "A" }, [`${U}B`]: { name: "B" } } as unknown as Record<string, ItemDbEntry>;
const ctx = (locks: string[] = []) =>
  buildSafetyContext({ itemDb: db, masteredUniqueNames: new Set(), pinnedRequirements: new Map(), settings: { spareDefault: 0, spares: {}, locks, setKeep: [] } });

/** Sellable copies by row key, for the rows' items. */
const modSellable = (rows: readonly InvRow[], context: ReturnType<typeof ctx>) =>
  new Map([...upgradeVerdicts(rows.map((r) => r.item), context)].map(([k, v]) => [k, v.safe] as const));

const row = (id: string, rank: number, amount: number, over: Partial<ParsedItem> = {}): InvRow => {
  const internalName = `${U}${id}`;
  const key = `${internalName}#r${rank}m10`;
  return { item: { internalName, uniqueName: internalName, inventoryKey: key, inventoryGroup: "mods", rank, maxRank: 10, amount, tradable: true, ...over } as ParsedItem, key, slug: null, canTrade: true, artSlug: null, maxRank: 0, label: id };
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
  const part = row("A", 0, 2, { inventoryGroup: "misc" });
  const out = modSellable([a, b, part], ctx());
  expect([out.get(a.key), out.get(b.key), out.size]).toEqual([0, 2, 2]);
});

test("arcanes keep one copy too, on the highest rank", () => {
  const r0 = row("A", 0, 2, { inventoryGroup: "arcanes" });
  const r5 = row("A", 5, 1, { inventoryGroup: "arcanes" });
  const out = modSellable([r0, r5], ctx());
  expect([out.get(r0.key), out.get(r5.key)]).toEqual([2, 0]);
});

test("a mod and an arcane sharing a name are not mixed up by the group", () => {
  const mod = row("A", 0, 1);
  const arcane = row("B", 0, 2, { inventoryGroup: "arcanes" });
  const out = modSellable([mod, arcane], ctx());
  expect([out.get(mod.key), out.get(arcane.key)]).toEqual([0, 1]);
});

test("the resolver judges other rows with safeToList and names the kept reason", () => {
  const part = row("A", 0, 3, { inventoryGroup: "all_parts" });
  const mod = row("B", 0, 2);
  const verdictOf = verdictResolver([part.item, mod.item], ctx([part.item.internalName]));
  expect(toSellable(verdictOf(part.item))).toMatchObject({ count: 0, reasonKey: "inventory.safety.reason.locked" });
  expect(toSellable(verdictOf(mod.item))).toMatchObject({ count: 1, reasonKey: "inventory.safety.reason.lastCopy" });
});

test("a lock is per mod, so it holds every rank row", () => {
  const r0 = row("A", 0, 3);
  const r5 = row("A", 5, 1);
  const out = modSellable([r0, r5], ctx([r0.item.internalName]));
  expect([out.get(r0.key), out.get(r5.key)]).toEqual([0, 0]);
});
