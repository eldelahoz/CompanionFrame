// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

const VIT = "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod";
const HS = "/Lotus/Upgrades/Mods/Pistol/HornetStrikeMod";
const ARC_A = "/Lotus/Upgrades/CosmeticEnhancers/Zariman/ArcaneAlpha";
const ARC_B = "/Lotus/Upgrades/CosmeticEnhancers/Zariman/ArcaneBeta";
const ARC_C = "/Lotus/Upgrades/CosmeticEnhancers/Zariman/ArcaneGamma";
const AKARIUS = "/Lotus/Weapons/Tenno/Pistols/AkariusPrime";
const BARREL = "/Lotus/Types/Recipes/Weapons/WeaponParts/AkariusPrimeBarrel";
const BOLTOR = "/Lotus/Weapons/Tenno/Rifle/BoltorPrime";
const RECEIVER = "/Lotus/Types/Recipes/Weapons/WeaponParts/BoltorPrimeReceiver";

// Akarius Prime is not mastered and needs two barrels; Boltor Prime is, so its receivers carry no floor.
vi.mock("../lib/ipc", async (original) => {
  const actual = await original<typeof import("../lib/ipc")>();
  return {
    ...actual,
    getMasteryProgress: async () => ({
      items: [
        { uniqueName: AKARIUS, status: "progress" },
        { uniqueName: BOLTOR, status: "mastered" },
      ],
    }),
    getInventory: async () => {
      const data = structuredClone(await actual.getInventory())!;
      data.RawUpgrades = [{ ItemType: VIT, ItemCount: 3 }, { ItemType: HS, ItemCount: 2 }];
      data.Upgrades = [
        { ItemType: ARC_A, ItemCount: 2, UpgradeFingerprint: '{"lvl":0}' },
        { ItemType: ARC_A, ItemCount: 1, UpgradeFingerprint: '{"lvl":5}' },
        { ItemType: ARC_B, ItemCount: 1, UpgradeFingerprint: '{"lvl":0}' },
        { ItemType: ARC_C, ItemCount: 3, UpgradeFingerprint: '{"lvl":2}' },
      ];
      data.MiscItems = [
        ...(data.MiscItems as unknown[]),
        { ItemType: BARREL, ItemCount: 3 },
        { ItemType: RECEIVER, ItemCount: 2 },
      ] as typeof data.MiscItems;
      return data;
    },
  };
});

const tr = translator("en");
let items: ItemDatabase;

beforeEach(async () => {
  resetItemPricesForTest();
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
  items[HS] = { ...items[VIT], name: "Hornet Strike" };
  for (const [path, name] of [[ARC_A, "Arcane Alpha"], [ARC_B, "Arcane Beta"], [ARC_C, "Arcane Gamma"]] as const) {
    items[path] = { ...items[VIT], name, category: "Arcanes", type: "Arcane" };
  }
  items[AKARIUS] = { name: "Akarius Prime", masterable: true, isPrime: true, components: [{ name: "Barrel", uniqueName: BARREL, itemCount: 2 }] } as never;
  items[BOLTOR] = { name: "Boltor Prime", masterable: true, isPrime: true, components: [{ name: "Receiver", uniqueName: RECEIVER, itemCount: 1 }] } as never;
  items[BARREL] = { name: "Akarius Prime Barrel", isBuildComponent: true, componentOf: AKARIUS, isPrime: true, tradable: true } as never;
  items[RECEIVER] = { name: "Boltor Prime Receiver", isBuildComponent: true, componentOf: BOLTOR, isPrime: true, tradable: true } as never;
});
afterEach(cleanup);

const chip = (attr: string) => document.querySelector(`[${attr}]`) as HTMLElement | null;
const press = (attr: string) => fireEvent.click(chip(attr)!);
const cards = (path: string) => [...document.querySelectorAll(`[data-inventory-card='${path}']`)];
const sellableOf = (path: string) => cards(path).map((c) => c.querySelector("[data-inventory-sellable]")?.textContent ?? null);
const open = async (tab: RegExp) => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: tab }));
};

test("Surplus on Everything shows the Akarius barrels, three owned and two for the recipe, as Sellable 1", async () => {
  await open(/Everything/);
  await waitFor(() => expect(chip("data-inventory-surplus")).toBeTruthy());
  press("data-inventory-surplus");
  await waitFor(() => expect(sellableOf(BARREL)).toEqual(["Sellable 1"]));
  expect(cards(BARREL)[0].querySelector("[data-inventory-sellable]")?.getAttribute("title")).toContain("2");
});

test("a part of a mastered weapon has no floor, so every copy is sellable", async () => {
  await open(/Everything/);
  await waitFor(() => expect(chip("data-inventory-surplus")).toBeTruthy());
  press("data-inventory-surplus");
  await waitFor(() => expect(sellableOf(RECEIVER)).toEqual(["Sellable 2"]));
});

test("Surplus hides rows with nothing to sell and combines with Tradable only", async () => {
  await open(/Everything/);
  await waitFor(() => expect(chip("data-inventory-surplus")).toBeTruthy());
  const before = document.querySelectorAll("[data-inventory-card]").length;
  press("data-inventory-surplus");
  await waitFor(() => expect(sellableOf(RECEIVER)).toEqual(["Sellable 2"]));
  const cardsNow = [...document.querySelectorAll("[data-inventory-card]")];
  expect(cardsNow.length).toBeLessThan(before);
  expect(cardsNow.every((c) => c.querySelector("[data-inventory-sellable]")?.textContent !== "Sellable 0")).toBe(true);
  // A lone R5 arcane is the kept copy of its mod, so ARC_B (one copy) never shows.
  expect(cards(ARC_B)).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Tradable only" }));
  expect(document.querySelectorAll("[data-inventory-card]").length).toBeLessThanOrEqual(cardsNow.length);
});

test("Surplus shows on every listed tab and not on Resources", async () => {
  await open(/Mods/);
  await waitFor(() => expect(chip("data-inventory-surplus")).toBeTruthy());
  press("data-inventory-surplus");
  await waitFor(() => expect(sellableOf(HS)).toEqual(["Sellable 1"]));
  fireEvent.click(screen.getByRole("tab", { name: /Resources/ }));
  expect(chip("data-inventory-surplus")).toBeNull();
});
