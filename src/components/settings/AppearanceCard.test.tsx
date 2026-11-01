// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import type { Settings } from "../../lib/ipc";
import { getThemeSettings, hydrateTheme } from "../../lib/theme/themeStore";
import ConfirmHost from "../shell/ConfirmHost";
import AppearanceCard from "./AppearanceCard";

const tr = translator("en");
const settings = { ui_scale: 1 } as Settings;

beforeEach(() => hydrateTheme(""));
afterEach(cleanup);

it("picks a built-in preset, which applies its tokens live", () => {
  render(<AppearanceCard section="theme" tr={tr} settings={settings} onChange={vi.fn()} />);
  fireEvent.click(screen.getByText(tr("appearance.builtinThemes")));
  fireEvent.click(screen.getByText("Midnight Blue"));
  expect(getThemeSettings().activePreset).toBe("midnight");
  expect(document.documentElement.style.getPropertyValue("--bg-base")).toBe(getThemeSettings().colors.bgBase);
});

it("saves and deletes a custom theme, and restores the defaults", async () => {
  render(
    <>
      <AppearanceCard section="theme" tr={tr} settings={settings} onChange={vi.fn()} />
      <ConfirmHost tr={tr} />
    </>,
  );
  fireEvent.change(screen.getByPlaceholderText(tr("appearance.customThemeName")), { target: { value: "Mine" } });
  fireEvent.click(screen.getByText(tr("appearance.saveCustomTheme")));
  expect(getThemeSettings().customThemes.map((t) => t.label)).toEqual(["Mine"]);
  fireEvent.click(screen.getByText(tr("appearance.deleteCustomTheme")));
  fireEvent.click(await screen.findByText(tr("common.confirm")));
  await waitFor(() => expect(getThemeSettings().customThemes).toEqual([]));
  fireEvent.click(screen.getByText(tr("appearance.restoreAll")));
  expect(getThemeSettings().activePreset).toBe("default");
});

it("commits the app scale on release and the glass blur live", () => {
  const onChange = vi.fn();
  render(<AppearanceCard section="theme" tr={tr} settings={settings} onChange={onChange} />);
  const scale = document.querySelector("[data-app-scale] input[type=range]") as HTMLInputElement;
  fireEvent.change(scale, { target: { value: "1.25" } });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.pointerUp(scale);
  expect(onChange).toHaveBeenCalledWith({ ui_scale: 1.25 });

  fireEvent.click(screen.getByLabelText(tr("common.glassBlur"), { exact: false }));
  expect(getThemeSettings().effects.glass).toBe(true);
});

it("shows the contrast badge only for low-contrast text colors in contrast-safe mode", () => {
  render(<AppearanceCard section="colors" tr={tr} settings={settings} onChange={vi.fn()} />);
  expect(screen.queryAllByTitle(/Low contrast/i)).toHaveLength(0);
});
