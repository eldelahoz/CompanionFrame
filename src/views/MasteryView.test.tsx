// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { translator } from "../lib/i18n";
import MasteryView from "./MasteryView";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const tr = translator("en");
const shown = () => [...document.querySelectorAll<HTMLElement>("[data-mastery-card]")];

test("lists every item and filters by category, status and search", async () => {
  render(<MasteryView tr={tr} locale="en" />);
  await waitFor(() => expect(shown().length).toBeGreaterThan(10));
  const total = shown().length;

  fireEvent.click(screen.getByRole("tab", { name: /^Melee/ }));
  expect(shown().length).toBeGreaterThan(0);
  expect(shown().length).toBeLessThan(total);
  expect(shown().every((c) => c.textContent?.includes("Melee"))).toBe(true);

  fireEvent.click(screen.getByRole("button", { name: tr("common.missing") }));
  expect(shown().every((c) => c.dataset.status === "missing")).toBe(true);

  fireEvent.change(screen.getByRole("searchbox"), { target: { value: "zzzz" } });
  expect(screen.getByText(tr("mastery.noItemsMatch"))).toBeTruthy();
});

test("shows the account rank line and the mastered percentage", async () => {
  render(<MasteryView tr={tr} locale="en" />);
  await waitFor(() => expect(document.querySelector("[data-summary-item=mr]")).toBeTruthy());
  expect(document.querySelector("[data-summary-item=mr]")?.textContent).toContain("MR 24");
  expect(document.querySelector("[data-mastery-summary]")?.textContent).toMatch(/\d+\.\d%/);
});

test("breakdown stays collapsed until toggled and remembers the choice", async () => {
  render(<MasteryView tr={tr} locale="en" />);
  await waitFor(() => expect(shown().length).toBeGreaterThan(0));
  expect(document.querySelector("[data-mastery-breakdown-row]")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: new RegExp(tr("mastery.detailedBreakdown")) }));
  expect(document.querySelectorAll("[data-mastery-breakdown-row]").length).toBeGreaterThan(5);
  expect(localStorage.getItem("mastery-breakdown-expanded")).toBe("true");
});

test("roadmap tab lists recommendations and a pin shows up on the planned tab", async () => {
  render(<MasteryView tr={tr} locale="en" />);
  await waitFor(() => expect(shown().length).toBeGreaterThan(0));
  fireEvent.click(screen.getByRole("tab", { name: new RegExp(tr("mastery.viewRoadmap")) }));
  await waitFor(() => expect(document.querySelectorAll("[data-roadmap-row]").length).toBeGreaterThan(0));
  fireEvent.click(document.querySelector("[data-roadmap-row] [data-mastery-pin]") as HTMLElement);
  expect(JSON.parse(localStorage.getItem("mastery.pinnedItems") ?? "[]")).toHaveLength(1);
  fireEvent.click(screen.getByRole("tab", { name: new RegExp(tr("mastery.viewPlanned")) }));
  await waitFor(() => expect(document.querySelectorAll("[data-planner-row]").length).toBe(1));
  fireEvent.click(document.querySelector("[data-planner-row] [data-mastery-pin]") as HTMLElement);
  expect(screen.getByText(tr("mastery.planner.empty"))).toBeTruthy();
});
