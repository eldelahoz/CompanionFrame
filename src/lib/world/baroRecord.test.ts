import { describe, expect, it } from "vitest";
import { recordBaroVisit } from "./baroRecord";

const now = Date.UTC(2026, 8, 11, 16);
const trader = (activation: number, names: string[]) => ({
  activation: new Date(activation).toISOString(),
  expiry: new Date(activation + 172_800_000).toISOString(),
  location: "Kronia Relay",
  inventory: names.map((uniqueName) => ({ uniqueName, item: uniqueName, ducats: 100, credits: null, imageOverride: null })),
});

describe("recordBaroVisit", () => {
  it("records an active visit once and keeps older last-seen dates", () => {
    const first = recordBaroVisit(null, trader(now - 1000, ["/Lotus/StoreItems/Types/A", "/Lotus/Types/B"]), now)!;
    expect(first.visits).toHaveLength(1);
    expect(first.lastSeen.map((e) => e.uniqueName).sort()).toEqual(["/Lotus/Types/A", "/Lotus/Types/B"]);
    expect(recordBaroVisit(first, trader(now - 1000, ["/Lotus/Types/A"]), now)).toBe(first);
    const later = recordBaroVisit(first, trader(now + 5_000_000_000, ["/Lotus/Types/A"]), now + 5_000_001_000)!;
    expect(later.visits).toHaveLength(2);
    expect(later.lastSeen.find((e) => e.uniqueName === "/Lotus/Types/A")?.visitId).toBe(`v${now + 5_000_000_000}`);
    expect(later.lastSeen.find((e) => e.uniqueName === "/Lotus/Types/B")?.visitId).toBe(`v${now - 1000}`);
    expect(later.coverageStart).toBe(now - 1000);
  });
  it("ignores an inactive or empty visit", () => {
    expect(recordBaroVisit(null, trader(now + 1000, ["/Lotus/Types/A"]), now)).toBeNull();
    expect(recordBaroVisit(null, trader(now - 1000, []), now)).toBeNull();
    expect(recordBaroVisit(null, null, now)).toBeNull();
  });
});
