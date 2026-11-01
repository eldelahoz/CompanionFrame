import { describe, expect, it } from "vitest";

import {
  BARO_HISTORY_MAX_ITEMS,
  BARO_HISTORY_MAX_VISITS,
  normalizeBaroHistory,
} from "./baroHistory";

const item = { uniqueName: "/Lotus/Types/TestItem", ducats: 0, credits: null };
const visit = { id: "visit-1", activation: 1000, expiry: 2000, node: "Test Relay", items: [item] };
const fixture = {
  version: 1,
  updatedAt: 3000,
  coverageStart: 1000,
  visits: [visit],
  lastSeen: [{ ...item, visitId: visit.id, lastSeen: 1000 }],
};

describe("normalizeBaroHistory", () => {
  it("joins StoreItems archive identities to inventory paths", () => {
    const raw = { ...item, uniqueName: "/Lotus/StoreItems/Types/TestItem" };
    expect(
      normalizeBaroHistory({
        ...fixture,
        visits: [{ ...visit, items: [raw] }],
        lastSeen: [{ ...raw, visitId: visit.id, lastSeen: 1000 }],
      }),
    ).toEqual(fixture);
  });
  it("preserves zero and unknown prices while stripping unexpected fields", () => {
    expect(normalizeBaroHistory(fixture)).toEqual(fixture);
    expect(
      normalizeBaroHistory({
        ...fixture,
        private: "discard",
        visits: [{ ...visit, private: "discard", items: [{ ...item, private: "discard" }] }],
        lastSeen: [{ ...fixture.lastSeen[0], private: "discard" }],
      }),
    ).toEqual(fixture);
    expect(
      normalizeBaroHistory({
        version: 1,
        updatedAt: 3000,
        coverageStart: null,
        visits: [],
        lastSeen: [],
      }),
    ).not.toBeNull();
  });

  it("rejects malformed envelopes and invalid timestamps", () => {
    for (const value of [
      null,
      [],
      {},
      { ...fixture, version: 2 },
      { ...fixture, visits: null },
      { ...fixture, lastSeen: {} },
      { ...fixture, coverageStart: undefined },
    ])
      expect(normalizeBaroHistory(value)).toBeNull();
    for (const timestamp of [0, -1, 1.5, "3000", Infinity, NaN, 8.64e15 + 1]) {
      expect(normalizeBaroHistory({ ...fixture, updatedAt: timestamp })).toBeNull();
      expect(normalizeBaroHistory({ ...fixture, coverageStart: timestamp })).toBeNull();
      expect(
        normalizeBaroHistory({ ...fixture, visits: [{ ...visit, activation: timestamp }] }),
      ).toBeNull();
      expect(
        normalizeBaroHistory({
          ...fixture,
          lastSeen: [{ ...fixture.lastSeen[0], lastSeen: timestamp }],
        }),
      ).toBeNull();
    }
  });

  it("rejects duplicate visits and duplicate item identities within each table", () => {
    expect(normalizeBaroHistory({ ...fixture, visits: [visit, visit] })).toBeNull();
    expect(
      normalizeBaroHistory({ ...fixture, visits: [{ ...visit, items: [item, item] }] }),
    ).toBeNull();
    expect(
      normalizeBaroHistory({ ...fixture, lastSeen: [fixture.lastSeen[0], fixture.lastSeen[0]] }),
    ).toBeNull();
  });

  it("rejects malformed visit IDs, intervals, nodes and item prices", () => {
    for (const id of ["", "bad/id", "x".repeat(65), 12]) {
      expect(normalizeBaroHistory({ ...fixture, visits: [{ ...visit, id }] })).toBeNull();
      expect(
        normalizeBaroHistory({ ...fixture, lastSeen: [{ ...fixture.lastSeen[0], visitId: id }] }),
      ).toBeNull();
    }
    for (const patch of [
      { expiry: 1000 },
      { expiry: 999 },
      { node: "x".repeat(129) },
      { node: null },
      { items: undefined },
    ])
      expect(normalizeBaroHistory({ ...fixture, visits: [{ ...visit, ...patch }] })).toBeNull();
    for (const patch of [
      { uniqueName: "" },
      { uniqueName: "OtherItem" },
      { uniqueName: `/Lotus/${"x".repeat(506)}` },
      { ducats: undefined },
      { credits: "20" },
      { ducats: -1 },
      { credits: 1.5 },
      { ducats: Infinity },
      { credits: Number.MAX_SAFE_INTEGER + 1 },
    ]) {
      expect(
        normalizeBaroHistory({
          ...fixture,
          visits: [{ ...visit, items: [{ ...item, ...patch }] }],
        }),
      ).toBeNull();
      expect(
        normalizeBaroHistory({ ...fixture, lastSeen: [{ ...fixture.lastSeen[0], ...patch }] }),
      ).toBeNull();
    }
  });

  it("accepts bounded collections and rejects each collection above its limit", () => {
    const visits = Array.from({ length: BARO_HISTORY_MAX_VISITS }, (_, index) => ({
      ...visit,
      id: `visit-${index}`,
    }));
    const items = Array.from({ length: 500 }, (_, index) => ({
      ...item,
      uniqueName: `/Lotus/Types/Item${index}`,
    }));
    const lastSeen = Array.from({ length: BARO_HISTORY_MAX_ITEMS }, (_, index) => ({
      ...fixture.lastSeen[0],
      uniqueName: `/Lotus/Types/Item${index}`,
    }));
    expect(normalizeBaroHistory({ ...fixture, visits, lastSeen })).not.toBeNull();
    expect(normalizeBaroHistory({ ...fixture, visits: [{ ...visit, items }] })).not.toBeNull();
    expect(
      normalizeBaroHistory({ ...fixture, visits: [...visits, { ...visit, id: "extra" }] }),
    ).toBeNull();
    expect(
      normalizeBaroHistory({
        ...fixture,
        lastSeen: [...lastSeen, { ...fixture.lastSeen[0], uniqueName: "/Lotus/Types/Extra" }],
      }),
    ).toBeNull();
    expect(
      normalizeBaroHistory({
        ...fixture,
        visits: [{ ...visit, items: [...items, { ...item, uniqueName: "/Lotus/Types/Extra" }] }],
      }),
    ).toBeNull();
  });
});
