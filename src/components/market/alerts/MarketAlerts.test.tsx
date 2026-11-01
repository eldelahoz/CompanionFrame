// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../../lib/i18n";
import { marketAlertsGet, resetMock } from "../../../lib/ipc";
import ConfirmHost from "../../shell/ConfirmHost";
import MarketAlerts from "./MarketAlerts";

const tr = translator("en");
beforeEach(resetMock);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const openEditor = async () => {
  render(
    <>
      <MarketAlerts tr={tr} />
      <ConfirmHost tr={tr} />
    </>,
  );
  fireEvent.click(await screen.findByText("New rule"));
  return await screen.findByLabelText("Rule name");
};

test("starts empty and shows the engine health", async () => {
  render(<MarketAlerts tr={tr} />);
  expect(await screen.findByText(/No alert rules yet/)).toBeTruthy();
  expect(await screen.findByText(/0\/0 rules active/)).toBeTruthy();
});

test("the editor refuses an empty name, then an item rule without item or price", async () => {
  await openEditor();
  fireEvent.click(screen.getByText("Save"));
  expect(await screen.findByText("Name the rule")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Rule name"), { target: { value: "Cheap serration" } });
  fireEvent.click(screen.getByText("Save"));
  expect(await screen.findByText("Choose an item")).toBeTruthy();
  expect(await marketAlertsGet()).toHaveLength(0);
});

test("creates a riven rule, toggles it and deletes it", async () => {
  const name = await openEditor();
  fireEvent.change(name, { target: { value: "Rubico crit" } });
  fireEvent.click(screen.getByText("Rivens"));
  fireEvent.change(screen.getByLabelText("Choose a weapon"), { target: { value: "rubico" } });
  fireEvent.click(screen.getAllByText("Critical Chance")[0]);
  fireEvent.click(screen.getByText("Save"));

  const row = await waitFor(() => {
    const r = document.querySelector("[data-alert-rule]");
    expect(r).toBeTruthy();
    return r as HTMLElement;
  });
  expect(row.textContent).toContain("Rubico crit");
  const [saved] = await marketAlertsGet();
  expect(saved.rule.riven).toMatchObject({ weaponUrlName: "rubico", requirePositive: ["critical_chance"] });

  fireEvent.click(screen.getByLabelText("Enabled"));
  await waitFor(async () => expect((await marketAlertsGet())[0].rule.enabled).toBe(false));

  fireEvent.click(screen.getByLabelText("Delete"));
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(async () => expect(await marketAlertsGet()).toHaveLength(0));
});

test("imports an exported file only after confirming, and the rules arrive disabled", async () => {
  render(
    <>
      <MarketAlerts tr={tr} />
      <ConfirmHost tr={tr} />
    </>,
  );
  const rule = { name: "Shared", kind: "item", item: { urlName: "serration", maxPrice: 10 } };
  const file = new File([JSON.stringify([rule])], "alerts.json", { type: "application/json" });
  fireEvent.change(document.querySelector("[data-alert-import-file]")!, { target: { files: [file] } });
  fireEvent.click(await screen.findByText("Confirm"));
  expect(await screen.findByText(/Imported 1 rule/)).toBeTruthy();
  const [added] = await marketAlertsGet();
  expect(added.rule).toMatchObject({ name: "Shared", enabled: false });
});
