import { describe, expect, it } from "vitest";

import { buildFullSetItems } from "./fullSets.js";
import type { ItemDbEntry } from "../../types/inventory.js";

const ROOT = "/Lotus/Weapons/Tenno/Pistol/AkfooPrime";
const BLUEPRINT = "/Lotus/Types/Recipes/Weapons/AkfooPrimeBlueprint";
const LINK = "/Lotus/Types/Recipes/Weapons/WeaponParts/AkfooPrimeLink";

const db: Record<string, ItemDbEntry> = {
  [ROOT]: {
    name: "Akfoo Prime",
    category: "Pistols",
    type: "Pistol",
    isPrime: true,
    tradable: true,
    components: [
      { name: "Blueprint", uniqueName: BLUEPRINT, itemCount: 1, tradable: true },
      { name: "Link", uniqueName: LINK, itemCount: 1, tradable: true },
      { name: "Link", uniqueName: LINK, itemCount: 1, tradable: true },
    ],
  } as unknown as ItemDbEntry,
  [BLUEPRINT]: {
    name: "Akfoo Prime Blueprint",
    isBuildComponent: true,
    tradable: true,
  } as ItemDbEntry,
  [LINK]: { name: "Akfoo Prime Link", isBuildComponent: true, tradable: true } as ItemDbEntry,
};

describe("buildFullSetItems", () => {
  it("merges a doubled part so one owned copy does not cover both rows", () => {
    const rows = buildFullSetItems(
      db,
      new Map([
        [BLUEPRINT, 1],
        [LINK, 1],
      ]),
    );
    const set = rows.find((row) => row.internalName.startsWith(ROOT));
    expect(set?.category).toBe("incomplete_sets");
    const link = set?.components.find((component) => component.uniqueName === LINK);
    expect(set?.components).toHaveLength(2);
    expect(link?.itemCount).toBe(2);
    expect(link?.ownedCount).toBe(1);
    expect(link?.owned).toBe(false);
  });

  it("counts a full set once both links are there", () => {
    const rows = buildFullSetItems(
      db,
      new Map([
        [LINK, 2],
        [BLUEPRINT, 1],
      ]),
    );
    const set = rows.find((row) => row.internalName.startsWith(ROOT));
    expect(set?.category).toBe("full_sets");
    expect(set?.amount).toBe(1);
  });
});
