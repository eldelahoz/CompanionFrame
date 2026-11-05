// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { translator } from "../lib/i18n";
import DashboardView from "./DashboardView";
import InventoryView from "./InventoryView";

const tr = translator("en");
let items: ItemDatabase;

beforeEach(async () => {
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(cleanup);

test("lists owned items from the mock inventory and counts them in the title", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(document.querySelector('[data-inventory-card="/Lotus/Powersuits/Volt/Volt"]')).toBeTruthy());
  // 'Everything' hides resources, so only equipment, mods and relics are owned rows here.
  expect(screen.getByRole("heading", { level: 1 }).textContent).toMatch(/^Inventory \(\d+\)$/);
  expect(document.querySelector('[data-inventory-card="/Lotus/Types/Items/MiscItems/Ferrite"]')).toBeNull();
});

test("the resources tab shows stacks with compact numbers", async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByText("Resources"));
  expect(await screen.findByText("Ferrite")).toBeTruthy();
  expect(screen.getByText("482.1K")).toBeTruthy();
});

test("search narrows the grid and an empty result says so", async () => {
  render(<InventoryView items={items} tr={tr} />);
  const search = await screen.findByPlaceholderText("Search...");
  await waitFor(() => expect(document.querySelectorAll("[data-inventory-card]").length).toBeGreaterThan(1));
  fireEvent.change(search, { target: { value: "volt" } });
  expect(document.querySelectorAll("[data-inventory-card]").length).toBe(1);
  fireEvent.change(search, { target: { value: "zzzz" } });
  expect(screen.getByText("No items found")).toBeTruthy();
});

test("the layout choice persists", async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("button", { name: "List" }));
  expect(localStorage.getItem("wf_inventory_view_mode")).toBe("list");
  expect(document.querySelector("[data-inventory-list]")).toBeTruthy();
});

test("clearing the inventory empties the grid", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await screen.findByText("Volt");
  fireEvent.click(screen.getByText("Clear"));
  await waitFor(() => expect(screen.getByText("No inventory loaded")).toBeTruthy());
});

test("the header shows how fresh the import is, with the trades applied since", async () => {
  render(<InventoryView items={items} tr={tr} />);
  expect((await screen.findByText(/Imported 3d ago/)).textContent).toBe("Imported 3d ago, 5 trades, 2 est. picks");
  expect(document.querySelector("[data-inventory-import]")).toBeTruthy();
  expect(document.querySelector("[data-import-note]")?.textContent).toContain("Based on your inventory import from");
});

test("the dashboard summarises the inventory", async () => {
  render(<DashboardView items={items} tr={tr} locale="en" />);
  await waitFor(() => expect(document.querySelector('[data-stat="resources"]')?.textContent).toBe("4"));
  expect(document.querySelector('[data-widget="resources"]')?.textContent).toContain("Ferrite");
});

test("cards show plat, ducat and average chips once the prices arrive", async () => {
  render(<InventoryView items={items} tr={tr} />);
  await waitFor(() => expect(document.querySelector("[data-inventory-card] [data-readout='platinum']")).toBeTruthy());
  await waitFor(() => expect(document.querySelector("[data-inventory-value-strip] [data-summary-item='allTradable'] [data-readout='platinum']")).toBeTruthy());
});

test("selection mode picks cards, totals them and clears", async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("button", { name: "Select" }));
  expect(document.querySelector("[data-inventory-select-bar]")).toBeTruthy();
  const card = await waitFor(() => {
    const el = document.querySelector("[data-inventory-card] .inv-card-main");
    expect(el).toBeTruthy();
    return el as HTMLElement;
  });
  fireEvent.click(card);
  expect(document.querySelector("[data-inventory-select-count]")?.textContent).toBe("1 selected");
  fireEvent.click(document.querySelector("[data-inventory-select-clear]") as HTMLElement);
  expect(document.querySelector("[data-inventory-select-count]")?.textContent).toBe("0 selected");
});

test("shift-click in selection mode picks every card between two clicks", async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("button", { name: "Select" }));
  const mains = await waitFor(() => {
    const els = [...document.querySelectorAll<HTMLElement>("[data-inventory-card] .inv-card-main")];
    expect(els.length).toBeGreaterThan(3);
    return els;
  });
  fireEvent.click(mains[1]);
  fireEvent.click(mains[3], { shiftKey: true });
  expect(document.querySelector("[data-inventory-select-count]")?.textContent).toBe("3 selected");
  fireEvent.click(mains[0], { shiftKey: true });
  expect(document.querySelector("[data-inventory-select-count]")?.textContent).toBe("4 selected");
});

test("the market button opens the item's order book", async () => {
  render(<InventoryView items={items} tr={tr} />);
  const book = await waitFor(() => {
    const el = document.querySelector<HTMLElement>(".inv-card-book");
    expect(el).toBeTruthy();
    return el as HTMLElement;
  });
  fireEvent.click(book);
  await waitFor(() => expect(document.querySelector("[data-inventory-book]")).toBeTruthy());
  await waitFor(() => expect(document.querySelector("[data-book-side='sell']")).toBeTruthy());
});

test("the pets tab says so when the inventory has none", async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: "Pets" }));
  expect(document.querySelector("[data-pets-inventory]")).toBeTruthy();
});
