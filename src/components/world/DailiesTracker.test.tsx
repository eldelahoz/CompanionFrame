// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../../lib/i18n";
import { mockWorldState } from "../../lib/world/fixture";
import DailiesTracker from "./DailiesTracker";
import WorldWeekOverview from "./WorldWeekOverview";

const tr = translator("en");
// Same instance on remount: acts key their ticks off the expiry, which mockWorldState derives from now.
const wd = mockWorldState();

beforeEach(() => localStorage.clear());
afterEach(cleanup);

test("lists built-in tasks with live nightwave and alert rows", async () => {
  render(<DailiesTracker wd={wd} tr={tr} />);
  await waitFor(() => expect(document.querySelector("[data-task=archonHunt]")).toBeTruthy());
  expect(document.querySelector("[data-task='nw:nw-d1']")).toBeTruthy();
  expect(document.querySelector("[data-task='alert:mock-alert']")).toBeTruthy();
});

test("ticking a task persists across a remount and the section counter follows", () => {
  const { unmount } = render(<DailiesTracker wd={wd} tr={tr} />);
  const box = () => document.querySelector("[data-task='nw:nw-d1']") as HTMLInputElement;
  expect(box().checked).toBe(false);
  fireEvent.click(box());
  expect(box().checked).toBe(true);
  unmount();
  render(<DailiesTracker wd={wd} tr={tr} />);
  expect(box().checked).toBe(true);
});

test("customize mode adds and removes a custom task", () => {
  render(<DailiesTracker wd={wd} tr={tr} />);
  fireEvent.click(document.querySelector("[data-tracker-edit]") as HTMLElement);
  fireEvent.change(document.querySelector("[data-task-name]") as HTMLElement, { target: { value: "Feed the kubrow" } });
  fireEvent.click(document.querySelector("[data-task-add]") as HTMLElement);
  expect(screen.getByText("Feed the kubrow")).toBeTruthy();
  fireEvent.click(document.querySelector("[data-task-remove]") as HTMLElement);
  expect(screen.queryByText("Feed the kubrow")).toBeNull();
});

test("the week overview shows the highlighted tasks and opens the dailies tab", () => {
  let opened = 0;
  render(<WorldWeekOverview wd={mockWorldState()} nowMs={Date.now()} tr={tr} onOpenDailies={() => opened++} />);
  expect(document.querySelector("[data-world-week-task=archonHunt]")).toBeTruthy();
  fireEvent.click(document.querySelector("[data-world-open-dailies]") as HTMLElement);
  expect(opened).toBe(1);
});
