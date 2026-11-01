import { describe, expect, it } from "vitest";

import {
  ownedRelicQualities,
  relicGroupForDisplayName,
} from "./relicInventory";
import type { OwnedCounts, RelicDatabase, RelicGroup } from "./types";

function makeGroup(tier: string, code: string): RelicGroup {
  return {
    key: `${tier} ${code}`,
    name: `${tier} ${code}`,
    tier,
    code,
    vaulted: false,
    imageUrl: null,
    qualities: {},
  };
}

const db: RelicDatabase = {
  groups: {
    "Lith A1": makeGroup("Lith", "A1"),
    "Meso B2": makeGroup("Meso", "B2"),
    "Requiem I": makeGroup("Requiem", "I"),
  },
  byUniqueName: {},
};

describe("relicGroupForDisplayName", () => {
  it("resolves the drop-table spelling", () => {
    expect(relicGroupForDisplayName(db, "Lith A1 Relic")?.key).toBe("Lith A1");
    expect(relicGroupForDisplayName(db, "Requiem I Relic")?.key).toBe("Requiem I");
  });

  it("tolerates a missing suffix, odd spacing and case", () => {
    expect(relicGroupForDisplayName(db, "meso b2")?.key).toBe("Meso B2");
    expect(relicGroupForDisplayName(db, "  LITH   a1   relic ")?.key).toBe("Lith A1");
  });

  it("drops a refinement in parentheses in either position", () => {
    expect(relicGroupForDisplayName(db, "Lith A1 Relic (Radiant)")?.key).toBe("Lith A1");
    expect(relicGroupForDisplayName(db, "Lith A1 (Intact) Relic")?.key).toBe("Lith A1");
  });

  it("resolves the acquisition-list spellings and skips a plain location", () => {
    // The acquisition-list spellings DropsList feeds in.
    expect(relicGroupForDisplayName(db, "Lith A1 Relic (Intact)")?.key).toBe("Lith A1");
    expect(relicGroupForDisplayName(db, "Meso B2 Relic (Exceptional)")?.key).toBe("Meso B2");
    expect(relicGroupForDisplayName(db, "Meso B2 Relic (Flawless)")?.key).toBe("Meso B2");
    expect(relicGroupForDisplayName(db, "Meso B2")?.key).toBe("Meso B2");
    expect(relicGroupForDisplayName(db, "Grineer Settlement (Mars)")).toBeNull();
  });

  it("returns null for an unknown relic, empty input or no database", () => {
    expect(relicGroupForDisplayName(db, "Axi Z9 Relic")).toBeNull();
    expect(relicGroupForDisplayName(db, "Relic")).toBeNull();
    expect(relicGroupForDisplayName(db, "")).toBeNull();
    expect(relicGroupForDisplayName(null, "Lith A1 Relic")).toBeNull();
  });
});

describe("owned relic refinements", () => {
  const counts: OwnedCounts = {
    "Lith A1": { intact: 3, exceptional: 0, flawless: 0, radiant: 2 },
    "Meso B2": { intact: 0, exceptional: 0, flawless: 0, radiant: 0 },
  };

  it("lists only the refinements held, in the game's tier order", () => {
    expect(ownedRelicQualities(counts, "Lith A1")).toEqual([
      { quality: "intact", count: 3 },
      { quality: "radiant", count: 2 },
    ]);
  });

  it("says nothing for a relic held at no refinement or never seen", () => {
    expect(ownedRelicQualities(counts, "Meso B2")).toEqual([]);
    expect(ownedRelicQualities(counts, "Axi Z9")).toEqual([]);
    expect(ownedRelicQualities({}, "Lith A1")).toEqual([]);
  });
});
