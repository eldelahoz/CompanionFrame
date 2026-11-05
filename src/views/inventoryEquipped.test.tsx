// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

const SER = "/Lotus/Upgrades/Mods/Rifle/WeaponDamageAmountMod";
const VIT = "/Lotus/Upgrades/Mods/Warframe/AvatarHealthMaxMod";

// Serration R4 (eq1) sits in config A of one frame and in configs B and C of another; Vitality is not fitted.
vi.mock("../lib/ipc", async (original) => {
  const actual = await original<typeof import("../lib/ipc")>();
  return {
    ...actual,
    getInventory: async () => {
      const data = structuredClone(await actual.getInventory())!;
      data.RawUpgrades = [{ ItemType: VIT, ItemCount: 3 }];
      data.Upgrades = [{ ItemType: SER, ItemCount: 1, UpgradeFingerprint: '{"lvl":4}', ItemId: { $oid: "eq1" } }];
      const suits = data.Suits as Record<string, unknown>[];
      suits[0].Configs = [{ Upgrades: ["eq1"] }];
      suits[1].Configs = [{}, { Upgrades: ["eq1"] }, { PvpUpgrades: ["eq1"] }];
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
});
afterEach(cleanup);

const card = (path: string) => document.querySelector(`[data-inventory-card='${path}']`)!;
const toggle = () => document.querySelector("[data-inventory-equipped-toggle]") as HTMLElement | null;
const openMods = async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Mods/ }));
  await waitFor(() => expect(toggle()).toBeTruthy());
};

test("the toggle is off by default: no pill, no column", async () => {
  await openMods();
  expect(toggle()?.getAttribute("aria-pressed")).toBe("false");
  expect(document.querySelector("[data-inventory-equipped]")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "List" }));
  expect(screen.queryByRole("columnheader", { name: "Equipped" })).toBeNull();
});

test("on, a fitted card gets a pill with the gear count and a popover with the configs; Vitality gets none", async () => {
  await openMods();
  fireEvent.click(toggle()!);
  const pill = card(SER).querySelector("[data-inventory-equipped] button")!;
  expect(pill.getAttribute("aria-label")).toBe("Equipped on 2 items");
  expect(pill.textContent).toBe("2");
  expect(card(VIT).querySelector("[data-inventory-equipped]")).toBeNull();
  fireEvent.click(pill);
  const pop = screen.getByRole("dialog", { name: "Equipped on 2 items" });
  expect(pop.textContent).toContain("Config A");
  expect(pop.textContent).toContain("Config B, C");
});

test("the popover closes on Escape and on an outside click", async () => {
  await openMods();
  fireEvent.click(toggle()!);
  const pill = card(SER).querySelector("[data-inventory-equipped] button")!;
  fireEvent.click(pill);
  fireEvent.keyDown(document, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(pill);
  fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("on, the list gets an Equipped column: On 2 items for the fitted mod, No for the rest", async () => {
  await openMods();
  fireEvent.click(screen.getByRole("button", { name: "List" }));
  fireEvent.click(toggle()!);
  expect(screen.getByRole("columnheader", { name: "Equipped" })).toBeTruthy();
  expect(card(SER).querySelector("[data-inventory-equipped] button")?.textContent).toBe("On 2 items");
  expect(card(VIT).textContent).toContain("No");
  fireEvent.click(card(SER).querySelector("[data-inventory-equipped] button")!);
  expect(screen.getByRole("dialog").textContent).toContain("Config B, C");
});
