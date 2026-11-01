// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { componentState, readyCount, sortByState } from "../lib/componentStatus";
import { translator } from "../lib/i18n";
import type { ComponentInfo } from "../types/inventory";
import ComponentStatusList from "./ComponentStatusList";

afterEach(cleanup);
const tr = translator("en");
const part = (name: string, itemCount: number, built: number, extra: Partial<ComponentInfo> = {}): ComponentInfo => ({ name, uniqueName: `/x/${name}`, itemCount, built, ownedCount: built, ...extra });
const ready = part("Systems", 1, 1);
const bp = part("Chassis", 1, 0, { blueprintHeld: true });
const some = part("Cells", 2, 1);
const none = part("Neuroptics", 1, 0);

test("states: blueprint without a build differs from some of N", () => {
  expect([ready, bp, some, none].map(componentState)).toEqual(["ready", "build", "partial", "missing"]);
  expect(componentState(part("Barrel", 3, 0, { blueprintHeld: true }))).toBe("build");
});

test("pending parts sort first, ties keep their order", () => {
  expect(sortByState([ready, bp, some, none]).map((c) => c.name)).toEqual(["Neuroptics", "Cells", "Chassis", "Systems"]);
  expect(readyCount([ready, bp, some, none])).toBe(1);
});

test("renders summary, labels and opens the part on click", () => {
  const onSelect = vi.fn();
  render(<ComponentStatusList components={[ready, bp, some, none]} selected={null} db={{}} relicDb={null} tr={tr} onSelect={onSelect} />);
  expect(screen.getByText("1 of 4 parts ready")).toBeTruthy();
  const bar = screen.getByRole("progressbar");
  expect(bar.getAttribute("aria-valuenow")).toBe("1");
  expect(bar.getAttribute("aria-valuemax")).toBe("4");
  const rows = screen.getAllByRole("button");
  expect(rows.map((r) => r.getAttribute("data-part-state"))).toEqual(["missing", "partial", "build", "ready"]);
  expect(screen.getByText("1 missing")).toBeTruthy();
  expect(screen.getByText("To build")).toBeTruthy();
  fireEvent.click(screen.getByText("Cells"));
  expect(onSelect).toHaveBeenCalledWith(some);
});
