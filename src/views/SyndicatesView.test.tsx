// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";

const inventory = vi.hoisted(() => ({ data: null as Record<string, unknown> | null }));
vi.mock("../stores/inventory", async (orig) => ({ ...(await orig<typeof import("../stores/inventory")>()), useInventory: () => ({ data: inventory.data, status: null, loaded: true }) }));
const { default: SyndicatesView } = await import("./SyndicatesView");

const tr = translator("en");
afterEach(() => {
  cleanup();
  localStorage.clear();
});

test("without an inventory it asks for one", () => {
  inventory.data = null;
  render(<SyndicatesView tr={tr} locale="en" />);
  expect(screen.getByText(tr("syndicates.noData"))).toBeTruthy();
});

test("picking a target rank lists its steps and the totals panel", () => {
  inventory.data = { PlayerLevel: 10, RegularCredits: 5, Affiliations: [{ Tag: "CetusSyndicate", Title: 0, Standing: 0 }] };
  render(<SyndicatesView tr={tr} locale="en" />);
  const card = document.querySelector('[data-syndicate-card="CetusSyndicate"]') as HTMLElement;
  fireEvent.click(card.querySelector('[data-syndicate-goal="1"]') as HTMLElement);
  expect(card.querySelector('[data-syndicate-step="1"]')).toBeTruthy();
  expect(document.querySelector("[data-syndicate-pool]")).toBeTruthy();
  fireEvent.click(screen.getByText(tr("syndicates.clearGoals")));
  expect(card.querySelector('[data-syndicate-step="1"]')).toBeNull();
});
