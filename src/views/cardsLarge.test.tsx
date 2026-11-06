// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import IngredientChips, { type Chip } from "../components/foundry/IngredientChips";
import FoundryView from "./FoundryView";
import MasteryView from "./MasteryView";
import RelicsView from "./RelicsView";

const tr = translator("en");
let items: ItemDatabase;

beforeEach(async () => {
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(() => {
  cleanup();
  document.getElementById("content")?.remove();
  history.pushState({}, "", "/");
});

/** jsdom has no layout: App's scroll element gets a fixed viewport and a settable offset, as the virtualizer reads them. */
function mountInScroller(height = 800) {
  const main = document.createElement("main");
  main.id = "content";
  let top = 0;
  Object.defineProperty(main, "clientHeight", { value: height });
  Object.defineProperty(main, "clientWidth", { value: 1280 });
  Object.defineProperty(main, "scrollHeight", { value: 10_000_000 });
  Object.defineProperty(main, "scrollTop", { get: () => top, set: (v: number) => (top = v), configurable: true });
  const rect = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    return { top: this === main ? 0 : -top, height: 0, width: 0, bottom: 0, left: 0, right: 0, x: 0, y: 0, toJSON() {} };
  });
  const host = document.createElement("div");
  main.append(host);
  document.body.append(main);
  return { host, scrollTo: (v: number) => { top = v; fireEvent.scroll(main); }, remove: () => rect.mockRestore() };
}

/** Mounts `ui` with a scroller, then checks that a window of `[attr]` cards is in the DOM and that scrolling swaps it. */
async function expectWindowed(ui: React.ReactElement, attr: string, far: number) {
  history.pushState({}, "", location.search);
  const s = mountInScroller();
  try {
    render(ui, { container: s.host });
    const names = () => [...document.querySelectorAll(`[${attr}]`)].map((c) => c.getAttribute(attr));
    await waitFor(() => expect(names().length).toBeGreaterThan(0));
    expect(names().length).toBeLessThanOrEqual(60);
    const top = names();
    act(() => s.scrollTo(far));
    await waitFor(() => expect(names()).not.toContain(top[0]));
    expect(names().length).toBeGreaterThan(0);
    expect(names().length).toBeLessThanOrEqual(60);
  } finally {
    s.remove();
  }
}

test("Relics: a big relic database keeps a window of cards in the DOM; scrolling swaps it", async () => {
  history.pushState({}, "", "/?bigRelics=2000");
  await expectWindowed(<RelicsView tr={tr} items={items} />, "data-relic", 60_000);
});

test("Relics: the vaulted badge sits on the second line next to the owned count, and the name keeps its own line", async () => {
  render(<RelicsView tr={tr} items={items} />);
  await waitFor(() => expect(document.querySelector('[data-relic="Lith A1"]')).toBeTruthy());
  const card = document.querySelector('[data-relic="Lith A1"]') as HTMLElement;
  const meta = card.querySelector(".relic-row-meta") as HTMLElement;
  expect(meta.textContent).toContain(tr("common.vaulted"));
  expect(meta.textContent).toContain(tr("relics.detail.noneOwned"));
  expect(card.querySelector(".relic-row-name")?.textContent).toBe("Lith A1");
  expect(card.querySelector(".relic-row-name")?.getAttribute("title")).toBe("Lith A1");
  expect(card.querySelector(".relic-row-name")?.textContent).not.toContain(tr("common.vaulted"));
});

test("Mastery: a big item list keeps a window of cards in the DOM; scrolling swaps it", async () => {
  history.pushState({}, "", "/?bigMastery=2000");
  await expectWindowed(<MasteryView tr={tr} locale="en" />, "data-mastery-card", 100_000);
});

test("Mastery: components past the sixth collapse into a +N whose title lists them", async () => {
  history.pushState({}, "", "/?bigMastery=40");
  render(<MasteryView tr={tr} locale="en" />);
  await waitFor(() => expect(document.querySelectorAll("[data-mastery-card]").length).toBeGreaterThan(0));
  const search = (q: string) => fireEvent.change(screen.getByRole("searchbox"), { target: { value: q } });
  search("Bulk Item 8");
  await waitFor(() => expect(document.querySelector('[data-mastery-card="/Lotus/Mock/Bulk8"]')).toBeTruthy());
  const card =document.querySelector('[data-mastery-card="/Lotus/Mock/Bulk8"]') as HTMLElement; // 8 parts
  expect(card.querySelectorAll(".ms-card-parts i").length).toBe(6);
  const more = card.querySelector(".ms-card-more") as HTMLElement;
  expect(more.textContent).toBe("+2");
  expect(more.title.split("\n")).toHaveLength(2);
  // the progress slot carries the compact rank, or "Not owned" in the same slot
  expect(card.querySelector(".ms-slot--progress")?.textContent).toBe(tr("mastery.notOwned"));
  search("Bulk Item 6"); // exactly 6 parts: no +N
  await waitFor(() => expect(document.querySelector('[data-mastery-card="/Lotus/Mock/Bulk6"]')).toBeTruthy());
  expect(document.querySelector('[data-mastery-card="/Lotus/Mock/Bulk6"] .ms-card-more')).toBeNull();
});

test("Foundry: a 2.6k blueprint inventory keeps a window of cards in the DOM; scrolling swaps it", async () => {
  history.pushState({}, "", "/?bigFoundry=2617");
  await expectWindowed(<FoundryView items={items} tr={tr} />, "data-foundry-card", 100_000);
});

const chip = (i: number): Chip => ({ key: `k${i}`, uniqueName: `/Lotus/Chip${i}`, name: `Part ${i}`, image: null, have: i, need: 5, state: i % 3 === 0 ? "owned" : i % 3 === 1 ? "blueprint" : "missing" });

test("Foundry: ingredient chips that do not fit the card turn the last slot into +N, listing the hidden ones in its title", () => {
  // 16 px rem: a chip is 56 px and the gap 6 px, so 304 px holds 5 slots
  const width = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(304);
  try {
    const six = render(<IngredientChips chips={[0, 1, 2, 3, 4, 5].map(chip)} tr={tr} />);
    expect(six.container.querySelectorAll("[data-ingredient]").length).toBe(4);
    const more = six.container.querySelector("[data-ingredients-more]") as HTMLElement;
    expect(more.textContent).toBe("+2");
    expect(more.title).toBe(`Part 4: ${tr("common.blueprintOwnedNotBuilt")}\nPart 5: ${tr("common.missing")}`);
    six.unmount();
    const five = render(<IngredientChips chips={[0, 1, 2, 3, 4].map(chip)} tr={tr} />);
    expect(five.container.querySelectorAll("[data-ingredient]").length).toBe(5);
    expect(five.container.querySelector("[data-ingredients-more]")).toBeNull();
    // every chip names its ingredient and its state outside the colour of its border
    expect(five.container.querySelector('[data-ingredient="/Lotus/Chip1"]')?.getAttribute("title")).toBe(`Part 1: ${tr("common.blueprintOwnedNotBuilt")}`);
    expect(five.container.querySelector('[data-ingredient="/Lotus/Chip2"] svg[aria-label]')?.getAttribute("aria-label")).toBe(tr("common.missing"));
  } finally {
    width.mockRestore();
  }
});
