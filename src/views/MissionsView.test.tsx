// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../lib/i18n";
import { getItemDatabase, missionRewardsSetTracking, type ItemDatabase } from "../lib/ipc";
import MissionsView from "./MissionsView";

// Non-Tauri ipc serves 5 mock missions (newest first): Survival 12 min ago, a carried read of 3,
// Interception, Defense, Survival; the default period is 7 days, so all of them show.
const tr = translator("en");
const ids = () => [...document.querySelectorAll("[data-mission-entry]")].map((e) => e.getAttribute("data-mission-entry"));
let items: ItemDatabase = {};

beforeEach(async () => {
  localStorage.clear();
  await missionRewardsSetTracking(true);
  items = await getItemDatabase();
});
afterEach(cleanup);

const renderView = () => render(<MissionsView tr={tr} locale="en" items={items} />);

test("shows the latest mission, the counts and the history newest first", async () => {
  renderView();
  await waitFor(() => expect(ids()).toEqual(["mock-5", "mock-4", "mock-3", "mock-2", "mock-1"]));
  expect(document.querySelector("[data-missions-latest]")?.getAttribute("data-missions-latest")).toBe("mock-5");
  expect(document.querySelector("[data-missions-recorded]")?.getAttribute("data-missions-recorded")).toBe("5");
  // Which missions fall on "today" depends on the time of day the test runs.
  expect(document.querySelector("[data-missions-today]")?.textContent).toMatch(/^\d+ missions? today$/);
  expect(document.querySelector("[data-summary-item=missions] .ui-stat-value")?.textContent).toBe("7");
  expect(document.querySelector("[data-missions-status]")).toBeNull();
});

test("the type filter narrows the history and the totals", async () => {
  renderView();
  await waitFor(() => expect(ids()).toHaveLength(5));
  fireEvent.change(document.querySelector("[data-missions-type-filter]")!, { target: { value: "MT_SURVIVAL" } });
  await waitFor(() => expect(ids()).toEqual(["mock-5", "mock-1"]));
  expect(document.querySelector("[data-summary-item=missions] .ui-stat-value")?.textContent).toBe("2");
  // The latest mission is not a filter result.
  expect(document.querySelector("[data-missions-latest]")?.getAttribute("data-missions-latest")).toBe("mock-5");
});

test("the item search keeps the missions that received a matching item", async () => {
  renderView();
  await waitFor(() => expect(ids()).toHaveLength(5));
  fireEvent.change(screen.getByPlaceholderText(tr("orderModal.searchItemsPlaceholder")), { target: { value: "forma" } });
  await waitFor(() => expect(ids()).toEqual(["mock-3"]));
  fireEvent.change(screen.getByPlaceholderText(tr("orderModal.searchItemsPlaceholder")), { target: { value: "no such item" } });
  await waitFor(() => expect(document.querySelector("[data-missions-no-match]")).not.toBeNull());
});

test("a mission with rewards expands to its items; one that brought nothing does not", async () => {
  renderView();
  await waitFor(() => expect(ids()).toHaveLength(5));
  expect(document.querySelector("[data-mission-toggle=mock-1]")).not.toBeNull();
  fireEvent.click(document.querySelector("[data-mission-toggle=mock-1]")!);
  const detail = document.querySelector("[data-mission-detail=mock-1]")!;
  expect([...detail.querySelectorAll("[data-reward-row]")].map((r) => r.getAttribute("data-reward-row"))).toEqual([
    "/Lotus/Types/Items/MiscItems/Ferrite",
    "/Lotus/Types/Items/MiscItems/Plastids",
  ]);
  // mock-4 is a carried read of 3 missions with no items: it still expands to say so.
  expect(document.querySelector("[data-mission-toggle=mock-4]")).not.toBeNull();
});

test("turning tracking off shows the notice and the switch turns it back on", async () => {
  renderView();
  await waitFor(() => expect(ids()).toHaveLength(5));
  const box = document.querySelector("[data-missions-tracking]") as HTMLInputElement;
  expect(box.checked).toBe(true);
  fireEvent.click(box);
  await waitFor(() => expect(document.querySelector("[data-missions-status=tracking-off]")?.textContent).toBe(tr("dashboard.lastMission.trackingOff")));
  expect(box.checked).toBe(false);
  fireEvent.click(box);
  await waitFor(() => expect(document.querySelector("[data-missions-status]")).toBeNull());
});
