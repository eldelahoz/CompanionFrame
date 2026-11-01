// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import { getWfmItems } from "../../lib/ipc";
import { buildCatalogIndex, type CatalogIndex } from "../../lib/wfm/catalog";
import MarketBrowseView, { filterRows, formatUpdatedLabel } from "./MarketBrowseView";

const tr = translator("en");
let catalog: CatalogIndex;
beforeEach(async () => {
  localStorage.clear();
  catalog = buildCatalogIndex(await getWfmItems());
});
afterEach(cleanup);

const entry = (over: object) => ({ userName: "u", status: "ingame", platinum: 10, quantity: 1, perTrade: 1, unitPlatinum: 10, rank: null, avatar: null, ...over });

test("filterRows applies status, max rank and the price range", () => {
  const rows = [entry({ unitPlatinum: 5, status: "online" }), entry({ unitPlatinum: 15, rank: 10 }), entry({ unitPlatinum: 25, rank: 0 })];
  expect(filterRows(rows, "ingame", "all", 10, null, null)).toHaveLength(2);
  expect(filterRows(rows, "all", "all", 10, null, null)).toHaveLength(3);
  expect(filterRows(rows, "all", "maxed", 10, null, null).map((r) => r.unitPlatinum)).toEqual([15]);
  expect(filterRows(rows, "all", "all", 10, 10, 20).map((r) => r.unitPlatinum)).toEqual([15]);
  // "maxed" without a known max rank filters nothing
  expect(filterRows(rows, "all", "maxed", 0, null, null)).toHaveLength(3);
});

test("formatUpdatedLabel words the age", () => {
  expect(formatUpdatedLabel(tr, null, 0)).toBe(tr("common.updatedRecently"));
  expect(formatUpdatedLabel(tr, 1000, 1000)).toBe(tr("common.updatedJustNow"));
  expect(formatUpdatedLabel(tr, 1000, 31_000)).toBe(tr("common.updatedSAgo", { sec: 30 }));
  expect(formatUpdatedLabel(tr, 1000, 1000 + 5 * 60_000)).toBe(tr("common.updatedMAgo", { min: 5 }));
});

const open = async (onPostOrder = vi.fn(), loggedIn = false) => {
  const view = render(<MarketBrowseView tr={tr} locale="en" catalog={catalog} loggedIn={loggedIn} onPostOrder={onPostOrder} />);
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "serr" } });
  fireEvent.mouseDown(await screen.findByRole("option", { name: /Serration/ }));
  await waitFor(() => expect(view.container.querySelector("[data-browse-table]")).toBeTruthy());
  return view;
};
const plats = (c: HTMLElement) => [...c.querySelectorAll("[data-browse-unit-plat]")].map((e) => Number(e.getAttribute("data-browse-unit-plat")));

test("browsing an item shows in-game sellers cheapest first, and the filters and sides change the table", async () => {
  const { container } = await open();
  const sellers = plats(container);
  expect(sellers.length).toBeGreaterThan(0);
  expect(sellers).toEqual([...sellers].sort((a, b) => a - b));
  fireEvent.click(screen.getAllByText("All", { selector: ".ui-seg button" })[0]);
  expect(plats(container).length).toBeGreaterThan(sellers.length);
  // the mock buyers are all offline: in-game only is empty, "All" lists them highest first
  fireEvent.click(screen.getByText(tr("browse.buyers")));
  const buyers = plats(container);
  expect(buyers).toEqual([...buyers].sort((a, b) => b - a));
  fireEvent.change(screen.getAllByPlaceholderText(tr("common.min"))[0], { target: { value: String(Math.max(...buyers) + 1) } });
  expect(screen.getByText(tr("browse.noOrdersBuy"))).toBeTruthy();
});

test("the max-rank filter keeps only max-rank listings", async () => {
  const { container } = await open();
  fireEvent.click(screen.getAllByText("All", { selector: ".ui-seg button" })[0]);
  const all = plats(container).length;
  fireEvent.click(screen.getByText(tr("browse.maxed")));
  await waitFor(() => expect(plats(container).length).toBeLessThan(all));
  expect([...container.querySelectorAll("tbody tr td:nth-child(4)")].every((c) => c.textContent === "R10")).toBe(true);
});

test("the statistics tab plots the stored history", async () => {
  const { container } = await open();
  fireEvent.click(screen.getByText(tr("browse.tabStatistics")));
  await waitFor(() => expect(container.querySelector("[data-market-stats-points]")?.getAttribute("data-market-stats-points")).toBe("90"));
  fireEvent.click(screen.getByText(tr("browse.days30")));
  await waitFor(() => expect(container.querySelector("[data-market-stats-points]")?.getAttribute("data-market-stats-points")).toBe("30"));
  expect(container.querySelector("svg path")).toBeTruthy();
});

test("posting an order needs a session and hands the browsed item to the order form", async () => {
  const onPost = vi.fn();
  const signedOut = await open(onPost, false);
  fireEvent.click(screen.getByText(tr("common.postWts")));
  expect(screen.getByText(tr("browse.signInRequired"))).toBeTruthy();
  expect(onPost).not.toHaveBeenCalled();
  signedOut.unmount();

  await open(onPost, true);
  fireEvent.click(screen.getByText(tr("common.postWtb")));
  expect(onPost).toHaveBeenCalledWith(expect.objectContaining({ orderType: "buy", rank: null, item: expect.objectContaining({ slug: "serration" }) }));
});

test("a searched item can be saved and reopened from the chips", async () => {
  await open();
  fireEvent.click(screen.getByTitle(tr("browse.saveItem")));
  expect(JSON.parse(localStorage.getItem("wf_market_browse_saved") ?? "[]")).toEqual(["Serration"]);
  fireEvent.click(screen.getByTitle(tr("browse.removeSavedItem")));
  expect(JSON.parse(localStorage.getItem("wf_market_browse_saved") ?? "[]")).toEqual([]);
});

test("the Ayatan preset lists sculptures ranked by endo per plat", async () => {
  const { container } = render(<MarketBrowseView tr={tr} locale="en" catalog={catalog} loggedIn={false} onPostOrder={vi.fn()} />);
  fireEvent.click(screen.getByText(tr("browse.ayatan.preset")));
  await waitFor(() => expect(container.querySelectorAll("[data-ayatan-row]").length).toBeGreaterThan(0));
  const ratios = [...container.querySelectorAll("[data-ayatan-endo-per-plat]")].map((e) => Number(e.getAttribute("data-ayatan-endo-per-plat")));
  expect(ratios).toEqual([...ratios].sort((a, b) => b - a));
  expect(container.querySelector("[data-ayatan-best]")?.getAttribute("data-ayatan-best")).toBe(String(ratios[0]));
});
