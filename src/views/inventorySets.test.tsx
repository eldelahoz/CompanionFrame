// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import * as ipc from "../lib/ipc";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { resetItemPricesForTest } from "../lib/inventory/itemPrices";
import { translator } from "../lib/i18n";
import InventoryView from "./InventoryView";

const tr = translator("en");
let items: ItemDatabase;

beforeEach(async () => {
  resetItemPricesForTest();
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(cleanup);

const openSets = async () => {
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Full Sets/ }));
};
const setCard = () => document.querySelector('[data-inventory-card$="RhinoPrime#set"]') as HTMLElement | null;
const openDetails = () => document.querySelectorAll<HTMLDetailsElement>("details.sets-detail").forEach((d) => { d.open = true; fireEvent(d, new Event("toggle")); });
const missing = (part: string) => document.querySelector(`[data-set-missing$="${part}"]`) as HTMLElement;

test("incomplete sets stay hidden until the toggle is on, and the choice is remembered", async () => {
  await openSets();
  const toggle = await screen.findByRole("button", { name: /Show incomplete sets/ });
  expect(setCard()).toBeNull();
  fireEvent.click(toggle);
  await waitFor(() => expect(setCard()).toBeTruthy());
  expect(localStorage.getItem("wf_inventory_incomplete_sets")).toBe("1");
});

test("an incomplete set shows its progress and only the parts it lacks", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  await openSets();
  await waitFor(() => expect(setCard()).toBeTruthy());
  const card = setCard()!;
  expect(card.textContent).toContain("Rhino Prime Set");
  expect(card.textContent).toContain("1/3");
  expect(card.querySelector("[title=\"1/3 parts\"]")).toBeTruthy();
  expect(document.querySelectorAll("[data-set-missing]").length).toBe(2);
  expect(missing("PrimeChassisComponent")).toBeNull();
});

test("each missing part lists the relics that drop it with rarity, vaulted and owned first", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  await openSets();
  await waitFor(() => expect(missing("PrimeSystemsComponent")?.querySelector("[data-set-relic]")).toBeTruthy());
  const relics = [...missing("PrimeSystemsComponent").querySelectorAll("[data-set-relic]")] as HTMLElement[];
  // Axi S3 is the only owned relic, so it leads even though Meso C2 is the rarer-vaulted one.
  expect(relics.map((r) => r.dataset.setRelic)).toEqual(["Axi S3", "Meso C2"]);
  expect(relics[0].textContent).toContain("Rare");
  expect(relics[0].textContent).toContain("5 owned");
  expect(relics[0].textContent).not.toContain("Vaulted");
  expect(relics[1].textContent).toContain("Vaulted");
});

test("a part no relic drops says so", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  items = { ...items };
  const key = "/Lotus/Powersuits/Rhino/RhinoPrime";
  items[key] = { ...items[key], components: [...(items[key].components ?? []), { name: "Barrel", uniqueName: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBarrel", itemCount: 1, tradable: true, drops: [] }] };
  items["/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBarrel"] = { ...items["/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeSystemsComponent"], name: "Barrel", drops: [] };
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Full Sets/ }));
  await waitFor(() => expect(missing("PrimeBarrel")).toBeTruthy());
  openDetails();
  await waitFor(() => expect(missing("PrimeBarrel")?.querySelector("[data-set-no-relics]")?.textContent).toContain("Not from relics"));
  // No drops anywhere: the part points at the wiki instead.
  const link = missing("PrimeBarrel").querySelector("a[data-set-wiki]") as HTMLAnchorElement;
  expect(link.href).toMatch(/^https:\/\/wiki\.warframe\.com\/w\//);
  expect(link.textContent).toBe("How to get it");
});

test("a part no relic drops lists its other drop sources once the set is expanded", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  items = { ...items };
  const key = "/Lotus/Powersuits/Rhino/RhinoPrime";
  items[key] = { ...items[key], components: [...(items[key].components ?? []), { name: "Barrel", uniqueName: "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBarrel", itemCount: 1, tradable: true, drops: [{ location: "Jackal (Fossa)", type: "Barrel", chance: 12.5, rarity: "Rare" }] }] };
  items["/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeBarrel"] = { ...items["/Lotus/Types/Recipes/WarframeRecipes/RhinoPrimeSystemsComponent"], name: "Barrel", drops: [{ location: "Jackal (Fossa)", type: "Barrel", chance: 12.5, rarity: "Rare" }] };
  render(<InventoryView items={items} tr={tr} />);
  fireEvent.click(await screen.findByRole("tab", { name: /Full Sets/ }));
  await waitFor(() => expect(missing("PrimeBarrel")).toBeTruthy());
  expect(missing("PrimeBarrel").textContent).not.toContain("Jackal");
  openDetails();
  await waitFor(() => expect(missing("PrimeBarrel").textContent).toContain("Jackal (Fossa)"));
  expect(missing("PrimeBarrel").textContent).toContain("12.5%");
  expect(missing("PrimeBarrel").querySelector("a[data-set-wiki]")).toBeNull();
});
test("the set's card shows the root item's public image, and a set without one falls back to the placeholder", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  const key = "/Lotus/Powersuits/Rhino/RhinoPrime";
  items = { ...items, [key]: { ...items[key], imageUrl: "https://cdn.test/rhino-prime.png", fallbackImageUrl: undefined } };
  await openSets();
  await waitFor(() => expect(setCard()?.querySelector(".inv-card-art img")).toBeTruthy());
  expect(setCard()!.querySelector(".inv-card-art img")!.getAttribute("src")).toBe("https://cdn.test/rhino-prime.png");
  cleanup();
  items = { ...items, [key]: { ...items[key], imageUrl: null } };
  await openSets();
  await waitFor(() => expect(setCard()).toBeTruthy());
  expect(setCard()!.querySelector(".inv-card-art img")).toBeNull();
  expect(setCard()!.querySelector(".inv-card-art svg")).toBeTruthy();
});

test("incomplete sets go through the same search, tradable filter and price sort as the full sets", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  await openSets();
  await waitFor(() => expect(setCard()).toBeTruthy());
  const names = () => [...document.querySelectorAll("[data-inventory-card] .inv-card-name")].map((n) => n.textContent);
  expect(names()).toContain("Rhino Prime Set");
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "rhino" } });
  await waitFor(() => expect(names()).toEqual(["Rhino Prime Set"]));
  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "no such set" } });
  await waitFor(() => expect(setCard()).toBeNull());
});

test("an incomplete set shows its set price and the sum of its missing parts, each part priced", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  await openSets();
  await waitFor(() => expect(document.querySelector("[data-set-missing-price]")).toBeTruthy());
  const parts = [...document.querySelectorAll("[data-set-missing]")] as HTMLElement[];
  expect(parts).toHaveLength(2);
  const price = (el: Element) => Number(el.querySelector("[data-readout], .ui-readout")?.textContent?.replace(/\D/g, ""));
  const sum = parts.map((p) => price(p)).reduce((a, b) => a + b, 0);
  expect(sum).toBeGreaterThan(0);
  expect(price(document.querySelector("[data-set-missing-price]")!)).toBe(sum);
});

test("set and missing-part slugs are asked through the price queue", async () => {
  localStorage.setItem("wf_inventory_incomplete_sets", "1");
  const asked: string[] = [];
  const real = ipc.wfmPricesLoad;
  vi.spyOn(ipc, "wfmPricesLoad").mockImplementation((slugs) => (asked.push(...slugs), real(slugs)));
  await openSets();
  await waitFor(() => expect(asked).toContain("rhino_prime_systems"));
  expect(asked).toContain("rhino_prime_set");
});
