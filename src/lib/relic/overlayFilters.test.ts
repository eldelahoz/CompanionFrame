// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";
import { normalizeRelicOverlayFilterPush } from "../../../config/shared/relicPlannerView";
import { loadRelicOverlayFilters, pushRelicOverlayFilters } from "./overlayFilters";

beforeEach(() => localStorage.clear());

it("round-trips a push and reads nothing before one", () => {
  expect(loadRelicOverlayFilters()).toBeNull();
  const push = normalizeRelicOverlayFilterPush({ vaultedMode: "unvaulted", squadSize: 3, neededRewardKeys: ["Lith A1"] });
  expect(pushRelicOverlayFilters(push)).toBe(true);
  expect(loadRelicOverlayFilters()).toEqual(push);
});

it("falls back to defaults for a corrupt entry", () => {
  localStorage.setItem("relic.overlayFilters", "{not json");
  expect(loadRelicOverlayFilters()).toBeNull();
});
