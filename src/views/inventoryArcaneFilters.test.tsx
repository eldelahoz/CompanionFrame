// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

const VIT = "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod";
const ARC_A = "/Lotus/Upgrades/CosmeticEnhancers/Zariman/ArcaneAlpha";
const ARC_B = "/Lotus/Upgrades/CosmeticEnhancers/Zariman/ArcaneBeta";
const ARC_C = "/Lotus/Upgrades/CosmeticEnhancers/Zariman/ArcaneGamma";

vi.mock("../lib/ipc", async (original) => {
  const actual = await original<typeof import("../lib/ipc")>();
  return {
    ...actual,
    getInventory: async () => {
      const data = structuredClone(await actual.getInventory())!;
      data.Upgrades = [
        { ItemType: ARC_A, ItemCount: 2, UpgradeFingerprint: '{"lvl":0}' },
        { ItemType: ARC_A, ItemCount: 1, UpgradeFingerprint: '{"lvl":5}' },
        { ItemType: ARC_B, ItemCount: 1, UpgradeFingerprint: '{"lvl":0}' },
        { ItemType: ARC_C, ItemCount: 3, UpgradeFingerprint: '{"lvl":2}' },
      ];
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
  for (const [path, name] of [[ARC_A, "Arcane Alpha"], [ARC_B, "Arcane Beta"], [ARC_C, "Arcane Gamma"]] as const) {
    items[path] = { ...items[VIT], name, category: "Arcanes", type: "Arcane" };
  }
});
afterEach(cleanup);

const chip = (attr: string) => document.querySelector(`[${attr}]`) as HTMLElement | null;
const cards = (path: string) => [...document.querySelectorAll(`[data-inventory-card='${path}']`)];
const sellableOf = (path: string) => cards(path).map((c) => c.querySelector("[data-inventory-sellable]")?.textContent ?? null);
const openArcanes = async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Arcanes/ }));
  await waitFor(() => expect(chip("data-inventory-mod-duplicates")).toBeTruthy());
};

test("the mod chips also show on the Arcanes tab and filter its rows", async () => {
  await openArcanes();
  expect([ARC_A, ARC_B, ARC_C].map((p) => cards(p).length)).toEqual([2, 1, 1]);
  fireEvent.click(chip("data-inventory-mod-duplicates")!);
  expect([ARC_A, ARC_B, ARC_C].map((p) => cards(p).length)).toEqual([2, 0, 1]);
});

test("an arcane keeps one copy on its highest rank, so the unranked stack sells", async () => {
  await openArcanes();
  fireEvent.click(chip("data-inventory-mod-duplicates")!);
  await waitFor(() => expect(sellableOf(ARC_A)).toHaveLength(2));
  expect(sellableOf(ARC_A).sort()).toEqual(["0 of 3", "2 of 3"]);
  expect(sellableOf(ARC_C)).toEqual(["2 of 3"]);
});
