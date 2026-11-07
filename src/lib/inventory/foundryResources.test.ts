import { describe, expect, it } from "vitest";

import { aggregateComponentOwnership } from "../../../config/shared/componentOwnership.js";
import { itemDbOf } from "../../test/itemDb";
import { parseFoundry } from "../inventory.js";
import {
  chainBuildableBlueprints,
  foundryClaimableProducts,
  isFoundryBuildClaimable,
  isFoundryRecipeReady,
} from "./foundryResources.js";
import type {
  FoundryBuildingItem,
  FoundryData,
  ItemDbEntry,
  RawInventoryData,
} from "../../types/inventory.js";

const NOW = Date.UTC(2026, 0, 2, 12, 0, 0);

function building(overrides: Partial<FoundryBuildingItem>): FoundryBuildingItem {
  return {
    name: "Build",
    imageUrl: null,
    endDate: null,
    uniqueName: null,
    productUniqueName: null,
    category: "Misc",
    ingredients: [],
    buildPrice: 0,
    ...overrides,
  };
}

function foundry(builds: FoundryBuildingItem[]): FoundryData {
  return { building: builds, recipes: [] };
}

describe("isFoundryBuildClaimable", () => {
  it("claims a build whose end date has passed and not one still running", () => {
    expect(isFoundryBuildClaimable({ endDate: new Date(NOW - 1) }, NOW)).toBe(true);
    expect(isFoundryBuildClaimable({ endDate: new Date(NOW + 1) }, NOW)).toBe(false);
  });

  it("claims a build whose end date is exactly now", () => {
    expect(isFoundryBuildClaimable({ endDate: new Date(NOW) }, NOW)).toBe(true);
  });

  it("leaves a build without an end date running", () => {
    expect(isFoundryBuildClaimable({ endDate: null }, NOW)).toBe(false);
  });
});

describe("foundryClaimableProducts", () => {
  it("collects the finished products and skips the ones still building", () => {
    const products = foundryClaimableProducts(
      foundry([
        building({
          endDate: new Date(NOW - 60_000),
          uniqueName: "/Recipes/LexPrimeBlueprint",
          productUniqueName: "/W/LexPrime",
        }),
        building({
          endDate: new Date(NOW + 60_000),
          uniqueName: "/Recipes/SomaPrimeBlueprint",
          productUniqueName: "/W/SomaPrime",
        }),
      ]),
      NOW,
    );

    expect([...products]).toEqual(["/W/LexPrime"]);
  });

  it("falls back to the blueprint when the recipe did not resolve a product", () => {
    const products = foundryClaimableProducts(
      foundry([
        building({
          endDate: new Date(NOW - 1),
          uniqueName: "/Recipes/UnmappedThingBlueprint",
          productUniqueName: null,
        }),
      ]),
      NOW,
    );

    expect([...products]).toEqual(["/Recipes/UnmappedThingBlueprint"]);
  });

  it("skips a finished build that names nothing at all", () => {
    expect(
      foundryClaimableProducts(foundry([building({ endDate: new Date(NOW - 1) })]), NOW).size,
    ).toBe(0);
  });
});

