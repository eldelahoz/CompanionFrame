import { expect, it } from "vitest";
import { MOCK_RELIC_DB, MOCK_RELIC_INVENTORY } from "../../lib/relic/mockDb";
import { parseOwnedRelics } from "../../lib/relic/relicInventory";
import { plannerPriceSlugs, plannerRows } from "./plannerRows";

const owned = parseOwnedRelics(MOCK_RELIC_INVENTORY, MOCK_RELIC_DB);
const names = (era: string | null, squad = 4) => plannerRows(MOCK_RELIC_DB, owned, era, squad).map((r) => r.group.key);

it("lists only owned relics of the detected era", () => {
  expect(names("meso").sort()).toEqual(["Meso C2", "Meso F3"]);
  expect(names("lith").sort()).toEqual(["Lith A1", "Lith B2"]);
});

it("lists every owned relic when no era was detected", () => {
  expect(names(null)).toHaveLength(10 - 2); // two refinements of Lith A1 and Axi S3 share a relic
});

it("omnia takes every era but requiem", () => {
  expect(names("omnia")).toEqual(names(null));
  expect(names("requiem")).toEqual([]);
});

it("ranks by ducat EV and shows the best owned refinement", () => {
  const rows = plannerRows(MOCK_RELIC_DB, owned, "lith", 4);
  expect(rows[0].ducat!).toBeGreaterThanOrEqual(rows[1].ducat!);
  const a1 = rows.find((r) => r.group.key === "Lith A1")!;
  expect(a1.quality).toBe("radiant");
  expect(a1.count).toBe(1);
  expect(a1.ownedTotal).toBe(4);
});

it("a larger squad raises the expected ducats of the same relic", () => {
  const ev = (squad: number) => plannerRows(MOCK_RELIC_DB, owned, "meso", squad).find((r) => r.group.key === "Meso F3")!.ducat!;
  expect(ev(4)).toBeGreaterThan(ev(1));
});

it("applies the filters pushed from the Relics view, keeping the overlay's squad size", async () => {
  const { normalizeRelicOverlayFilterPush } = await import("../../../config/shared/relicPlannerView");
  const push = normalizeRelicOverlayFilterPush({ vaultedMode: "vaulted", sortMode: "name", sortDirection: "asc", neededRewardKeys: null });
  const rows = plannerRows(MOCK_RELIC_DB, owned, null, 4, push);
  expect(rows.length).toBeGreaterThan(0);
  expect(rows.every((r) => r.group.vaulted)).toBe(true);
  const keys = plannerRows(MOCK_RELIC_DB, owned, null, 4, { ...push, vaultedMode: "all", containsNeededReward: true, neededRewardKeys: ["Lith A1"] }).map((r) => r.group.key);
  expect(keys).toEqual(["Lith A1"]);
});

it("prices plat EV when given a lookup, and asks only for the era's reward slugs", () => {
  const slugs = plannerPriceSlugs(MOCK_RELIC_DB, "meso");
  expect(slugs.length).toBeGreaterThan(0);
  expect(plannerPriceSlugs(MOCK_RELIC_DB, null).length).toBeGreaterThanOrEqual(slugs.length);
  expect(plannerRows(MOCK_RELIC_DB, owned, "meso", 4).every((r) => r.plat == null)).toBe(true);
  const priced = plannerRows(MOCK_RELIC_DB, owned, "meso", 4, null, false, () => 10);
  expect(priced.every((r) => r.plat != null && r.plat > 0)).toBe(true);
});
