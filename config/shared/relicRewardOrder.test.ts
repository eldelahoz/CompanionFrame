import { describe, expect, it } from "vitest";

import { sortRelicRewards } from "./relicRewardOrder";

describe("relic reward order", () => {
  it("reads common to rare, then by descending chance inside a rarity", () => {
    const rewards = [
      { name: "rare", rarity: "Rare", chance: 2 },
      { name: "common-low", rarity: "common", chance: 20 },
      { name: "uncommon", rarity: "Uncommon", chance: 11 },
      { name: "common-high", rarity: "Common", chance: 25 },
    ];

    expect(sortRelicRewards(rewards).map((reward) => reward.name)).toEqual([
      "common-high",
      "common-low",
      "uncommon",
      "rare",
    ]);
  });

  it("leaves the source alone and parks an unknown rarity past the rare", () => {
    const rewards = [
      { name: "mystery", rarity: null, chance: 50 },
      { name: "rare", rarity: "Rare", chance: 2 },
      { name: "common", rarity: "Common", chance: 25 },
    ];
    const sorted = sortRelicRewards(rewards);

    expect(sorted.map((reward) => reward.name)).toEqual(["common", "rare", "mystery"]);
    expect(rewards[0].name).toBe("mystery");
  });

  it("reads rare to common for the planner overlay, unknown still last", () => {
    const rewards = [
      { name: "common", rarity: "Common", chance: 25 },
      { name: "mystery", rarity: null, chance: 50 },
      { name: "rare", rarity: "Rare", chance: 2 },
      { name: "uncommon", rarity: "Uncommon", chance: 11 },
    ];

    expect(sortRelicRewards(rewards, "rare-first").map((reward) => reward.name)).toEqual([
      "rare",
      "uncommon",
      "common",
      "mystery",
    ]);
  });

  it("keeps the source order for rewards that tie on both keys", () => {
    const rewards = [
      { name: "first", rarity: "Common", chance: 25 },
      { name: "second", rarity: "Common", chance: 25 },
      { name: "third", rarity: "Common", chance: 25 },
    ];

    expect(sortRelicRewards(rewards).map((reward) => reward.name)).toEqual([
      "first",
      "second",
      "third",
    ]);
  });
});
