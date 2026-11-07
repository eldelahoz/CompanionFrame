// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { getItemDatabase, resetMock, type ItemDatabase } from "../lib/ipc";
import { translator } from "../lib/i18n";
import FoundryView from "./FoundryView";

const tr = translator("en");
const P = "/Lotus/Types/Recipes/WarframeRecipes/RhinoPrime";
let items: ItemDatabase;

beforeEach(async () => {
  resetMock();
  localStorage.clear();
  items = await getItemDatabase();
});
afterEach(cleanup);

const statusOf = (un: string) => document.querySelector(`[data-foundry-card="${un}"]`)?.getAttribute("data-status");

test("shows claimable, building, buildable and missing-parts cards from the fixture inventory", async () => {
  render(<FoundryView items={items} tr={tr} />);
  await waitFor(() => expect(statusOf(`${P}SystemsComponent`)).toBe("claimable"));
  expect(statusOf(`${P}HelmetComponent`)).toBe("building");
  expect(statusOf(`${P}ChassisComponent`)).toBe("buildable");
  expect(statusOf(`${P}Blueprint`)).toBe("missing");
  // claimable first, missing last
  const order = [...document.querySelectorAll("[data-foundry-card]")].map((n) => n.getAttribute("data-status"));
  expect(order).toEqual(["claimable", "building", "buildable", "missing"]);
});

test("the claim filter narrows to buildable cards and an impossible search says so", async () => {
  render(<FoundryView items={items} tr={tr} />);
  await waitFor(() => expect(document.querySelectorAll("[data-foundry-card]").length).toBe(4));
  fireEvent.change(document.querySelector("[data-foundry-state]") as HTMLSelectElement, { target: { value: "buildable" } });
  expect(document.querySelectorAll("[data-foundry-card]").length).toBe(1);
  fireEvent.change(document.querySelector("[data-foundry-state]") as HTMLSelectElement, { target: { value: "all" } });
  fireEvent.change(screen.getByPlaceholderText("Search..."), { target: { value: "zzzz" } });
  expect(screen.getByText("No foundry items match your filters")).toBeTruthy();
});

test("pinning a blueprint totals its missing resources and persists", async () => {
  render(<FoundryView items={items} tr={tr} />);
  await waitFor(() => expect(document.querySelector(`[data-foundry-card="${P}Blueprint"]`)).toBeTruthy());
  fireEvent.click(document.querySelector(`[data-foundry-card="${P}Blueprint"] [data-foundry-pin]`) as HTMLElement);
  expect(screen.getByText("Pinned blueprints")).toBeTruthy();
  expect(screen.getAllByText("Missing").length).toBeGreaterThan(0);
  expect(JSON.parse(localStorage.getItem("foundry.pinnedRecipes") ?? "[]")).toEqual([`${P}Blueprint`]);
});