describe("chainBuildableBlueprints", () => {
  const FRAME = "/wf/Frame";
  const CHASSIS = "/wf/FrameChassis";
  const SYSTEMS = "/wf/FrameSystems";
  const FERRITE = "/misc/Ferrite";
  const CELL = "/misc/Cell";

  const db = itemDbOf({
    [FRAME]: {
      name: "Framey",
      recipe: {
        buildPrice: 0,
        buildTime: 0,
        num: 1,
        blueprintUniqueName: "/bp/Frame",
        ingredients: [
          { uniqueName: CHASSIS, count: 1 },
          { uniqueName: SYSTEMS, count: 1 },
          { uniqueName: CELL, count: 3 },
        ],
      },
    } as ItemDbEntry,
    [CHASSIS]: {
      name: "Framey Chassis",
      componentOf: FRAME,
      recipe: {
        buildPrice: 0,
        buildTime: 0,
        num: 1,
        blueprintUniqueName: "/bp/Chassis",
        ingredients: [{ uniqueName: FERRITE, count: 2 }],
      },
    } as ItemDbEntry,
    [SYSTEMS]: {
      name: "Framey Systems",
      componentOf: FRAME,
      recipe: {
        buildPrice: 0,
        buildTime: 0,
        num: 1,
        blueprintUniqueName: "/bp/Systems",
        ingredients: [{ uniqueName: FERRITE, count: 2 }],
      },
    } as ItemDbEntry,
  });

  function recipeRow(
    uniqueName: string,
    productUniqueName: string,
    ingredients: Array<{ uniqueName: string; count: number }>,
  ) {
    return {
      name: uniqueName,
      imageUrl: null,
      count: 1,
      uniqueName,
      productUniqueName,
      isIngredient: false,
      category: "Warframe",
      ingredients,
      buildPrice: 0,
      buildTime: 0,
    };
  }

  const frameBp = recipeRow("/bp/Frame", FRAME, db[FRAME].recipe!.ingredients);
  const chassisBp = recipeRow("/bp/Chassis", CHASSIS, db[CHASSIS].recipe!.ingredients);
  const systemsBp = recipeRow("/bp/Systems", SYSTEMS, db[SYSTEMS].recipe!.ingredients);

  it("marks a frame whose parts are all craftable from owned blueprints", () => {
    const owned = new Map([
      [FERRITE, 4],
      [CELL, 3],
    ]);
    const result = chainBuildableBlueprints([frameBp, chassisBp, systemsBp], owned, db);

    expect(result.has("/bp/Frame")).toBe(true);
    // Part blueprints stay out: the full-set view hides loose components.
    expect(result.has("/bp/Chassis")).toBe(false);
  });

  it("rejects the frame when a part blueprint is missing", () => {
    const owned = new Map([
      [FERRITE, 4],
      [CELL, 3],
    ]);
    const result = chainBuildableBlueprints([frameBp, chassisBp], owned, db);

    expect(result.has("/bp/Frame")).toBe(false);
  });

  it("rejects the frame when parts would double-spend a shared resource", () => {
    const owned = new Map([
      [FERRITE, 3],
      [CELL, 3],
    ]);
    const result = chainBuildableBlueprints([frameBp, chassisBp, systemsBp], owned, db);

    expect(result.has("/bp/Frame")).toBe(false);
  });

  it("accepts already-built parts without their blueprints", () => {
    const owned = new Map([
      [CHASSIS, 1],
      [SYSTEMS, 1],
      [CELL, 3],
    ]);
    const result = chainBuildableBlueprints([frameBp], owned, db);

    expect(result.has("/bp/Frame")).toBe(true);
  });

  it("scales sub-builds by the recipe yield", () => {
    const PART = "/misc/Part";
    const yieldDb = itemDbOf({
      "/item/Thing": {
        name: "Thing",
        recipe: {
          buildPrice: 0,
          buildTime: 0,
          num: 1,
          blueprintUniqueName: "/bp/Thing",
          ingredients: [{ uniqueName: PART, count: 15 }],
        },
      } as ItemDbEntry,
      [PART]: {
        name: "Part",
        recipe: {
          buildPrice: 0,
          buildTime: 0,
          num: 10,
          blueprintUniqueName: "/bp/Part",
          ingredients: [{ uniqueName: FERRITE, count: 2 }],
        },
      } as ItemDbEntry,
    });
    const thingBp = recipeRow(
      "/bp/Thing",
      "/item/Thing",
      yieldDb["/item/Thing"].recipe!.ingredients,
    );
    // Two builds burn two blueprint copies, so hold the yield math to that.
    const partBp = { ...recipeRow("/bp/Part", PART, yieldDb[PART].recipe!.ingredients), count: 2 };

    // 15 parts = 2 builds of 10, consuming 4 ferrite.
    expect(
      chainBuildableBlueprints([thingBp, partBp], new Map([[FERRITE, 4]]), yieldDb).has(
        "/bp/Thing",
      ),
    ).toBe(true);
    expect(
      chainBuildableBlueprints([thingBp, partBp], new Map([[FERRITE, 3]]), yieldDb).has(
        "/bp/Thing",
      ),
    ).toBe(false);
    // One copy cannot cover both builds.
    expect(
      chainBuildableBlueprints(
        [thingBp, { ...partBp, count: 1 }],
        new Map([[FERRITE, 4]]),
        yieldDb,
      ).has("/bp/Thing"),
    ).toBe(false);
  });
});

