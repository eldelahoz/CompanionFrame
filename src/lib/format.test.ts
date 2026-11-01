import { afterEach, expect, test } from "vitest";
import { formatFixed, formatInt, formatNumber, setUiLocale } from "./format";

afterEach(() => setUiLocale(undefined));

test("numbers follow the UI locale unless one is passed", () => {
  setUiLocale("fr");
  expect(formatInt(5690)).toMatch(/^5\s690$/);
  expect(formatNumber(5690)).toMatch(/^5\s690$/);
  expect(formatFixed(1234.5, 1)).toMatch(/^1\s234,5$/);
  expect(formatInt(5690, "en")).toBe("5,690");
});
