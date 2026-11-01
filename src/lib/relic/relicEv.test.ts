import { describe, expect, it } from "vitest";
import { MOCK_RELIC_DB } from "./mockDb";
import { evDataFor, groupDucatEv } from "./relicEv";

const group = MOCK_RELIC_DB.groups["Lith B2"];

describe("relic ducat EV", () => {
  it("averages the ducats by drop chance for a solo opening", () => {
    // Forma carries no ducats; 2 commons at 25.33% (15), 2 uncommons at 11% (45), the rare at 2% (100).
    expect(groupDucatEv(group, 1, "intact")).toBeCloseTo(0.2533 * 2 * 15 + 0.11 * 2 * 45 + 0.02 * 100, 5);
  });

  it("is higher for a refinement with better odds, and for a bigger squad", () => {
    expect(groupDucatEv(group, 1, "radiant")!).toBeGreaterThan(groupDucatEv(group, 1, "intact")!);
    expect(groupDucatEv(group, 4, "intact")!).toBeGreaterThan(groupDucatEv(group, 1, "intact")!);
    expect(groupDucatEv(group, 1, "best")).toBe(groupDucatEv(group, 1, "radiant"));
  });

  it("has no value without ducats, and no data without a selected refinement", () => {
    const bare = { ...group, qualities: { intact: { uniqueName: null, rewards: [{ ...group.qualities.intact.rewards[0], ducats: null }] } } };
    expect(groupDucatEv(bare, 1, "intact")).toBeNull();
    expect(evDataFor(group, 1, "owned", null).cls).toBe("no-data");
    expect(evDataFor(group, 1, "owned", "intact")).toMatchObject({ plat: null, cls: "has-value" });
    expect(evDataFor(group, 1, "radiant", null).ducat).not.toBeNull();
  });
});

describe("relic platinum EV", () => {
  it("uses the price lookup and feeds the ducats per platinum ratio", () => {
    const ev = evDataFor(group, 1, "intact", null, () => 10);
    expect(ev.plat).toBeCloseTo(10, 1); // the chances sum to ~100%
    expect(ev.ratio).toBeCloseTo(ev.ducat! / ev.plat!, 5);
  });

  it("stays null when no reward has a price", () => {
    expect(evDataFor(group, 1, "intact", null, () => null).plat).toBeNull();
  });
});
