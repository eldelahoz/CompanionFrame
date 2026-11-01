import { describe, expect, it } from "vitest";

import type { BaroHistory } from "../../../config/shared/baroHistory";
import { baroBudget, buildBaroCatalog, buildBaroOwnedSet } from "./baroPlanner";
import type { RawInventoryData } from "../../types/inventory";
import type { VaultTrader } from "./baroPlanner";

const first = "/Lotus/Weapons/FirstWeapon";
const second = "/Lotus/Upgrades/SecondMod";
const historical = "/Lotus/Types/ThirdItem";
const missing = "/Lotus/Types/FourthItem";
const now = Date.UTC(2026, 8, 11, 16);
const history: BaroHistory = {
  version: 1,
  updatedAt: now - 60_000,
  coverageStart: now - 1_000_000,
  visits: [],
  lastSeen: [
    { uniqueName: first, ducats: 50, credits: 100, visitId: "older", lastSeen: now - 900_000 },
    {
      uniqueName: historical,
      ducats: 500,
      credits: 1_000_000,
      visitId: "older",
      lastSeen: now - 900_000,
    },
  ],
};
const current: VaultTrader = {
  activation: new Date(now - 1000).toISOString(),
  expiry: new Date(now + 1000).toISOString(),
  inventory: [
    { uniqueName: first, item: "First item", ducats: 600, credits: 60_000 },
    { uniqueName: second, item: "Second item", ducats: 600, credits: 60_000 },
  ],
};
const ducatType = "/Lotus/Types/Items/MiscItems/PrimeBucks";
const wallet = (ducats: number, credits: number): RawInventoryData => ({
  MiscItems: [{ ItemType: ducatType, ItemCount: ducats }],
  RegularCredits: credits,
});

describe("buildBaroCatalog", () => {
  it("joins history, current offers and wishlist identities without duplicating repeated offers", () => {
    const rows = buildBaroCatalog(
      history,
      {
        ...current,
        inventory: [...current.inventory!, { uniqueName: first, ducats: 999, credits: 999 }],
      },
      null,
      {
        [first]: {
          name: "English weapon",
          displayName: "Localized weapon",
          imageUrl: "https://example.test/weapon.png",
        },
      },
      { [first]: 2, [missing]: 1 },
      now,
    );
    expect(rows).toHaveLength(4);
    expect(rows.find((row) => row.uniqueName === first)).toMatchObject({
      name: "Localized weapon",
      imageUrl: "https://example.test/weapon.png",
      ducats: 600,
      credits: 60_000,
      available: true,
      lastSeen: now - 1000,
      quantity: 2,
      owned: false,
    });
    expect(rows.find((row) => row.uniqueName === second)?.name).toBe("Second item");
    expect(rows.find((row) => row.uniqueName === historical)).toMatchObject({
      available: false,
      ducats: 500,
      lastSeen: now - 900_000,
    });
    expect(rows.find((row) => row.uniqueName === missing)).toMatchObject({
      name: "Fourth Item",
      imageUrl: null,
      ducats: null,
      credits: null,
      lastSeen: null,
      quantity: 1,
    });
  });

  it("does not substitute a historical price when the current offer omits it", () => {
    const rows = buildBaroCatalog(
      history,
      { ...current, inventory: [{ uniqueName: first, credits: 0 }] },
      null,
      {},
      {},
      now,
    );
    expect(rows.find((row) => row.uniqueName === first)).toMatchObject({
      ducats: null,
      credits: 0,
      available: true,
    });
  });

  it("closes expired and future visits even when inventory remains present", () => {
    for (const clock of [now + 1000, now - 1001]) {
      const rows = buildBaroCatalog(null, current, null, {}, { [first]: 1 }, clock);
      expect(rows.every((row) => !row.available)).toBe(true);
      expect(baroBudget(rows, wallet(0, 0))).toMatchObject({
        totalDucats: 0,
        totalCredits: 0,
        unavailableItems: 1,
      });
    }
    expect(buildBaroCatalog(null, current, null, {}, {}, now - 1001)[0].lastSeen).toBeNull();
    expect(buildBaroCatalog(null, current, null, {}, {}, now + 1000)[0].lastSeen).toBe(now - 1000);
    for (const patch of [{ activation: "" }, { expiry: "invalid" }]) {
      expect(
        buildBaroCatalog(null, { ...current, ...patch }, null, {}, {}, now).every(
          (row) => !row.available,
        ),
      ).toBe(true);
    }
  });

  it("keeps wishlist rows without history and ignores malformed quantities and identities", () => {
    const rows = buildBaroCatalog(
      null,
      null,
      null,
      {},
      { [first]: 0, [second]: -1, [historical]: 1.5, [missing]: NaN, bogus: 1 },
      now,
    );
    expect(rows).toHaveLength(4);
    expect(rows.every((row) => row.quantity === 0 && row.lastSeen === null && !row.available)).toBe(
      true,
    );
  });

  it("uses current ownership including ranked mods, not mastery history", () => {
    const inventory: RawInventoryData = {
      LongGuns: [{ ItemType: first }],
      Upgrades: [{ ItemType: second, UpgradeFingerprint: "ranked" }],
      MiscItems: [{ ItemType: historical, ItemCount: 0 }],
      XPInfo: [{ ItemType: missing, XP: 450000 }],
    };
    const rows = buildBaroCatalog(history, current, inventory, {}, { [missing]: 1 }, now);
    expect(rows.filter((row) => row.owned).map((row) => row.uniqueName)).toEqual([first, second]);
    expect(rows.find((row) => row.uniqueName === missing)?.owned).toBe(false);
    expect(
      buildBaroOwnedSet({
        RawUpgrades: [
          { ItemType: first, ItemCount: 0 },
          { ItemType: second, ItemCount: 2 },
        ],
        FlavourItems: [{ ItemType: historical, Count: 0 }, { ItemType: missing }],
      }),
    ).toEqual(new Set([second, missing]));
  });
});

