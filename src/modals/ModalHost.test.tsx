// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { dedupeDrops } from "../components/DropsList";
import { getItemDatabase } from "../lib/ipc";
import { translator } from "../lib/i18n";
import { MOCK_RELIC_DB } from "../lib/relic/mockDb";
import { openItemBy, setActiveComponent, setActiveItem } from "../stores/modals";
import ModalHost from "./ModalHost";

const tr = translator("en");
const EXCALIBUR = "/Lotus/Powersuits/Rhino/RhinoPrime";
afterEach(() => {
  cleanup();
  act(() => {
    setActiveItem(null);
    setActiveComponent(null);
  });
});

test("a by-name request opens the item popup, a component opens beside it, Escape closes", async () => {
  const db = await getItemDatabase();
  render(<ModalHost tr={tr} locale="en" db={db} inventory={null} />);
  act(() => openItemBy({ name: "Rhino Prime" }));
  const dialog = await screen.findByRole("dialog", { name: "Rhino Prime" });
  expect(dialog.textContent).toContain("Crafting Tree");
  fireEvent.click(screen.getByText("Chassis"));
  await waitFor(() => expect(dialog.textContent).toContain("0/1 owned"));
  fireEvent.keyDown(dialog, { key: "Escape" });
  await waitFor(() => expect(dialog.textContent).not.toContain("0/1 owned"));
  fireEvent.keyDown(dialog, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("an unknown name opens nothing", async () => {
  render(<ModalHost tr={tr} locale="en" db={await getItemDatabase()} inventory={null} />);
  act(() => openItemBy({ uniqueName: `${EXCALIBUR}Nope` }));
  await act(async () => {});
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("relic drops collapse to one row per relic, base row first", () => {
  const drop = (location: string) => ({ location, type: "x", rarity: "Rare", chance: 2 });
  const rows = dedupeDrops([drop("Lith A1 Relic (Radiant)"), drop("Lith A1 Relic"), drop("Lith B2 Relic (Flawless)"), drop("Orokin Derelict")], MOCK_RELIC_DB);
  expect(rows.map((r) => r.location)).toEqual(["Lith A1 Relic", "Orokin Derelict", "Lith B2"]);
});
