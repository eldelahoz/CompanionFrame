import { describe, expect, it } from "vitest";

import { createAnalyticsItemResolver } from "./analyticsItemLink.js";
import type { ItemDbEntry } from "../../types/inventory.js";
import type { ItemDbLookup, WfmItemsLookup } from "../../types/stats.js";
import type { RelicDatabase } from "../relic/types.js";

const FORMA = "/Lotus/Types/Items/MiscItems/Forma";
const CELL = "/Lotus/Types/Items/MiscItems/OrokinCell";
const SERRATION = "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod";

function entry(name: string): ItemDbEntry {
  return { name, category: "Misc" } as ItemDbEntry;
}

const itemDb: ItemDbLookup = {
  [FORMA]: entry("Forma"),
  [CELL]: entry("Orokin Cell"),
  [SERRATION]: entry("Serration"),
};

const wfmItems: WfmItemsLookup = {
  forma: { url_name: "forma", gameRef: FORMA },
  "orokin cell": { url_name: "orokin_cell", gameRef: CELL },
  // WFM's gameRef does not promise DE's casing, so this one differs from the key.
  serration: { url_name: "serration", gameRef: SERRATION.toLowerCase() },
  "mirage prime set": { url_name: "mirage_prime_set", gameRef: "/Lotus/Types/Absent" },
};

// @wfcd names the Intact relic "Lith M7 Intact" and DE suffixes it Bronze;
// warframe.market calls the same listing "Lith M7 Relic".
const LITH_M7_INTACT = "/Lotus/Types/Game/Projections/T1VoidProjectionBansheeMirageVaultBBronze";

const relicDb: RelicDatabase = {
  groups: {
    "Lith M7": {
      key: "Lith M7",
      name: "Lith M7",
      tier: "Lith",
      code: "M7",
      imageUrl: null,
      vaulted: false,
      qualities: { intact: { uniqueName: LITH_M7_INTACT, rewards: [] } },
    },
  },
  byUniqueName: { [LITH_M7_INTACT]: { groupKey: "Lith M7", quality: "intact" } },
};

describe("analytics item link resolver", () => {
  it("takes an internal name straight from the item database", () => {
    expect(createAnalyticsItemResolver(itemDb, wfmItems, null)(FORMA, "Forma")).toBe(FORMA);
  });

  it("resolves a warframe.market slug through the catalog", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("orokin_cell", "Orokin Cell")).toBe(CELL);
  });

  it("folds a lowercased display-name key into the slug it matches", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("orokin cell", "Orokin Cell")).toBe(CELL);
  });

  it("falls back to the catalog name when the key resolves to nothing", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("", "Forma")).toBe(FORMA);
  });

  it("folds a gameRef whose casing differs from the item-database key", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("serration", "Serration")).toBe(SERRATION);
  });

  it("resolves a ranked row through its bare name", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("serration (rank 10)", "Serration (Rank 10)")).toBe(SERRATION);
  });

  it("returns null for a riven roll and for a catalog entry the database lacks", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("braton riven", "Braton Riven")).toBeNull();
    expect(resolve("mirage_prime_set", "Mirage Prime Set")).toBeNull();
  });

  it("keeps resolving the same keys after the lazy indices are built", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, null);
    expect(resolve("serration", "Serration")).toBe(SERRATION);
    expect(resolve("orokin_cell", "Orokin Cell")).toBe(CELL);
    expect(resolve("serration", "Serration")).toBe(SERRATION);
    expect(resolve("mirage_prime_set", "Mirage Prime Set")).toBeNull();
  });

  it("resolves a market relic name to the intact relic the database knows", () => {
    const resolve = createAnalyticsItemResolver(itemDb, wfmItems, relicDb);

    expect(resolve("lith_m7_relic", "Lith M7 Relic")).toBe(LITH_M7_INTACT);
    expect(resolve("lith_m7_relic", "Lith M7 Relic (Radiant)")).toBe(LITH_M7_INTACT);
  });

  it("leaves an unknown relic and a missing relic database alone", () => {
    expect(
      createAnalyticsItemResolver(itemDb, wfmItems, relicDb)("axi_z1_relic", "Axi Z1 Relic"),
    ).toBeNull();
    expect(
      createAnalyticsItemResolver(itemDb, wfmItems, null)("lith_m7_relic", "Lith M7 Relic"),
    ).toBeNull();
  });
});