describe("baroBudget", () => {
  it("checks the combined basket rather than each individually affordable item", () => {
    const rows = buildBaroCatalog(
      history,
      current,
      null,
      {},
      { [first]: 1, [second]: 1, [historical]: 3, [missing]: 1 },
      now,
    );
    const inventory = wallet(1000, 100_000);
    expect(
      baroBudget([rows.find((row) => row.uniqueName === first)!], inventory).ducatShortfall,
    ).toBe(0);
    expect(
      baroBudget([rows.find((row) => row.uniqueName === second)!], inventory).creditShortfall,
    ).toBe(0);
    expect(baroBudget(rows, inventory)).toEqual({
      ducats: 1000,
      credits: 100_000,
      totalDucats: 1200,
      totalCredits: 120_000,
      unknownCosts: 0,
      unknownDucats: false,
      unknownCredits: false,
      unavailableItems: 2,
      ducatShortfall: 200,
      creditShortfall: 20_000,
    });
  });

  it("multiplies quantities once and excludes unwished offers", () => {
    const rows = buildBaroCatalog(
      null,
      { ...current, inventory: [...current.inventory!, ...current.inventory!] },
      null,
      {},
      { [first]: 2 },
      now,
    );
    expect(baroBudget(rows, wallet(0, 0))).toMatchObject({
      totalDucats: 1200,
      totalCredits: 120_000,
      ducatShortfall: 1200,
      creditShortfall: 120_000,
    });
  });

  it("keeps missing balances distinct from zero and reads only the configured currencies", () => {
    const rows = buildBaroCatalog(null, current, null, {}, { [first]: 1 }, now);
    for (const inventory of [null, {}, { PrimeTokens: 999, PremiumCredits: 999 }]) {
      expect(baroBudget(rows, inventory)).toMatchObject({
        ducats: null,
        credits: null,
        ducatShortfall: null,
        creditShortfall: null,
      });
    }
    expect(baroBudget(rows, { MiscItems: [], RegularCredits: 0 })).toMatchObject({
      ducats: 0,
      credits: 0,
      ducatShortfall: 600,
      creditShortfall: 60_000,
    });
    for (const value of [null, "500", false, -1, Infinity, NaN]) {
      const inventory = {
        MiscItems: [{ ItemType: ducatType, ItemCount: value }],
        RegularCredits: value,
      } as unknown as RawInventoryData;
      expect(baroBudget(rows, inventory)).toMatchObject({
        ducats: null,
        credits: null,
        ducatShortfall: null,
        creditShortfall: null,
      });
    }
    expect(
      baroBudget(rows, {
        MiscItems: [
          { ItemType: ducatType, ItemCount: 50 },
          { ItemType: ducatType, ItemCount: 500 },
        ],
      }).ducats,
    ).toBe(50);
  });

  it("marks a partial price unknown and only computes a fully known currency shortfall", () => {
    const offers = {
      ...current,
      inventory: [
        { uniqueName: first, credits: 60_000 },
        { uniqueName: second, ducats: 100, credits: 10_000 },
      ],
    };
    const rows = buildBaroCatalog(history, offers, null, {}, { [first]: 2, [second]: 1 }, now);
    expect(baroBudget(rows, wallet(0, 100_000))).toMatchObject({
      totalDucats: 100,
      totalCredits: 130_000,
      unknownCosts: 1,
      unknownDucats: true,
      unknownCredits: false,
      ducatShortfall: null,
      creditShortfall: 30_000,
    });
    expect(
      baroBudget(
        rows.map((row) => ({ ...row, ducats: 0, credits: null })),
        wallet(0, 0),
      ),
    ).toMatchObject({
      totalDucats: 0,
      totalCredits: 0,
      unknownCosts: 2,
      unknownDucats: false,
      unknownCredits: true,
      ducatShortfall: 0,
      creditShortfall: null,
    });
  });
});
