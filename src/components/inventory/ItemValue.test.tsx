// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import type { InvRow } from "../../lib/inventory/inventoryRows";
import type { PriceInfo } from "../../lib/inventory/itemPrices";
import { AverageValue, DucatValue, PlatinumValue, PriceTrend, RankIndicator } from "./ItemValue";

const tr = translator("en");
const row = (slug: string | null, ducats = 0) => ({ slug, item: { ducats } }) as unknown as InvRow;
const info: PriceInfo = { state: "ok", price: 12, maxPrice: 40, avg: 10, maxAvg: null, avgState: "ok" };
afterEach(cleanup);

test("not tradable says so in the list and stays empty in the grid", () => {
  const { container, rerender } = render(<PlatinumValue row={row(null)} tr={tr} showNotTradable />);
  expect(container.textContent).toBe("Not tradable");
  rerender(<PlatinumValue row={row(null)} tr={tr} />);
  expect(container.textContent).toBe("");
});

test("the weekly median is unlabelled and muted (its label is the column or card row), or offers a Load button that asks for that row", () => {
  const onLoad = vi.fn();
  const { container, rerender } = render(<AverageValue row={row("volt")} info={info} wanted tr={tr} />);
  expect(container.textContent).toBe("10");
  expect(container.querySelector(".inv-median")).not.toBeNull();
  expect(container.querySelector("[title]")?.getAttribute("title")).toContain("warframe.market");
  rerender(<AverageValue row={row("volt")} wanted={false} onLoad={onLoad} tr={tr} />);
  fireEvent.click(screen.getByRole("button", { name: "Load the 7-day median" }));
  expect(onLoad).toHaveBeenCalledWith("volt");
});

test("the 48h price says where it comes from", () => {
  const { container } = render(<PlatinumValue row={row("volt")} info={info} tr={tr} />);
  expect(container.querySelector("[title]")?.getAttribute("title")).toContain("48 hours");
});

test("the trend shows only past 10% and says which way", () => {
  const trend = (price: number | null, avg: number | null, compact = false) => render(<PriceTrend info={{ ...info, price, avg }} tr={tr} compact={compact} />).container;
  expect(trend(12, 10).textContent).toBe("▲ 20%");
  expect(trend(8, 10).textContent).toBe("▼ 20%");
  expect(trend(8, 10).querySelector("[title]")?.getAttribute("title")).toBe("20% below the 7-day median");
  expect(trend(12, 10, true).textContent).toBe("▲");
  expect(trend(10.5, 10).textContent).toBe("");
  expect(trend(null, 10).textContent).toBe("");
  expect(trend(12, null).textContent).toBe("");
  expect(trend(12, 0).textContent).toBe("");
});

test("no ducat value renders nothing, and no value shows no dash", () => {
  const { container } = render(
    <>
      <DucatValue row={row("volt")} />
      <PlatinumValue row={row("volt")} info={{ ...info, price: null, state: "none" }} tr={tr} />
    </>,
  );
  expect(container.textContent).toBe("");
});

const ranked = (rank: number, maxRank = 10) => ({ slug: "adaptation", maxRank, item: { rank, maxRank, inventoryGroup: "mods", ducats: 0 } }) as unknown as InvRow;

test("a ranked item shows its rank, highlighted at max; an unranked one shows nothing", () => {
  const mid = render(<RankIndicator row={ranked(3)} tr={tr} />).container;
  expect(mid.querySelector("[aria-label]")?.getAttribute("aria-label")).toBe("Rank 3/10");
  expect(mid.textContent).toBe("3 of 10");
  expect(mid.querySelectorAll(".inv-rank-dot")).toHaveLength(10);
  expect(mid.querySelectorAll(".inv-rank-dot[data-on]")).toHaveLength(3);
  expect(mid.querySelector("[data-max]")).toBeNull();
  expect(render(<RankIndicator row={ranked(10)} tr={tr} />).container.querySelector("[data-max]")).not.toBeNull();
  expect(render(<RankIndicator row={ranked(0, 0)} tr={tr} />).container.textContent).toBe("");
});

test("max rank is priced at the max-rank price, the other ranks at rank 0 and say so", () => {
  const price = (r: InvRow) => render(<PlatinumValue row={r} info={info} tr={tr} />).container;
  expect(price(ranked(10)).textContent).toBe("40");
  expect(price(ranked(10)).querySelector("[title]")?.getAttribute("title")).toContain("max rank");
  expect(price(ranked(0)).textContent).toBe("12");
  expect(price(ranked(4)).querySelector("[title]")?.getAttribute("title")).toContain("rank 0");
  // No trend against the rank-0 median for a max-rank price.
  expect(render(<PriceTrend info={info} row={ranked(10)} tr={tr} />).container.textContent).toBe("");
});
