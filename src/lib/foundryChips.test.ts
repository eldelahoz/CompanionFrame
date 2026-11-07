import { expect, test } from "vitest";
import { fitChips } from "./foundryChips";

// 56 px chips, 6 px gap: a 5-chip row is 5*56 + 4*6 = 304 px wide
test("every chip is drawn when they all fit, however wide the row is", () => {
  expect(fitChips(5, 304, 56, 6)).toBe(5);
  expect(fitChips(3, 304, 56, 6)).toBe(3);
  expect(fitChips(0, 304, 56, 6)).toBe(0);
});

test("when they do not fit the last slot becomes the +N chip", () => {
  expect(fitChips(6, 304, 56, 6)).toBe(4); // 5 slots: 4 chips and "+2"
  expect(fitChips(9, 303, 56, 6)).toBe(3); // one px short of 5 slots: 4 slots
});

test("a row that holds a single chip draws only the +N", () => {
  expect(fitChips(4, 60, 56, 6)).toBe(0);
  expect(fitChips(1, 60, 56, 6)).toBe(1);
  expect(fitChips(2, 10, 56, 6)).toBe(0);
});

test("an unmeasured row draws everything", () => {
  expect(fitChips(8, 0, 56, 6)).toBe(8);
});