// DE names a recipe ingredient ...Component but the inventory only ever holds
// the ...Blueprint it is built from, so the card status and the "Can build
// (full set)" filter have to read the same predicate or they disagree.
describe("foundry set readiness", () => {
  const FRAME = "/Lotus/Powersuits/Test/TestPrime";
  const FRAME_BP = "/Lotus/Types/Recipes/WarframeRecipes/TestPrimeBlueprint";
  const CELL = "/Lotus/Types/Items/MiscItems/OrokinCell";
  const RUBEDO = "/Lotus/Types/Items/MiscItems/Rubedo";
  const PARTS = ["Helmet", "Chassis", "Systems"] as const;
  const partComponent = (part: string) =>
    `/Lotus/Types/Recipes/WarframeRecipes/TestPrime${part}Component`;
  const partBlueprint = (part: string) =>
    `/Lotus/Types/Recipes/WarframeRecipes/TestPrime${part}Blueprint`;

  const db = itemDbOf({
    [FRAME]: {
      name: "Test Prime",
      recipe: {
        buildPrice: 25000,
        buildTime: 259200,
        num: 1,
        blueprintUniqueName: FRAME_BP,
        ingredients: [
          ...PARTS.map((part) => ({ uniqueName: partComponent(part), count: 1 })),
          { uniqueName: CELL, count: 3 },
        ],
      },
    } as ItemDbEntry,
  });
  for (const part of PARTS) {
    db[partComponent(part)] = {
      name: `Test Prime ${part}`,
      componentOf: FRAME,
      recipe: {
        buildPrice: 0,
        buildTime: 0,
        num: 1,
        blueprintUniqueName: partBlueprint(part),
        ingredients: [{ uniqueName: RUBEDO, count: 100 }],
      },
    } as ItemDbEntry;
  }

  function readiness(inventory: RawInventoryData) {
    const foundry = parseFoundry(inventory, db);
    const owned = aggregateComponentOwnership(inventory);
    const chain = chainBuildableBlueprints(foundry.recipes, owned, db);
    const frame = foundry.recipes.find((r) => r.uniqueName === FRAME_BP);
    expect(frame).toBeDefined();
    return {
      chainBuildable: chain.has(FRAME_BP),
      ready: isFoundryRecipeReady(frame!, owned, chain),
    };
  }

  it("reports a frame ready when its parts are still craftable from owned blueprints", () => {
    const { chainBuildable, ready } = readiness({
      Recipes: [
        { ItemType: FRAME_BP, ItemCount: 13 },
        ...PARTS.map((part) => ({ ItemType: partBlueprint(part), ItemCount: 1 })),
      ],
      MiscItems: [
        { ItemType: CELL, ItemCount: 1200 },
        { ItemType: RUBEDO, ItemCount: 300 },
      ],
    } as RawInventoryData);

    // The filter admits it, so the card must not read "Missing parts".
    expect(chainBuildable).toBe(true);
    expect(ready).toBe(true);
  });

  it("reports a frame ready when the built parts are already in hand", () => {
    const { ready } = readiness({
      Recipes: [{ ItemType: FRAME_BP, ItemCount: 1 }],
      MiscItems: [
        { ItemType: CELL, ItemCount: 1200 },
        ...PARTS.map((part) => ({ ItemType: partComponent(part), ItemCount: 1 })),
      ],
    } as RawInventoryData);

    expect(ready).toBe(true);
  });

  it("keeps a frame unready when only the bulk resource is owned", () => {
    const { chainBuildable, ready } = readiness({
      Recipes: [{ ItemType: FRAME_BP, ItemCount: 13 }],
      MiscItems: [{ ItemType: CELL, ItemCount: 1200 }],
    } as RawInventoryData);

    expect(chainBuildable).toBe(false);
    expect(ready).toBe(false);
  });

  it("keeps a frame unready when a part blueprint cannot be built", () => {
    const { chainBuildable, ready } = readiness({
      Recipes: [
        { ItemType: FRAME_BP, ItemCount: 13 },
        ...PARTS.map((part) => ({ ItemType: partBlueprint(part), ItemCount: 1 })),
      ],
      MiscItems: [
        { ItemType: CELL, ItemCount: 1200 },
        { ItemType: RUBEDO, ItemCount: 250 },
      ],
    } as RawInventoryData);

    expect(chainBuildable).toBe(false);
    expect(ready).toBe(false);
  });

  // A build burns its blueprint, so a part wanted twice wants two copies unless
  // DE marks the recipe consumeOnUse=false.
  describe("part blueprint copies", () => {
    const GUN = "/Lotus/Weapons/Test/AktestPrime";
    const GUN_BP = "/Lotus/Types/Recipes/Weapons/AktestPrimeBlueprint";
    const LINK = "/Lotus/Types/Recipes/Weapons/AktestPrimeLinkComponent";
    const LINK_BP = "/Lotus/Types/Recipes/Weapons/AktestPrimeLinkBlueprint";

    type ReusableFlag = "none" | "recipe" | "blueprint";

    function gunDb(reusable: ReusableFlag): Record<string, ItemDbEntry> {
      return {
        [GUN]: {
          name: "Aktest Prime",
          recipe: {
            buildPrice: 0,
            buildTime: 0,
            num: 1,
            blueprintUniqueName: GUN_BP,
            ingredients: [
              { uniqueName: LINK, count: 2 },
              { uniqueName: CELL, count: 2 },
            ],
          },
        } as ItemDbEntry,
        [LINK]: {
          name: "Aktest Prime Link",
          componentOf: GUN,
          recipe: {
            buildPrice: 0,
            buildTime: 0,
            num: 1,
            blueprintUniqueName: LINK_BP,
            ...(reusable === "recipe" ? { reusableBlueprint: true } : {}),
            ingredients: [{ uniqueName: RUBEDO, count: 100 }],
          },
        } as ItemDbEntry,
        [LINK_BP]: {
          name: "Aktest Prime Link Blueprint",
          ...(reusable === "blueprint" ? { reusableBlueprint: true } : {}),
        } as ItemDbEntry,
      };
    }

    function gunReadiness(linkCopies: number, builtLinks: number, reusable: ReusableFlag = "none") {
      const db = gunDb(reusable);
      const inventory: RawInventoryData = {
        Recipes: [
          { ItemType: GUN_BP, ItemCount: 1 },
          { ItemType: LINK_BP, ItemCount: linkCopies },
        ],
        MiscItems: [
          { ItemType: CELL, ItemCount: 10 },
          { ItemType: RUBEDO, ItemCount: 500 },
          ...(builtLinks > 0 ? [{ ItemType: LINK, ItemCount: builtLinks }] : []),
        ],
      };
      const foundry = parseFoundry(inventory, db);
      const owned = aggregateComponentOwnership(inventory);
      const chain = chainBuildableBlueprints(foundry.recipes, owned, db);
      const gun = foundry.recipes.find((r) => r.uniqueName === GUN_BP);
      expect(gun).toBeDefined();
      return {
        chainBuildable: chain.has(GUN_BP),
        ready: isFoundryRecipeReady(gun!, owned, chain),
      };
    }

    it("keeps a weapon unready when one consumed blueprint must cover two parts", () => {
      const { chainBuildable, ready } = gunReadiness(1, 0);

      expect(chainBuildable).toBe(false);
      expect(ready).toBe(false);
    });

    it("accepts a second copy of the part blueprint", () => {
      expect(gunReadiness(2, 0).ready).toBe(true);
    });

    it("accepts one built part plus one blueprint", () => {
      expect(gunReadiness(1, 1).ready).toBe(true);
    });

    it("accepts a single reusable part blueprint", () => {
      expect(gunReadiness(1, 0, "recipe").ready).toBe(true);
      expect(gunReadiness(1, 0, "blueprint").ready).toBe(true);
    });
  });
});

