// @vitest-environment jsdom
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { sortRowsCalls } from "../lib/inventory/inventoryRows";
import { applyRefreshed, resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import InventoryView from "./InventoryView";

vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  wfmPricesLoad: () => new Promise(() => {}),
}));

const tr = translator("en");
let items: ItemDatabase;

beforeEach(async () => {
  history.pushState({}, "", "/?bigInv=300");
  resetMock();
  resetItemPricesForTest();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  history.pushState({}, "", "/");
});

const slugsOnScreen = () => [...document.querySelectorAll("[data-inventory-card]")].length;
const burst = (n: number) => {
  for (let i = 0; i < n; i++) act(() => applyRefreshed([{ slug: `slug-${i}`, status: "ok", price: i + 1, stale: false }] as never, false));
};

test("sorted by name, price batches streaming in re-sort nothing", async () => {
  localStorage.setItem("wf_inventory_sort", "name");
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(slugsOnScreen()).toBeGreaterThan(0));
  const before = sortRowsCalls();
  burst(30);
  await act(async () => void (await new Promise((r) => setTimeout(r, 300))));
  expect(sortRowsCalls()).toBe(before);
});

test("sorted by platinum, a burst of batches re-sorts once at the start and once at the end of the window", async () => {
  localStorage.setItem("wf_inventory_sort", "platinum");
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(slugsOnScreen()).toBeGreaterThan(0));
  await act(async () => void (await new Promise((r) => setTimeout(r, 300)))); // let the mount settle
  const before = sortRowsCalls();
  burst(30);
  await act(async () => void (await new Promise((r) => setTimeout(r, 300))));
  expect(sortRowsCalls() - before).toBeLessThanOrEqual(2);
  expect(sortRowsCalls() - before).toBeGreaterThan(0);
});
