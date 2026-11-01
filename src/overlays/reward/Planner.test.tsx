// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { RelicPlannerEvent } from "../../bindings";
import { translator } from "../../lib/i18n";
import * as ipcLib from "../../lib/ipc";
import type { OverlayIpc } from "../common/overlayIpc";
import { Planner } from "./Planner";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const ipc: OverlayIpc = { settings: async () => ({}) as never, on: () => () => {} } as never;
const ev = (over: Partial<RelicPlannerEvent> = {}): RelicPlannerEvent => ({ token: 1, phase: "ready", era: "meso", confidence: 1, ocrUnavailable: false, ...over });
const mount = (e: RelicPlannerEvent, interactive = false) => render(<Planner tr={translator("en")} ipc={ipc} interactive={interactive} event={e} />);

it("lists the owned relics of the detected era, best first", async () => {
  mount(ev());
  expect(await screen.findByText("Meso C2")).toBeTruthy();
  expect(screen.getByText("Meso F3")).toBeTruthy();
  expect(screen.queryByText("Lith A1")).toBeNull();
});

it("without an era every owned relic is listed and the header says so", async () => {
  mount(ev({ era: null }));
  expect(await screen.findByText("Lith A1")).toBeTruthy();
  expect(screen.getByText("Era not detected: every relic")).toBeTruthy();
});

it("says so when there is no OCR, and still lists relics", async () => {
  mount(ev({ era: null, ocrUnavailable: true }));
  expect(screen.getByRole("alert").textContent).toMatch(/no OCR/);
  expect(await screen.findByText("Lith A1")).toBeTruthy();
});

it("shows no empty-state while the era is still being read", async () => {
  mount(ev({ phase: "detecting", era: "requiem" }));
  await screen.findByText("Reading the era…");
  expect(screen.queryByText("No relics found")).toBeNull();
});

it("the squad control only exists while interactive and re-ranks locally", async () => {
  const { rerender } = mount(ev());
  await screen.findByText("Meso C2");
  expect(screen.queryByRole("combobox")).toBeNull();
  rerender(<Planner tr={translator("en")} ipc={ipc} interactive event={ev()} />);
  fireEvent.change(screen.getByRole("combobox"), { target: { value: "1" } });
  expect(screen.getByText("Meso C2")).toBeTruthy();
});

it("ranks every relic of the era when no inventory is loaded", async () => {
  vi.spyOn(ipcLib, "getRawInventory").mockResolvedValue(null);
  mount(ev());
  expect(await screen.findByText("Showing all relics: no inventory loaded")).toBeTruthy();
  expect(await screen.findByText("Meso C2")).toBeTruthy();
  expect(screen.queryByText("Lith A1")).toBeNull();
  expect(screen.queryByText("No relics found")).toBeNull();
});

it("keeps 'No relics found' for an inventory without matching relics", async () => {
  vi.spyOn(ipcLib, "getRawInventory").mockResolvedValue({ MiscItems: [] });
  mount(ev());
  expect(await screen.findByText("No relics found")).toBeTruthy();
});