describe("foundry doubled ingredient", () => {
  const TWIN = "/Lotus/Weapons/Test/TwinTest";
  const TWIN_BP = "/Lotus/Types/Recipes/Weapons/TwinTestBlueprint";
  const BASE = "/Lotus/Weapons/Test/BaseTest";
  // DE lists a doubled ingredient as two rows of one.
  const db = itemDbOf({
    [TWIN]: {
      name: "Twin Test",
      recipe: {
        buildPrice: 0,
        buildTime: 0,
        num: 1,
        blueprintUniqueName: TWIN_BP,
        ingredients: [
          { uniqueName: BASE, count: 1 },
          { uniqueName: BASE, count: 1 },
        ],
      },
    } as ItemDbEntry,
  });

  function twin(baseCopies: number) {
    const inventory = {
      Recipes: [{ ItemType: TWIN_BP, ItemCount: 1 }],
      MiscItems: [{ ItemType: BASE, ItemCount: baseCopies }],
    } as RawInventoryData;
    const recipe = parseFoundry(inventory, db).recipes.find((r) => r.uniqueName === TWIN_BP)!;
    const owned = aggregateComponentOwnership(inventory);
    return { recipe, ready: isFoundryRecipeReady(recipe, owned, new Set()) };
  }

  it("merges the two rows into one ingredient of count 2", () => {
    expect(twin(1).recipe.ingredients).toEqual([{ uniqueName: BASE, count: 2 }]);
  });

  it("is not ready with one copy and ready with two", () => {
    expect(twin(1).ready).toBe(false);
    expect(twin(2).ready).toBe(true);
  });
});
