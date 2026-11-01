import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { THEME_PRESETS } from "../../config/themePresets";
import { bindThemePersistence, getThemeSettings, hydrateTheme, themeSettings } from "./themeStore";
import { normalizeThemeSettings } from "./themeStorage";

describe("theme store", () => {
  const saved: string[] = [];
  let unbind: () => void;
  beforeEach(() => {
    vi.useFakeTimers();
    saved.length = 0;
    hydrateTheme("");
    unbind = bindThemePersistence((json) => saved.push(json));
  });
  afterEach(() => {
    unbind();
    vi.useRealTimers();
  });

  it("hydrates without writing back, and survives a corrupt blob", () => {
    hydrateTheme("{not json");
    expect(getThemeSettings().activePreset).toBe("default");
    vi.advanceTimersByTime(500);
    expect(saved).toEqual([]);
  });

  it("debounces edits into one persisted blob that round-trips", () => {
    themeSettings.setGlobalScale(1.2);
    themeSettings.setEffects({ glass: true, glassBlurPx: 12 });
    vi.advanceTimersByTime(500);
    expect(saved).toHaveLength(1);
    const back = normalizeThemeSettings(JSON.parse(saved[0]));
    expect(back.fontSizes.globalScale).toBe(1.2);
    expect(back.effects.glassBlurPx).toBe(12);
    expect(back.activePreset).toBe("custom");
  });

  it("applies and restores built-in presets", () => {
    themeSettings.applyPreset("midnight");
    expect(getThemeSettings().colors).toEqual(THEME_PRESETS.midnight.colors);
    themeSettings.resetAll();
    expect(getThemeSettings().activePreset).toBe("default");
  });

  it("keeps edits of a custom theme inside it, and deleting the active one falls back to default", () => {
    themeSettings.saveCustomTheme("  Mine  ");
    const id = getThemeSettings().activePreset;
    expect(getThemeSettings().customThemes[0].label).toBe("Mine");
    themeSettings.setColor("accent", "#123456");
    expect(getThemeSettings().activePreset).toBe(id);
    expect(getThemeSettings().customThemes[0].colors.accent).toBe("#123456");
    themeSettings.deleteCustomTheme(id);
    expect(getThemeSettings().activePreset).toBe("default");
    expect(getThemeSettings().customThemes).toEqual([]);
  });

  it("rejects out-of-range per-view font sizes", () => {
    themeSettings.setViewFontSize("market", "bodySize", 99);
    expect(getThemeSettings().viewOverrides.market).toBeUndefined();
  });
});
