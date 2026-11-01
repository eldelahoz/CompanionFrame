// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import type { Settings } from "../../lib/ipc";
import GameLanguageRow from "./GameLanguageRow";

vi.mock("../../lib/ipc", () => ({
  gameLanguageDetected: async () => "fr",
  onItemDbUpdated: async () => () => {},
}));

afterEach(cleanup);
const tr = translator("en");

it("offers Auto with the language EE.log reported, and saves an explicit choice", async () => {
  const onChange = vi.fn(async () => null);
  render(<GameLanguageRow id="g" tr={tr} settings={{ game_language: "auto" } as Settings} onChange={onChange} />);
  expect(await screen.findByRole("option", { name: "Auto (detected: Français)" })).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Warframe game language"), { target: { value: "es" } });
  expect(onChange).toHaveBeenCalledWith({ game_language: "es" });
});
