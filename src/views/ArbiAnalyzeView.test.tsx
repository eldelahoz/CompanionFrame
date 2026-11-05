// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import PtMetricCards from "../components/profitTaker/PtMetricCards";
import { translator } from "../lib/i18n";
import { arbiRuns } from "../stores/arbiRuns";
import { ptRuns } from "../stores/ptRuns";
import ArbiAnalyzeView from "./ArbiAnalyzeView";

// Non-Tauri ipc serves the mock runs of lib/arbi/mockRuns: 4 arbitrations, 4 Profit Taker runs (3 eligible).
const tr = translator("en");
const renderView = () => render(<ArbiAnalyzeView tr={tr} language="en" />);

beforeEach(() => {
  localStorage.clear();
  arbiRuns.reset();
  ptRuns.reset();
});
afterEach(cleanup);

const arbiRows = () => document.querySelectorAll("[data-arbi-run-table] tbody tr");

test("lists the stored arbitrations and filters them by type", async () => {
  renderView();
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
  fireEvent.change(screen.getByDisplayValue(tr("arbi.filter.allTypes")), { target: { value: "defense" } });
  expect(arbiRows()).toHaveLength(2);
  expect(screen.getByText(tr("arbi.filter.showing", { shown: "2", total: "4" }))).toBeTruthy();
  fireEvent.click(screen.getByText(tr("arbi.filter.clear")));
  expect(arbiRows()).toHaveLength(4);
});

test("shows a run's tags and its imported badge", async () => {
  renderView();
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
  expect(within(arbiRows()[0] as HTMLElement).getByText("boar run")).toBeTruthy();
  expect(within(arbiRows()[2] as HTMLElement).getByText(tr("common.imported"))).toBeTruthy();
});

test("a run opens into its detail with the KPIs, and previous/next walk the filtered list", async () => {
  renderView();
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
  fireEvent.click(arbiRows()[0]);
  expect(await screen.findByText("Casta (Ceres)")).toBeTruthy();
  expect(document.querySelector('[data-summary-item="drones"]')?.textContent).toContain("82");
  expect(document.querySelector("[data-arbi-clear-map]")).toBeTruthy();
  expect((document.querySelector("[data-arbi-prev]") as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(document.querySelector("[data-arbi-next]")!);
  expect(await screen.findByText("Hydron (Sedna)")).toBeTruthy();
  fireEvent.click(screen.getByText(/All runs/));
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
});

test("a run with no stats says so instead of drawing charts", async () => {
  renderView();
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
  fireEvent.click(arbiRows()[3]);
  expect(await screen.findByText(tr("arbi.noStats"))).toBeTruthy();
  expect(document.querySelector("[data-arbi-clear-map]")).toBeNull();
});

test("two selected runs compare side by side", async () => {
  renderView();
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
  const boxes = screen.getAllByLabelText(tr("arbi.selectRun"));
  fireEvent.click(boxes[0]);
  expect((document.querySelector("[data-arbi-compare]") as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(boxes[1]);
  fireEvent.click(document.querySelector("[data-arbi-compare]")!);
  expect(document.querySelector("[data-arbi-compare-panel]")).toBeTruthy();
  expect(screen.getByText("Casta (Ceres)", { selector: "[data-arbi-compare-panel] span" })).toBeTruthy();
});

test("the Profit Taker tab lists runs, marks the bugged one and opens the analytics", async () => {
  renderView();
  fireEvent.click(screen.getByText(tr("pt.tab")));
  await waitFor(() => expect(document.querySelectorAll("[data-pt-run]")).toHaveLength(4));
  expect(document.querySelectorAll("[data-pt-bugged]")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: tr("common.analytics") }));
  const scope = document.querySelector("[data-pt-analytics-scope]")!;
  expect(scope.getAttribute("data-pt-eligible-count")).toBe("3");
  expect(scope.getAttribute("data-pt-excluded-count")).toBe("1");
  expect(document.querySelectorAll("[data-pt-chart-run]")).toHaveLength(3);
  expect(document.querySelector('[data-pt-mean="total"]')).toBeTruthy();
});

test("a Profit Taker run compares against a baseline of the same squad size", async () => {
  renderView();
  fireEvent.click(screen.getByText(tr("pt.tab")));
  await waitFor(() => expect(document.querySelectorAll("[data-pt-run]")).toHaveLength(4));
  fireEvent.click(document.querySelectorAll("[data-pt-run]")[1]);
  const baseline = (await screen.findByLabelText(new RegExp(tr("pt.baseline")))) as HTMLSelectElement;
  expect(document.querySelectorAll("[data-pt-delta]")).toHaveLength(0);
  fireEvent.change(baseline, { target: { value: baseline.options[1].value } });
  expect(document.querySelectorAll("[data-pt-delta]")).toHaveLength(6);
  expect(document.querySelectorAll("[data-pt-phase]")).toHaveLength(4);
});

test("an excluded Profit Taker run explains why and offers no baseline", async () => {
  renderView();
  fireEvent.click(screen.getByText(tr("pt.tab")));
  await waitFor(() => expect(document.querySelectorAll("[data-pt-run]")).toHaveLength(4));
  fireEvent.click(document.querySelectorAll("[data-pt-run]")[3]);
  expect(await screen.findByText(new RegExp(tr("pt.excluded")))).toBeTruthy();
  expect(document.querySelector("[data-pt-baseline]")).toBeNull();
});

test("metric cards show a delta sign and the unit only for second values", () => {
  render(
    <PtMetricCards
      tr={tr}
      rows={[
        { metric: "total", value: 112.971, delta: { seconds: -1.5, percent: -1.3 } },
        { metric: "flight", value: null },
      ]}
    />,
  );
  expect(document.querySelector('[data-pt-delta="total"]')?.textContent).toContain("-1.500s (-1.3%)");
  expect(document.querySelector('[data-pt-stat-value="flight"]')?.textContent).not.toBe("");
});

test("shift-click on a run checkbox selects the runs in between", async () => {
  renderView();
  await waitFor(() => expect(arbiRows()).toHaveLength(4));
  const boxes = () => screen.getAllByLabelText(tr("arbi.selectRun")) as HTMLInputElement[];
  fireEvent.click(boxes()[0]);
  fireEvent.click(boxes()[2], { shiftKey: true });
  expect(boxes().map((b) => b.checked)).toEqual([true, true, true, false]);
});
