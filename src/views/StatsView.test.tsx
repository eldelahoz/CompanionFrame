// @vitest-environment jsdom
import { StrictMode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../lib/i18n";
import ConfirmHost from "../components/shell/ConfirmHost";
import StatsView from "./StatsView";

// Non-Tauri ipc serves the 36-row mock ledger of lib/ipc: sales except every 4th row, partners Tenno_A..C.
const tr = translator("en");
const renderView = () =>
  render(
    <StrictMode>
      <StatsView tr={tr} locale="en" />
      <ConfirmHost tr={tr} />
    </StrictMode>,
  );
const rows = () => document.querySelectorAll("[data-trade-row]");
const filter = (f: string) => fireEvent.click(document.querySelector(`[data-trade-filter=${f}]`) as HTMLElement);

beforeEach(() => localStorage.clear());
afterEach(cleanup);

test("lists the ledger trades next to the daily stats", async () => {
  renderView();
  await waitFor(() => expect(rows()).toHaveLength(36));
  expect(document.querySelector("[data-daily-stats]")).toBeTruthy();
  expect(document.querySelector("[data-trade-filter=sale]")?.textContent).toContain("27");
});

test("filters by type and by partner or item text", async () => {
  renderView();
  await waitFor(() => expect(rows()).toHaveLength(36));
  filter("purchase");
  expect(rows()).toHaveLength(9);
  filter("all");
  fireEvent.change(screen.getByPlaceholderText(tr("stats.tradeSearchPlaceholder")), { target: { value: "forma" } });
  expect(rows()).toHaveLength(12);
  fireEvent.change(screen.getByPlaceholderText(tr("stats.tradeSearchPlaceholder")), { target: { value: "nobody" } });
  expect(rows()).toHaveLength(0);
  expect(screen.getByText(tr("stats.noMatchingTrades"))).toBeTruthy();
});

test("the personal tab shows the profile, its tables and the saved loadouts, and remembers the choice", async () => {
  renderView();
  await waitFor(() => expect(rows()).toHaveLength(36));
  fireEvent.click(document.querySelector("[data-tour-tab=personal]") as HTMLElement);
  await waitFor(() => expect(document.querySelector("[data-profile-name]")?.textContent).toBe("Tenno"));
  expect(localStorage.getItem("stats-tab")).toBe("personal");
  expect(document.querySelector("[data-profile-career=MissionsCompleted] dd")?.textContent).toBe("5,400");
  fireEvent.click(screen.getByRole("tab", { name: tr("profile.enemies") }));
  await waitFor(() => expect(document.querySelectorAll("[data-profile-table=enemies] tr")).toHaveLength(1));
  fireEvent.click(screen.getByRole("tab", { name: tr("profile.loadouts") }));
  await waitFor(() => expect(document.querySelector("[data-profile-loadout]")).toBeTruthy());
  expect(document.querySelector("[data-profile-appearance-item=Suits]")).toBeTruthy();
  expect(document.querySelector("[data-profile-mod-slot]")).toBeTruthy();
});

test("a file that is not JSON reports the error and changes nothing", async () => {
  renderView();
  await waitFor(() => expect(rows()).toHaveLength(36));
  fireEvent.change(document.querySelector("[data-stats-import-aleca]") as HTMLInputElement, { target: { files: [new File(["{nope"], "x.json")] } });
  await waitFor(() => expect(screen.getByText(tr("stats.invalidJsonFile"))).toBeTruthy());
  expect(rows()).toHaveLength(36);
});

// Daily stats: the non-Tauri ipc serves three mock days, today last.
const dailyRows = () => document.querySelectorAll("[data-daily-stats] tbody tr");

test("shows today's deltas and the recent days for the default resources", async () => {
  renderView();
  await waitFor(() => expect(dailyRows()).toHaveLength(3));
  expect(document.querySelector("[data-summary-item=plat]")?.textContent).toContain("+90");
  expect(document.querySelector("[data-summary-item=forma]")).toBeNull();
});

test("the resource picker adds a resource to the summary", async () => {
  renderView();
  await waitFor(() => expect(dailyRows()).toHaveLength(3));
  fireEvent.click(screen.getByText(tr("stats.chartResources")));
  fireEvent.click(document.querySelector("[data-stat-resource=forma]") as HTMLElement);
  fireEvent.click(screen.getByText(tr("common.close")));
  expect(document.querySelector("[data-summary-item=forma]")).toBeTruthy();
});

test("reset asks first and then empties the daily stats", async () => {
  renderView();
  await waitFor(() => expect(dailyRows()).toHaveLength(3));
  fireEvent.click(screen.getByText(tr("common.reset")));
  fireEvent.click(await screen.findByText(tr("common.cancel")));
  expect(dailyRows()).toHaveLength(3);
  fireEvent.click(screen.getByText(tr("common.reset")));
  fireEvent.click(await screen.findByText(tr("common.confirm")));
  await waitFor(() => expect(screen.getByText(tr("stats.noData"))).toBeTruthy());
});

// Last: the mock ledger is module state, so this import leaves a 37th row behind.
test("an AlecaFrame file lands in the ledger once, even when imported twice", async () => {
  renderView();
  await waitFor(() => expect(rows()).toHaveLength(36));
  const file = () => new File([JSON.stringify({ trades: [{ ts: "2026-03-01T00:00:00Z", type: 0, totalPlat: 99, user: "Zed" }] })], "stats.json");
  const input = document.querySelector("[data-stats-import-aleca]") as HTMLInputElement;
  fireEvent.change(input, { target: { files: [file()] } });
  await waitFor(() => expect(rows()).toHaveLength(37));
  expect(screen.getByText(tr("stats.importedTradeSingular", { count: 1 }))).toBeTruthy();
  fireEvent.change(input, { target: { files: [file()] } });
  await waitFor(() => expect(screen.getAllByText(tr("stats.importedTradeSingular", { count: 1 })).length).toBe(1));
  expect(rows()).toHaveLength(37);
});
