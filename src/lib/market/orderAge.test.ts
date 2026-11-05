import { expect, it } from "vitest";
import { orderAge } from "./orderAge";

const now = Date.parse("2026-03-10T12:00:00Z");
it("words minutes, hours and days", () => {
  expect(orderAge("2026-03-10T11:30:00Z", now, "en")).toBe("30 min. ago");
  expect(orderAge("2026-03-10T10:00:00Z", now, "en")).toBe("2 hr. ago");
  expect(orderAge("2026-03-07T12:00:00Z", now, "en")).toBe("3 days ago");
});
it("is empty without a usable date", () => {
  expect(orderAge(null, now, "en")).toBe("");
  expect(orderAge("nope", now, "en")).toBe("");
});
