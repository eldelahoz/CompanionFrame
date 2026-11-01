// @vitest-environment jsdom
import { StrictMode } from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { RowEditor } from "../components/analysis/ledger";
import { translator } from "../lib/i18n";
import type { TradeEvent } from "../types/stats";
import ConfirmHost from "../components/shell/ConfirmHost";
import MarketAnalysisView from "./MarketAnalysisView";

// Non-Tauri ipc serves the 36-row mock ledger of lib/ipc (a sale/purchase every 9 days, newest first).
const tr = translator("en");
const renderView = () => render(<StrictMode><MarketAnalysisView tr={tr} locale="en" items={{}} /></StrictMode>);
const rows = () => document.querySelectorAll("[data-analysis-table] tbody tr");
const pickAll = () => fireEvent.click(document.querySelector("[data-analysis-range-preset=all] button") as HTMLElement);

const found = (sel: string) => waitFor(() => {
  const el = document.querySelector(sel);
  expect(el).toBeTruthy();
  return el as HTMLElement;
});

beforeEach(() => localStorage.clear());
afterEach(cleanup);

test("renders the summary and the ledger for the whole range", async () => {
  renderView();
  pickAll();
  await waitFor(() => expect(rows()).toHaveLength(36));
  expect(document.querySelector("[data-analysis-summary]")).toBeTruthy();
  expect(screen.getByText(tr("analysis.rowsRange", { first: 1, last: 36, total: 36 }))).toBeTruthy();
});

test("filters the ledger by type and by text", async () => {
  renderView();
  pickAll();
  await waitFor(() => expect(rows()).toHaveLength(36));
  fireEvent.change(screen.getByLabelText(tr("common.type")), { target: { value: "purchase" } });
  await waitFor(() => expect(rows()).toHaveLength(9));
  fireEvent.change(screen.getByLabelText(tr("analysis.searchPlaceholder")), { target: { value: "forma" } });
  await waitFor(() => expect(rows()).toHaveLength(3));
});

test("editing a row writes the patch and reloads the ledger", async () => {
  renderView();
  pickAll();
  await waitFor(() => expect(rows()).toHaveLength(36));
  fireEvent.click(rows()[0].querySelector(`button[aria-label="${tr("market.edit")}"]`) as HTMLElement);
  const plat = document.querySelector("[data-analysis-row-editor] input[type=number]") as HTMLInputElement;
  fireEvent.change(plat, { target: { value: "777" } });
  fireEvent.click(document.querySelector("[data-analysis-row-editor-save]") as HTMLElement);
  await waitFor(() => expect(document.querySelector("[data-analysis-row-editor]")).toBeNull());
  await waitFor(() => expect(rows()[0].textContent).toContain("777"));
});

const event: TradeEvent = { id: "e1", date: "2026-01-01T00:00:00Z", type: "sale", platChange: 50, partner: "Ash", items: [], credits: 100 };

test("row editor sends only changed fields and blocks a negative number", () => {
  const onSave = vi.fn();
  render(<RowEditor event={event} saving={false} error={null} tr={tr} onSave={onSave} onClose={() => {}} />);
  const [plat, credits] = Array.from(document.querySelectorAll<HTMLInputElement>("input[type=number]"));
  fireEvent.change(plat, { target: { value: "60" } });
  fireEvent.click(document.querySelector("[data-analysis-row-editor-save]") as HTMLElement);
  expect(onSave).toHaveBeenCalledWith({ platChange: 60 });
  onSave.mockClear();
  fireEvent.change(credits, { target: { value: "-1" } });
  expect((document.querySelector("[data-analysis-row-editor-save]") as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText(tr("analysis.invalidNumber"))).toBeTruthy();
  fireEvent.click(document.querySelector("[data-analysis-row-editor-save]") as HTMLElement);
  expect(onSave).not.toHaveBeenCalled();
});

test("an applied import offers Undo, which asks first and then reports the removed rows", async () => {
  render(<><MarketAnalysisView tr={tr} locale="en" items={{}} /><ConfirmHost tr={tr} /></>);
  fireEvent.click(await screen.findByText(tr("analysis.import")));
  fireEvent.change(screen.getByLabelText("GDPR export path"), { target: { value: "gdpr.json" } });
  fireEvent.submit(screen.getByLabelText("GDPR export path").closest("form")!);
  fireEvent.click(await found("[data-analysis-import-apply]"));
  fireEvent.click(await found("[data-analysis-undo]"));
  fireEvent.click(await screen.findByText("Confirm"));
  expect(await screen.findByText(/Removed \d+ imported rows/)).toBeTruthy();
  await waitFor(() => expect(document.querySelector("[data-analysis-batches]")).toBeNull());
});

test("imported batches stay listed on reopen, each with its own Undo", async () => {
  const first = render(<MarketAnalysisView tr={tr} locale="en" items={{}} />);
  fireEvent.click(await screen.findByText(tr("analysis.import")));
  fireEvent.change(screen.getByLabelText("GDPR export path"), { target: { value: "gdpr.json" } });
  fireEvent.submit(screen.getByLabelText("GDPR export path").closest("form")!);
  fireEvent.click(await found("[data-analysis-import-apply]"));
  await found("[data-analysis-batch]");
  first.unmount();
  render(<MarketAnalysisView tr={tr} locale="en" items={{}} />);
  expect((await found("[data-analysis-batch]")).textContent).toContain("3 rows");
});
