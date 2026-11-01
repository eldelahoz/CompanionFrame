import { describe, expect, it } from "vitest";

import {
  collectRelicInventoryCounts,
  totalRelicInventoryCount,
} from "./relicCounts";

const RELIC = "/Lotus/Relics/LithA1Intact";

describe("relic inventory counts", () => {
  it("uses the largest duplicate count across inventory collections", () => {
    const counts = collectRelicInventoryCounts({
      LevelKeys: [{ ItemType: RELIC, ItemCount: 5 }],
      MiscItems: [{ ItemType: RELIC, ItemCount: 7 }],
    });

    expect(counts.get(RELIC)).toBe(7);
    expect(totalRelicInventoryCount(counts)).toBe(7);
  });

  it("sums split stacks within one collection before deduplicating", () => {
    const counts = collectRelicInventoryCounts({
      LevelKeys: [{ ItemType: RELIC, ItemCount: 5 }],
      MiscItems: [
        { ItemType: RELIC, ItemCount: 3 },
        { ItemType: RELIC, ItemCount: 4 },
      ],
    });

    expect(counts.get(RELIC)).toBe(7);
  });

  it.each([
    ["missing", undefined, 1],
    ["zero", 0, 0],
    ["fractional", 2.9, 2],
    ["negative", -3, 0],
    ["not finite", Number.POSITIVE_INFINITY, 0],
    ["not numeric", "4", 0],
  ])("normalizes a %s count", (_label, count, expected) => {
    const counts = collectRelicInventoryCounts({
      LevelKeys: [{ ItemType: RELIC, ItemCount: count }],
    });

    expect(counts.get(RELIC)).toBe(expected);
  });

  it("uses the legacy array fallback when named collections have no relics", () => {
    const counts = collectRelicInventoryCounts({
      LevelKeys: [],
      LegacyRelics: [{ ItemType: RELIC, ItemCount: 3 }],
    });

    expect(totalRelicInventoryCount(counts)).toBe(3);
  });
});
