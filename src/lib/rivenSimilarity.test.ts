import { expect, it } from "vitest";
import { canonicalRivenStatName, computeRivenStatSimilarity, formatRivenListingStatValue } from "./rivenSimilarity";

it("scores the overlap of stat names", () => {
  const { pct, matchedNames } = computeRivenStatSimilarity(["critical chance", "multishot"], [{ name: "Critical Chance" }, { name: "Zoom" }]);
  expect(pct).toBe(33);
  expect([...matchedNames]).toEqual(["critical chance"]);
  expect(computeRivenStatSimilarity([], [{ name: "x" }]).pct).toBe(0);
});

it("canonicalises aliases and formats signed values", () => {
  expect(canonicalRivenStatName(" Attack Speed ")).toBe("fire rate");
  expect(formatRivenListingStatValue({ name: "Damage Vs Corpus", value: 0.98 })).toBe("x0.98");
  expect(formatRivenListingStatValue({ name: "Zoom", value: -4.4 })).toBe("−4%");
});
