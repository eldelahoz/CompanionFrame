// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import InventoryView from "./InventoryView";

const bulk = vi.hoisted(() => ({ calls: [] as string[][], open: 0, peak: 0 }));
vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  wfmPricesBulk: async (slugs: string[]) => {
    bulk.calls.push(slugs);
    bulk.peak = Math.max(bulk.peak, ++bulk.open);
    await new Promise((r) => setTimeout(r, 0));
    bulk.open--;
    return slugs.map((slug) => ({ slug, status: "ok", price: 5, stale: false }));
  },
}));

const tr = translator("en");
const BIG = 2617;
let items: ItemDatabase;

beforeEach(async () => {
  // a pump left over from the last test finishes its call before the counters restart
  await waitFor(() => expect(bulk.open).toBe(0));
  history.pushState({}, "", `/?bigInv=${BIG}`);
  resetMock();
  resetItemPricesForTest();
  bulk.calls.length = bulk.open = bulk.peak = 0;
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(() => {
  cleanup();
  history.pushState({}, "", "/");
});

const cards = () => document.querySelectorAll("[data-inventory-card]");

test("a 2.6k inventory renders one batch, then more on demand, in cards and in the list", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  expect(cards().length).toBeLessThanOrEqual(60);
  expect(screen.getByRole("tab", { name: /Mods/ }).textContent).toContain("26");
  expect(document.querySelector("[data-inventory-more]")?.textContent).toMatch(/of 26\d\d/);
  fireEvent.click(screen.getByRole("button", { name: /Show \d+ more/ }));
  expect(cards().length).toBeLessThanOrEqual(120);
  expect(cards().length).toBeGreaterThan(60);
  fireEvent.click(screen.getByRole("button", { name: "List" }));
  expect(cards().length).toBeLessThanOrEqual(60);
});

test("searching starts again from one batch and counts follow the search", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  // role queries walk the whole 2.6k-row DOM: slow under a loaded CI worker, so the waits get room and the click uses a plain selector
  fireEvent.click(document.querySelector("[data-inventory-more] button") as HTMLElement);
  expect(cards().length).toBeGreaterThan(60);
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Bulk Mod 2616" } });
  await waitFor(() => expect(cards().length).toBe(1), { timeout: 15000 });
  expect(screen.getByRole("tab", { name: /Everything/ }).textContent).not.toContain("26");
}, 30000);

test("prices are asked in chunks of at most 40, one call at a time, rows on screen first, and the totals converge", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  const onScreen = new Set([...cards()].map((c) => c.getAttribute("data-inventory-card")?.match(/Bulk(\d+)Mod/)?.[1]));
  await waitFor(() => expect(bulk.calls.length).toBeGreaterThan(0));
  expect(bulk.calls[0].every((s) => !s.startsWith("bulk_mod_") || onScreen.has(s.replace("bulk_mod_", "")))).toBe(true);
  // Everything is priced in the end: the "pricing n/m" line goes away.
  await waitFor(() => expect(document.querySelector("[data-inventory-pricing]")).toBeNull(), { timeout: 30000 });
  expect(Math.max(...bulk.calls.map((c) => c.length))).toBeLessThanOrEqual(40);
  expect(bulk.peak).toBe(1);
}, 40000);

test("hovering or focusing a card opens the large preview", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  const main = document.querySelector("[data-inventory-card] .inv-card-main") as HTMLElement;
  fireEvent.pointerEnter(main);
  expect(document.querySelector("[data-item-preview]")).toBeTruthy();
  fireEvent.pointerLeave(main);
  expect(document.querySelector("[data-item-preview]")).toBeNull();
});
