import { describe, expect, it } from "vitest";

import {
  componentUniqueNameAliases,
  ownedComponentCount,
} from "./componentNames";

const WARFRAME_BP = "/Lotus/Types/Recipes/WarframeRecipes/ProteaPrimeBlueprint";
const WARFRAME_PART = "/Lotus/Types/Recipes/WarframeRecipes/ProteaPrimeChassisComponent";
const WEAPON_PART = "/Lotus/Types/Recipes/Weapons/WeaponParts/AkbroncoPrimeLink";
const BUILT_WEAPON = "/Lotus/Weapons/Tenno/Pistol/BroncoPrime";

describe("componentUniqueNameAliases", () => {
  it("maps a warframe part between its Component and Blueprint spellings", () => {
    expect(componentUniqueNameAliases(WARFRAME_PART)).toContain(
      "/Lotus/Types/Recipes/WarframeRecipes/ProteaPrimeChassisBlueprint",
    );
    expect(componentUniqueNameAliases(WARFRAME_BP)).toContain(
      "/Lotus/Types/Recipes/WarframeRecipes/ProteaPrimeComponent",
    );
  });

  it("appends Blueprint to recipe parts that carry no suffix", () => {
    expect(componentUniqueNameAliases(WEAPON_PART)).toContain(`${WEAPON_PART}Blueprint`);
  });

  it("leaves non-recipe paths alone", () => {
    expect(componentUniqueNameAliases(BUILT_WEAPON)).toEqual([BUILT_WEAPON]);
  });
});

describe("ownedComponentCount", () => {
  it("finds a warframe part the inventory holds as a blueprint", () => {
    const owned = new Map([
      ["/Lotus/Types/Recipes/WarframeRecipes/ProteaPrimeChassisBlueprint", 5],
    ]);
    expect(ownedComponentCount(WARFRAME_PART, owned)).toBe(5);
  });

  it("finds a weapon part the inventory holds as a blueprint", () => {
    const owned = new Map([[`${WEAPON_PART}Blueprint`, 3]]);
    expect(ownedComponentCount(WEAPON_PART, owned)).toBe(3);
  });

  it("takes the largest alias instead of summing the same pile", () => {
    const owned = new Map([
      [WARFRAME_PART, 2],
      ["/Lotus/Types/Recipes/WarframeRecipes/ProteaPrimeChassisBlueprint", 5],
    ]);
    expect(ownedComponentCount(WARFRAME_PART, owned)).toBe(5);
  });

  it("returns 0 for missing or empty names", () => {
    expect(ownedComponentCount(WARFRAME_PART, new Map())).toBe(0);
    expect(ownedComponentCount("", new Map([["", 4]]))).toBe(0);
  });
});
