import { expect, test } from "vitest";
import type { ParsedItem } from "../../types/inventory";
import type { RelicDatabase, RelicGroup } from "../relic/types";
import { buildIncompleteSets, buildRelicRewardIndex } from "./incompleteSets";

const group = (key: string, vaulted: boolean, rewards: Array<[string, string]>): RelicGroup => ({
  key,
  name: key,
  tier: key.split(" ")[0],
  code: key.split(" ")[1],
  vaulted,
  qualities: { intact: { uniqueName: `/p/${key}`, rewards: rewards.map(([name, rarity]) => ({ name, rarity, chance: 10 })) } },
});
const db = {
  groups: Object.fromEntries(
    [
      group("Lith A1", true, [["Ash Prime Systems", "Rare"]]),
      group("Meso B2", false, [["Ash Prime Systems", "Uncommon"], ["Ash Prime Chassis", "Common"]]),
      group("Neo C3", false, [["Ash Prime Systems", "Common"]]),
    ].map((g) => [g.key, g]),
  ),
  byUniqueName: {},
} as unknown as RelicDatabase;

const part = (name: string, ownedCount: number, itemCount = 1) => ({ name, uniqueName: `/r/Ash${name}`, ownedCount, itemCount, owned: ownedCount >= itemCount });
const set = (name: string, parts: ReturnType<typeof part>[]) =>
  ({ name, internalName: `${name}#set`, inventoryGroup: "incomplete_sets", components: parts, totalPartTypes: parts.length }) as unknown as ParsedItem;

const index = buildRelicRewardIndex(db);
const owned = { "Neo C3": { intact: 2, exceptional: 0, flawless: 0, radiant: 1 } };

test("lists the missing parts of a set, with owned parts left out", () => {
  const [row] = buildIncompleteSets([set("Ash Prime Set", [part("Chassis", 1), part("Systems", 0), part("Neuroptics", 0)])], index, owned);
  expect(row.owned.map((p) => p.name)).toEqual(["Chassis"]);
  expect(row.missing.map((m) => m.part.name)).toEqual(["Systems", "Neuroptics"]);
});

test("relic sources carry rarity and vaulted, owned relics first", () => {
  const [row] = buildIncompleteSets([set("Ash Prime Set", [part("Systems", 0)])], index, owned);
  const sources = row.missing[0].sources!;
  expect(sources.map((s) => [s.group.key, s.rarity, s.group.vaulted])).toEqual([
    ["Neo C3", "Common", false],
    ["Meso B2", "Uncommon", false],
    ["Lith A1", "Rare", true],
  ]);
  expect(sources[0].ownedQualities).toEqual([{ quality: "intact", count: 2 }, { quality: "radiant", count: 1 }]);
});

test("a part no relic drops has an empty source list, and none before the database loads", () => {
  const sets = [set("Ash Prime Set", [part("Neuroptics", 0)])];
  expect(buildIncompleteSets(sets, index, owned)[0].missing[0].sources).toEqual([]);
  expect(buildIncompleteSets(sets, null, owned)[0].missing[0].sources).toBeNull();
});

test("closest to complete first, then the pricier set", () => {
  const a = set("A Set", [part("X", 1), part("Y", 0), part("Z", 0)]);
  const b = set("B Set", [part("X", 1), part("Y", 1), part("Z", 0)]);
  const c = set("C Set", [part("X", 1), part("Y", 0), part("Z", 0)]);
  const price = (s: ParsedItem) => (s === c ? 50 : 0);
  expect(buildIncompleteSets([a, b, c], index, owned, price).map((r) => r.set.name)).toEqual(["B Set", "C Set", "A Set"]);
});