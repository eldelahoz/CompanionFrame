// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

const SER = "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod";
const VIT = "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod";
const PS = "/Lotus/Upgrades/Mods/Rifle/WeaponCritChanceMod";
const HS = "/Lotus/Upgrades/Mods/Pistol/HornetStrikeMod";
const CON = "/Lotus/Upgrades/Mods/Warframe/ContinuityMod";

// Serration is in the mock catalog (tradable, max rank 10); Hornet Strike and Continuity are not.
vi.mock("../lib/ipc", async (original) => {
  const actual = await original<typeof import("../lib/ipc")>();
  return {
    ...actual,
    getInventory: async () => {
      const data = structuredClone(await actual.getInventory())!;
      data.RawUpgrades = [
        { ItemType: VIT, ItemCount: 3 },
        { ItemType: PS, ItemCount: 1 },
        { ItemType: HS, ItemCount: 2 },
      ];
      data.Upgrades = [
        { ItemType: SER, ItemCount: 4, UpgradeFingerprint: '{"lvl":0}' },
        { ItemType: SER, ItemCount: 1, UpgradeFingerprint: '{"lvl":4}', ItemId: { $oid: "eq1" } },
        { ItemType: SER, ItemCount: 1, UpgradeFingerprint: '{"lvl":10}' },
        { ItemType: CON, ItemCount: 4, UpgradeFingerprint: '{"lvl":0}' },
        { ItemType: CON, ItemCount: 1, UpgradeFingerprint: '{"lvl":5}', ItemId: { $oid: "eq2" } },
      ];
      (data.Suits as Record<string, unknown>[])[1].Configs = [{ Upgrades: ["eq1", "eq2"] }];
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
  for (const [path, name] of [[HS, "Hornet Strike"], [CON, "Continuity"]] as const) items[path] = { ...items[VIT], name };
});
afterEach(cleanup);

const chip = (attr: string) => document.querySelector(`[${attr}]`) as HTMLElement | null;
const press = (attr: string) => fireEvent.click(chip(attr)!);
const rows = (path: string) => document.querySelectorAll(`[data-inventory-card='${path}']`).length;
const openMods = async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Mods/ }));
  await waitFor(() => expect(chip("data-inventory-mod-duplicates")).toBeTruthy());
};
const shown = () => [SER, VIT, PS, HS, CON].map(rows);

test("with no chip on, every rank row shows", async () => {
  await openMods();
  expect(shown()).toEqual([3, 1, 1, 1, 2]);
});

test("Duplicates keeps mods with two or more copies across all their ranks", async () => {
  await openMods();
  press("data-inventory-mod-duplicates");
  expect(shown()).toEqual([3, 1, 0, 1, 2]);
});

test("Not max rank hides the maxed copy even when the mod is a duplicate", async () => {
  await openMods();
  press("data-inventory-mod-not-max");
  expect(shown()).toEqual([2, 1, 1, 1, 2]);
});

test("Not equipped hides rows fitted in a config", async () => {
  await openMods();
  press("data-inventory-mod-not-equipped");
  expect(shown()).toEqual([2, 1, 1, 1, 1]);
});

test("the three chips combine, and combine with Tradable only", async () => {
  await openMods();
  press("data-inventory-mod-duplicates");
  press("data-inventory-mod-not-max");
  press("data-inventory-mod-not-equipped");
  expect(shown()).toEqual([1, 1, 0, 1, 1]);
  fireEvent.click(screen.getByRole("button", { name: "Tradable only" }));
  expect(shown()).toEqual([1, 1, 0, 0, 0]);
});

test("each chip also combines with Tradable only alone", async () => {
  await openMods();
  fireEvent.click(screen.getByRole("button", { name: "Tradable only" }));
  press("data-inventory-mod-duplicates");
  expect(shown()).toEqual([3, 1, 0, 0, 0]);
  press("data-inventory-mod-duplicates");
  press("data-inventory-mod-not-equipped");
  expect(shown()).toEqual([2, 1, 1, 0, 0]);
});

const sellableOf = (path: string) => [...document.querySelectorAll(`[data-inventory-card='${path}'] [data-inventory-sellable]`)].map((n) => n.textContent);

test("no chip, no sellable count", async () => {
  await openMods();
  expect(document.querySelector("[data-inventory-sellable]")).toBeNull();
});

test("Hornet Strike R0 x2 with nothing else sells one", async () => {
  await openMods();
  press("data-inventory-mod-duplicates");
  expect(sellableOf(HS)).toEqual(["Sellable 1"]);
});

test("Continuity R0 x4 beside an equipped R5 sells all four, and the R5 none", async () => {
  await openMods();
  press("data-inventory-mod-duplicates");
  await waitFor(() => expect(sellableOf(CON)).toEqual(["Sellable 4", "Sellable 0"]));
});

test("the sellable count follows the list view too", async () => {
  await openMods();
  fireEvent.click(screen.getByRole("button", { name: "List" }));
  press("data-inventory-mod-duplicates");
  expect(document.querySelector(`[data-inventory-card='${HS}'] [data-inventory-sellable]`)?.textContent).toBe("Sellable 1");
});

test("the chips are hidden on a tab with no mods and do not filter it", async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Equipment/ }));
  await waitFor(() => expect(document.querySelectorAll("[data-inventory-card]").length).toBeGreaterThan(0));
  expect(chip("data-inventory-mod-duplicates")).toBeNull();
  expect(chip("data-inventory-mod-not-max")).toBeNull();
  expect(chip("data-inventory-mod-not-equipped")).toBeNull();
});
