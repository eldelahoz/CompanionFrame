// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import InventoryView from "./InventoryView";

const bulk = vi.hoisted(() => ({ calls: [] as string[][], open: 0, peak: 0 }));
vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  wfmPricesLoad: async (slugs: string[]) => {
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
  document.getElementById("content")?.remove();
  history.pushState({}, "", "/");
});

const cards = () => document.querySelectorAll("[data-inventory-card]");

/** jsdom has no layout: App's scroll element gets a fixed viewport and a settable offset, as the virtualizer reads them. */
function mountInScroller(height = 800) {
  const main = document.createElement("main");
  main.id = "content";
  let top = 0;
  Object.defineProperty(main, "clientHeight", { value: height });
  Object.defineProperty(main, "clientWidth", { value: 1280 });
  Object.defineProperty(main, "scrollHeight", { value: 10_000_000 }); // the virtualizer ignores offsets past the scrollable end
  Object.defineProperty(main, "scrollTop", { get: () => top, set: (v: number) => (top = v), configurable: true });
  // a real browser reports a row's top relative to the viewport, so the scroll offset cancels out of the list's margin
  const rect = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    return { top: this === main ? 0 : -top, height: 0, width: 0, bottom: 0, left: 0, right: 0, x: 0, y: 0, toJSON() {} };
  });
  const host = document.createElement("div");
  main.append(host);
  document.body.append(main);
  return { host, scrollTo: (v: number) => { top = v; fireEvent.scroll(main); }, remove: () => (rect.mockRestore(), main.remove()) };
}
const names = () => [...cards()].map((c) => c.getAttribute("data-inventory-card"));

test("a 2.6k inventory keeps a window of rows in the DOM, in cards and in the list; scrolling swaps the window", async () => {
  const s = mountInScroller();
  render(<InventoryView items={items} tr={tr} />, { container: s.host });
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  expect(cards().length).toBeLessThanOrEqual(60);
  expect(screen.getByRole("tab", { name: /Mods/ }).textContent).toContain("26");
  const top = names();
  // 4 cards a row at 266 px: row 300 starts at ~80 000 px
  act(() => s.scrollTo(80_000));
  await waitFor(() => expect(names()).not.toContain(top[0]));
  expect(cards().length).toBeGreaterThan(0);
  expect(cards().length).toBeLessThanOrEqual(60);
  fireEvent.click(screen.getByRole("button", { name: "List" }));
  act(() => s.scrollTo(0));
  await waitFor(() => expect(document.querySelector("[data-inventory-list]")).toBeTruthy());
  expect(cards().length).toBeLessThanOrEqual(60);
  act(() => s.scrollTo(60_000)); // 41 px rows
  await waitFor(() => expect(names()).not.toContain(top[0]));
  expect(cards().length).toBeLessThanOrEqual(60);
  s.remove();
});

test("searching narrows the window to the matches and counts follow the search", async () => {
  const s = mountInScroller();
  render(<InventoryView items={items} tr={tr} />, { container: s.host });
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "Bulk Mod 2616" } });
  await waitFor(() => expect(cards().length).toBe(1), { timeout: 15000 });
  expect(screen.getByRole("tab", { name: /Everything/ }).textContent).not.toContain("26");
  s.remove();
}, 30000);

test("shift-click selects the whole range of the list, rows that are not in the DOM included", async () => {
  const s = mountInScroller();
  render(<InventoryView items={items} tr={tr} />, { container: s.host });
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  fireEvent.click(document.querySelector("[data-inventory-select-toggle]") as HTMLElement);
  fireEvent.click(cards()[0].querySelector(".inv-card-main") as HTMLElement);
  act(() => s.scrollTo(80_000));
  await waitFor(() => expect(names()).not.toContain(null));
  const far = cards()[0];
  fireEvent.click(far.querySelector(".inv-card-main") as HTMLElement, { shiftKey: true });
  const count = Number(document.querySelector("[data-inventory-select-count]")?.textContent?.match(/\d+/)?.[0]);
  expect(count).toBeGreaterThan(1000); // from the first row to a row ~300 rows down, not only the few in the DOM
  s.remove();
}, 30000);

test("prices are asked in one request for the whole inventory, rows on screen first, and the totals converge", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(cards().length).toBeGreaterThan(0));
  const onScreen = new Set([...cards()].flatMap((c) => c.getAttribute("data-inventory-card")?.match(/Bulk(\d+)Mod/)?.[1] ?? []));
  await waitFor(() => expect(bulk.calls.length).toBeGreaterThan(0));
  const first = bulk.calls[0].filter((s) => s.startsWith("bulk_mod_")).slice(0, onScreen.size);
  expect(first.every((s) => onScreen.has(s.replace("bulk_mod_", "")))).toBe(true);
  // Everything is priced in the end: no price failure row shows.
  await waitFor(() => expect(document.querySelector("[data-inventory-pricing]")).toBeNull(), { timeout: 30000 });
  // The backend chunks and queues: the frontend sends everything at once.
  expect(bulk.calls[0].length).toBeGreaterThan(40);
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
