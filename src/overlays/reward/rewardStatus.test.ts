import { expect, it } from "vitest";
import type { RewardOverlayItem, RewardOverlaySlot } from "../../bindings";
import { translator } from "../../lib/i18n";
import { bestSlot, rewardStatus, rewardStatusLabel } from "./rewardStatus";

const item = (over: Partial<RewardOverlayItem> = {}): RewardOverlayItem => ({
  name: "Ash Prime Systems Blueprint", rarity: "Rare", ducats: 100, partOwnedCount: 0, partRequiredCount: 1, mastered: false,
  vaulted: true, building: false, setOwnedCount: 2, setRequiredCount: 4, setUrlName: "ash_prime_set", setParts: [], ...over,
});
const tr = translator("en");
const label = (over: Partial<RewardOverlayItem>, inv = true) => rewardStatusLabel(rewardStatus(item(over), inv), tr);

it("says owned ×N, not built for a masterable item never built", () => {
  expect(rewardStatus(item({ partOwnedCount: 2 }), true)).toEqual({ kind: "unbuilt", owned: 2 });
  expect(label({ partOwnedCount: 2 })).toBe("Owned ×2, not built");
});

it("says mastered with the owned count, owned or not", () => {
  expect(label({ mastered: true, partOwnedCount: 1 })).toBe("Mastered, owned ×1");
  expect(label({ mastered: true, partOwnedCount: 0 })).toBe("Mastered, owned ×0");
});

it("says plain owned ×N for things with nothing to master (Forma)", () => {
  expect(label({ mastered: null, partOwnedCount: 3, setRequiredCount: 0 })).toBe("Owned ×3");
});

it("says needed for set with the set progress when the part is missing", () => {
  expect(rewardStatus(item(), true)).toEqual({ kind: "needed", owned: 2, required: 4 });
  expect(label({})).toBe("Needed for set");
});

it("says not owned when there is no set to count", () => {
  expect(label({ mastered: null, setRequiredCount: 0 })).toBe("Not owned");
});

it("never guesses counts without an inventory, but keeps the profile's mastery", () => {
  expect(label({ partOwnedCount: 0 }, false)).toBe("No inventory");
  expect(label({ mastered: null }, false)).toBe("No inventory");
  expect(rewardStatus(item({ mastered: true }), false)).toEqual({ kind: "mastered", owned: null });
  expect(label({ mastered: true }, false)).toBe("Mastered");
});

it("picks the highest price, then ducats, skipping empty and unvalued slots", () => {
  const slot = (price: number | null, ducats: number): RewardOverlaySlot => ({ item: item({ ducats }), price, setPrice: null });
  expect(bestSlot([slot(10, 45), null, slot(38, 100), slot(38, 15)])).toBe(2);
  expect(bestSlot([slot(null, 15), slot(null, 45)])).toBe(1);
  expect(bestSlot([slot(null, 0), null])).toBeNull();
});
