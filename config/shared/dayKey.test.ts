import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";

import { localDateRange, localDayKey, toLocalDayKey } from "./dayKey.js";

// Stubbing TZ writes process.env, and Node re-reads the zone for every Date
// built after that, so these cases assert fixed keys instead of recomputing
// them the way the builder does.
beforeAll(() => {
  vi.stubEnv("TZ", "America/New_York");
});

afterAll(() => {
  vi.unstubAllEnvs();
});

describe("toLocalDayKey", () => {
  it("pads the key to YYYY-MM-DD", () => {
    expect(toLocalDayKey("2026-03-04T12:00:00")).toBe("2026-03-04");
    expect(toLocalDayKey("2026-11-30T12:00:00")).toBe("2026-11-30");
  });

  it("keys a UTC-midnight instant on the day the local clock shows", () => {
    expect(toLocalDayKey("2026-01-02T00:00:00Z")).toBe("2026-01-01");
    expect(toLocalDayKey("2026-07-01T03:59:00Z")).toBe("2026-06-30");
  });

  it("keys both sides of a DST change on the day they fall on", () => {
    // 2026-03-08 is the US spring-forward: 06:30Z is 01:30 EST, 07:30Z 03:30 EDT.
    expect(toLocalDayKey("2026-03-08T04:30:00Z")).toBe("2026-03-07");
    expect(toLocalDayKey("2026-03-08T06:30:00Z")).toBe("2026-03-08");
    expect(toLocalDayKey("2026-03-08T07:30:00Z")).toBe("2026-03-08");
  });

  it("returns an empty key for an unusable date", () => {
    expect(toLocalDayKey("not-a-date")).toBe("");
    expect(toLocalDayKey("")).toBe("");
  });
});

describe("localDayKey", () => {
  it("keys a Date the same way as the string entry point", () => {
    expect(localDayKey(new Date("2026-01-02T00:00:00Z"))).toBe("2026-01-01");
    // 02:30 does not exist on the spring-forward day and rolls to 03:30 EDT.
    expect(localDayKey(new Date(2026, 2, 8, 2, 30))).toBe("2026-03-08");
    expect(localDayKey(new Date(2025, 11, 31, 23, 59))).toBe("2025-12-31");
  });

  it("returns an empty key for an invalid Date", () => {
    expect(localDayKey(new Date("not-a-date"))).toBe("");
  });
});

describe("localDateRange", () => {
  it("includes the whole local day across both DST transitions", () => {
    for (const day of ["2026-03-08", "2026-11-01"]) {
      const inside = localDateRange(day, day);
      const start = new Date(`${day}T00:00:00`);
      const next = new Date(start);
      next.setDate(next.getDate() + 1);
      expect(inside(start.getTime())).toBe(true);
      expect(inside(next.getTime() - 1)).toBe(true);
      expect(inside(next.getTime())).toBe(false);
      expect(inside(start.getTime() - 1)).toBe(false);
    }
  });

  it("rejects invalid timestamps only when a valid bound is active", () => {
    expect(localDateRange()(NaN)).toBe(true);
    expect(localDateRange("2026-01-01")(NaN)).toBe(false);
    expect(localDateRange("2026-01-01")(null as unknown as number)).toBe(false);
    expect(localDateRange("2026-02-31")(NaN)).toBe(true);
    expect(localDateRange("2026-09-09", "2026-09-08")(new Date(2026, 8, 8).getTime())).toBe(false);
  });
});
