// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

vi.mock("../stores/mastery", () => ({
  useMasteryProgress: () => ({
    loaded: true,
    data: {
      items: [
        { uniqueName: "/Lotus/Powersuits/Rhino/RhinoPrime", status: "mastered", rank: 30, maxRank: 30 },
        { uniqueName: "/Lotus/Powersuits/Volt/Volt", status: "progress", rank: 12, maxRank: 30 },
      ],
    },
  }),
}));

const tr = translator("en");
let items: ItemDatabase;

beforeEach(async () => {
  resetItemPricesForTest();
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(cleanup);

const toggle = () => document.querySelector("[data-inventory-not-mastered]") as HTMLElement | null;
const cards = () => document.querySelectorAll("[data-inventory-card]").length;
const open = async (tab: RegExp) => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: tab }));
};

test("the toggle shows only on tabs with masterable items, and leaves other tabs untouched", async () => {
  await open(/Equipment/);
  await waitFor(() => expect(toggle()).toBeTruthy());
  fireEvent.click(toggle()!);
  const filtered = cards();
  expect(filtered).toBeGreaterThan(0);
  for (const t of [/Mods/, /Arcanes/, /Misc/]) {
    fireEvent.click(screen.getByRole("tab", { name: t }));
    expect(toggle()).toBeNull();
  }
});

test("a filter left on does not empty a tab without mastery", async () => {
  await open(/Mods/);
  const before = cards();
  expect(before).toBeGreaterThan(0);
  fireEvent.click(screen.getByRole("tab", { name: /Equipment/ }));
  await waitFor(() => expect(toggle()).toBeTruthy());
  fireEvent.click(toggle()!);
  fireEvent.click(screen.getByRole("tab", { name: /Mods/ }));
  expect(cards()).toBe(before);
});

test("the filter hides mastered items", async () => {
  await open(/Equipment/);
  await waitFor(() => expect(toggle()).toBeTruthy());
  const all = cards();
  fireEvent.click(toggle()!);
  expect(all).toBe(5);
  expect(cards()).toBe(1);
  expect(document.querySelector('[data-inventory-card*="RhinoPrime"]')).toBeNull();
  expect(document.querySelector('[data-inventory-card*="Powersuits/Volt/Volt"]')).toBeTruthy();
});

test("the mastered filter shows only mastered items, and turning it on clears not mastered", async () => {
  await open(/Equipment/);
  await waitFor(() => expect(toggle()).toBeTruthy());
  const mastered = document.querySelector("[data-inventory-mastered]") as HTMLElement;
  fireEvent.click(toggle()!);
  fireEvent.click(mastered);
  expect(toggle()!.getAttribute("aria-pressed")).toBe("false");
  expect(cards()).toBe(1);
  expect(document.querySelector('[data-inventory-card*="RhinoPrime"]')).toBeTruthy();
});

const chip = (attr: string) => document.querySelector(`[${attr}]`) as HTMLElement | null;

test("the Ducats and Prime toggles show only where the data has them and never empty another tab", async () => {
  const chassis = items["/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeChassisComponent"];
  expect(chassis).toBeTruthy();
  chassis!.ducats = 45;
  for (const part of ["HelmetComponent", "SystemsComponent", "Blueprint"]) items[`/Lotus/Types/Recipes/WarframeRecipes/RhinoPrime${part}`].ducats = null;
  await open(/Mods/);
  expect(chip("data-inventory-ducats")).toBeNull();
  const before = cards();
  fireEvent.click(screen.getByRole("tab", { name: /parts/i }));
  await waitFor(() => expect(chip("data-inventory-ducats")).toBeTruthy());
  const all = cards();
  fireEvent.click(chip("data-inventory-ducats")!);
  expect(all).toBe(2);
  expect(cards()).toBe(1);
  fireEvent.click(screen.getByRole("tab", { name: /Mods/ }));
  expect(cards()).toBe(before);
});

test("Prime and Non-prime are mutually exclusive and pressing the active one clears it", async () => {
  await open(/Everything/);
  await waitFor(() => expect(chip("data-inventory-prime")).toBeTruthy());
  const all = cards();
  fireEvent.click(chip("data-inventory-prime")!);
  const prime = cards();
  fireEvent.click(chip("data-inventory-non-prime")!);
  expect(chip("data-inventory-prime")!.getAttribute("aria-pressed")).toBe("false");
  expect(prime + cards()).toBe(all);
  fireEvent.click(chip("data-inventory-non-prime")!);
  expect(cards()).toBe(all);
});
