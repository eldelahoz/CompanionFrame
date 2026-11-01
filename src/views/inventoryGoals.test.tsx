// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

const tr = translator("en");
const BIG = 150;
const bigPrices = Array.from({ length: BIG }, (_, n) => 1 + ((n * 7) % 90));
let items: ItemDatabase;

beforeEach(async () => {
  // 150 tradable mods priced 1..90: more than one batch of cards, and a spread to cut ranges from.
  history.pushState({}, "", `/?bigInv=${BIG}`);
  resetItemPricesForTest();
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(() => {
  cleanup();
  history.pushState({}, "", "/");
});

const goal = (name: string) => screen.getByRole("button", { name });
const tradable = () => screen.getByRole("button", { name: "Tradable only" });
const platinum = () => screen.getByRole("combobox", { name: "Platinum" }) as HTMLSelectElement;
const cell = (key: string) => document.querySelector(`[data-summary-item='${key}']`) as HTMLElement;
const plat = (key: string) => cell(key).querySelector("[data-readout='platinum']")?.textContent;
const ready = async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(document.querySelector("[data-inventory-pricing]")).toBeNull());
  await waitFor(() => expect(plat("allTradable")).not.toBe("0"), { timeout: 5000 });
  await waitFor(() => expect(document.querySelector("[data-inventory-pricing]")).toBeNull());
};
const count = () => Number(cell("filtered").textContent?.match(/Filtered \((\d+) items\)/)?.[1]);

test("a goal sets exactly its filters, and touching a filter by hand releases it", async () => {
  await ready();
  expect(goal("All").getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(goal("Sell to players"));
  expect(goal("Sell to players").getAttribute("aria-pressed")).toBe("true");
  expect(tradable().getAttribute("aria-pressed")).toBe("true");
  expect(platinum().value).toBe("ge5");
  fireEvent.click(goal("Mastery hunting"));
  expect(tradable().getAttribute("aria-pressed")).toBe("false");
  expect(platinum().value).toBe("any");
  fireEvent.click(goal("Sell to players"));
  fireEvent.click(tradable());
  expect(goal("All").getAttribute("aria-pressed")).toBe("true");
  expect(platinum().value).toBe("ge5");
  fireEvent.click(goal("All"));
  expect(platinum().value).toBe("any");
});

test("an empty filter shows zero in Filtered and never swaps to the whole inventory, which the filters do not move", async () => {
  await ready();
  const all = plat("allTradable");
  fireEvent.change(platinum(), { target: { value: "custom" } });
  fireEvent.change(screen.getByRole("spinbutton", { name: "From" }), { target: { value: "99999" } });
  await waitFor(() => expect(cell("filtered").textContent).toContain("Filtered (0 items)"));
  expect(plat("filtered")).toBe("0");
  expect(plat("allTradable")).toBe(all);
  expect(cell("allTradable")).toBeTruthy();
});

test("the platinum range persists as min and max", async () => {
  await ready();
  fireEvent.change(platinum(), { target: { value: "5-15" } });
  expect([localStorage.getItem("wf_inventory_plat_min"), localStorage.getItem("wf_inventory_plat_max")]).toEqual(["5", "15"]);
  cleanup();
  render(<InventoryView items={items} tr={tr} />);
  expect(platinum().value).toBe("5-15");
});

test("the old single floor is read as the min", async () => {
  localStorage.setItem("wf_inventory_value_floor", "15");
  render(<InventoryView items={items} tr={tr} />);
  expect(platinum().value).toBe("ge15");
});

test("Filtered adds up every filtered row, not only the cards shown", async () => {
  await ready();
  expect(document.querySelectorAll("[data-inventory-card]").length).toBeLessThan(BIG);
  expect(count()).toBeGreaterThanOrEqual(BIG);
  expect(plat("filtered")).toBe(plat("allTradable"));
});

test("a platinum range filters the list and the Filtered total, and the all tradable total stays", async () => {
  await ready();
  const all = plat("allTradable");
  // only the generated mods, whose prices are known
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Bulk Mod" } });
  fireEvent.change(platinum(), { target: { value: "le5" } });
  await waitFor(() => expect(count()).toBe(bigPrices.filter((p) => p <= 5).length));
  expect(plat("allTradable")).toBe(all);
  fireEvent.change(platinum(), { target: { value: "custom" } });
  fireEvent.change(screen.getByRole("spinbutton", { name: "From" }), { target: { value: "10" } });
  fireEvent.change(screen.getByRole("spinbutton", { name: "To" }), { target: { value: "20" } });
  await waitFor(() => expect(count()).toBe(bigPrices.filter((p) => p >= 10 && p <= 20).length));
});
