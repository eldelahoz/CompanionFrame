// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase, type MyOrder } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import type { ParsedItem } from "../types/inventory";
import InventoryView from "./InventoryView";

// The first card's item is the one "on sale"; set once the cards render.
let listed = "";
vi.mock("../lib/inventory/useOnSale", () => ({
  useOnSale: () => ({
    has: (i: ParsedItem) => i.internalName === listed,
    get: (i: ParsedItem) => (i.internalName === listed ? ({ id: "o", orderType: "sell", platinum: 9, quantity: 1, visible: true } as MyOrder) : undefined),
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

const cards = () => [...document.querySelectorAll("[data-inventory-card]")].map((c) => c.getAttribute("data-inventory-card"));
const chip = (name: string) => document.querySelector(`[data-inventory-${name}]`) as HTMLElement | null;

test("signed out there are no sale chips", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await screen.findByRole("tab", { name: /Mods/ });
  expect(chip("on-sale")).toBeNull();
});

test("On sale keeps listed items, Not on sale the rest, and each chip is exclusive with the other", async () => {
  render(<InventoryView items={items} tr={tr} wfmLoggedIn />);
  fireEvent.click(await screen.findByRole("tab", { name: /Mods/ }));
  await waitFor(() => expect(cards().length).toBeGreaterThan(1));
  const all = cards();
  listed = all[0]!;
  fireEvent.click(chip("on-sale")!);
  await waitFor(() => expect(cards()).toEqual([listed]));
  fireEvent.click(chip("not-on-sale")!);
  await waitFor(() => expect(cards()).toEqual(all.slice(1)));
  expect(chip("on-sale")!.getAttribute("aria-pressed")).toBe("false");
  fireEvent.click(chip("not-on-sale")!);
  await waitFor(() => expect(cards()).toEqual(all));
});
