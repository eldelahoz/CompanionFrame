// @vitest-environment jsdom
import { expect, it } from "vitest";
import { OVERLAY_OPACITY_MAX_PERCENT, OVERLAY_OPACITY_MIN_PERCENT } from "../../../config/shared/overlayOpacity";
import { cloneDefaultTheme } from "../../lib/theme/themeStorage";
import { applyOverlayThemeVars, OVERLAY_KINDS, overlayOpacityCssVar, parseOverlayKind, resolveOverlayOpacity, settingsThemeVars, themeSettingsVars } from "./overlayTheme";

const current = (vars: Record<string, string>) => {
  const root = document.createElement("div");
  applyOverlayThemeVars(root, vars, "reward");
  return root.style.getPropertyValue("--overlay-opacity-current");
};

it("parses only the four window kinds", () => {
  expect(OVERLAY_KINDS.map((k) => parseOverlayKind(`?kind=${k}`))).toEqual([...OVERLAY_KINDS]);
  expect(parseOverlayKind("?kind=planner")).toBeNull();
  expect(parseOverlayKind("")).toBeNull();
});

it("per-kind override beats the shared value, and re-derives on every update", () => {
  expect(current({ "--overlay-opacity": "90%", [overlayOpacityCssVar("reward")]: "40%" })).toBe("40%");
  expect(current({ "--overlay-opacity": "70%" })).toBe("70%");
  expect(current({ "--bg-deep": "#000000" })).toBe("100%");
});

it("each window kind reads its own layout kind", () => {
  const vars = { [overlayOpacityCssVar("rivenLeft")]: "50%", [overlayOpacityCssVar("arbiSummary")]: "60%", [overlayOpacityCssVar("tradeNotification")]: "70%" };
  expect(OVERLAY_KINDS.map((k) => resolveOverlayOpacity(vars, k))).toEqual(["100%", "50%", "60%", "70%"]);
});

it("rejects out-of-range or injected opacity and falls back to the shared one", () => {
  expect(current({ "--overlay-opacity": "60%", [overlayOpacityCssVar("reward")]: `${OVERLAY_OPACITY_MIN_PERCENT - 1}%` })).toBe("60%");
  expect(current({ [overlayOpacityCssVar("reward")]: "20%;opacity:0" })).toBe("100%");
  expect(current({ "--overlay-opacity": `${OVERLAY_OPACITY_MAX_PERCENT + 1}%` })).toBe("100%");
  expect(current({ "--overlay-opacity": `${OVERLAY_OPACITY_MIN_PERCENT}%` })).toBe("30%");
});

it("ignores non-custom-property keys", () => {
  const root = document.createElement("div");
  applyOverlayThemeVars(root, { color: "red", "--accent": " #3366ff " }, "reward");
  expect(root.style.color).toBe("");
  expect(root.style.getPropertyValue("--accent")).toBe("#3366ff");
});

it("builds the overlay theme vars from the stored theme, and the Settings opacity wins over the theme's", () => {
  const dark = themeSettingsVars("");
  expect(dark).toEqual({});
  const stored = JSON.stringify({ ...cloneDefaultTheme(), contrastSafeMode: false });
  const vars = themeSettingsVars(stored);
  expect(vars["--bg-base"]).toBeTruthy();
  expect(Object.keys(vars).some((k) => k.startsWith("--overlay-opacity"))).toBe(false);
  expect(themeSettingsVars("{broken")).toEqual({});
  const merged = settingsThemeVars({ overlay_opacity: { reward: 55 }, theme_settings: stored }, "reward");
  expect(merged["--overlay-opacity"]).toBe("55%");
  expect(merged["--bg-base"]).toBe(vars["--bg-base"]);
});
