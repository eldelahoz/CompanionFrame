import { useSyncExternalStore } from "react";
import type {
  ThemeBaseColors,
  ThemeColors,
  ThemeEffects,
  ThemeFontSizes,
  ThemeSettings,
  ViewThemeOverride,
} from "../../types/theme";
import type { ViewName } from "../../types/views";
import type { OverlayLayoutKind } from "../../../config/shared/overlayLayout";
import { DEFAULT_FONT_SIZES, VIEW_FONT_SIZE_MAX, VIEW_FONT_SIZE_MIN } from "../../config/themeDefaults";
import { THEME_PRESETS } from "../../config/themePresets";
import {
  asOverrideColor,
  asOverrideFontSize,
  cloneDefaultTheme,
  cloneThemeEffects,
  normalizeOverlayOpacity,
  normalizeThemeSettings,
} from "./themeStorage";
import { applyTheme } from "./applyTheme";

// Port of WFHelper's stores/theme.ts. It is persisted in the Rust settings (`theme_settings`) instead of localStorage,
// so the overlay windows can read it too; `bindThemePersistence` connects the two.
const SAVE_DEBOUNCE_MS = 300;
const CUSTOM_THEME_PREFIX = "custom:";

const isCustomThemeId = (value: string) => value.startsWith(CUSTOM_THEME_PREFIX);
const createCustomThemeId = () => `${CUSTOM_THEME_PREFIX}${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
const sanitizeCustomThemeName = (name: string) => name.trim().slice(0, 40) || "Custom Theme";

function applyMutableThemeEdits(
  settings: ThemeSettings,
  edits: Pick<Partial<ThemeSettings>, "colors" | "fontSizes" | "effects">,
): ThemeSettings {
  const next: ThemeSettings = {
    ...settings,
    ...edits,
    colors: edits.colors ? { ...edits.colors } : { ...settings.colors },
    fontSizes: edits.fontSizes ? { ...edits.fontSizes } : { ...settings.fontSizes },
    effects: cloneThemeEffects(edits.effects ?? settings.effects),
  };

  if (isCustomThemeId(settings.activePreset)) {
    let updatedActiveTheme = false;
    const customThemes = settings.customThemes.map((theme) => {
      if (theme.id !== settings.activePreset) return theme;
      updatedActiveTheme = true;
      return { ...theme, colors: { ...next.colors }, fontSizes: { ...next.fontSizes }, effects: cloneThemeEffects(next.effects) };
    });
    return { ...next, activePreset: updatedActiveTheme ? settings.activePreset : "custom", customThemes };
  }
  return { ...next, activePreset: "custom" };
}

function withViewOverride(
  settings: ThemeSettings,
  view: ViewName,
  edit: (current: ViewThemeOverride) => ViewThemeOverride,
): ThemeSettings {
  const next = edit(settings.viewOverrides[view] ?? {});
  const colors = next.colors && Object.keys(next.colors).length > 0 ? next.colors : null;
  const fontSizes = next.fontSizes && Object.keys(next.fontSizes).length > 0 ? next.fontSizes : null;

  const viewOverrides = { ...settings.viewOverrides };
  if (colors || fontSizes) {
    const entry: ViewThemeOverride = {};
    if (colors) entry.colors = colors;
    if (fontSizes) entry.fontSizes = fontSizes;
    viewOverrides[view] = entry;
  } else {
    delete viewOverrides[view];
  }
  return { ...settings, viewOverrides };
}

let state: ThemeSettings = cloneDefaultTheme();
const listeners = new Set<() => void>();
let persist: ((json: string) => void) | null = null;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

function set(next: ThemeSettings, save = true): void {
  state = next;
  applyTheme(state);
  listeners.forEach((l) => l());
  if (!save || !persist) return;
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => persist?.(JSON.stringify(state)), SAVE_DEBOUNCE_MS);
}
const update = (fn: (s: ThemeSettings) => ThemeSettings) => set(fn(state));

export const getThemeSettings = () => state;
const subscribeTheme = (l: () => void) => (listeners.add(l), () => void listeners.delete(l));
export const useThemeSettings = () => useSyncExternalStore(subscribeTheme, getThemeSettings);

/** Load the persisted JSON (empty = defaults) without writing it back. */
export function hydrateTheme(json: string): void {
  let parsed: unknown = null;
  try {
    parsed = json ? JSON.parse(json) : null;
  } catch {
    // a corrupt blob falls back to the defaults
  }
  set(parsed ? normalizeThemeSettings(parsed) : cloneDefaultTheme(), false);
}

export function bindThemePersistence(save: (json: string) => void): () => void {
  persist = save;
  return () => {
    persist = null;
    clearTimeout(saveTimer);
  };
}

function setViewColor(view: ViewName, key: keyof ThemeBaseColors, value: string): void {
  const color = asOverrideColor(value);
  if (!color) return;
  update((s) => withViewOverride(s, view, (current) => ({ ...current, colors: { ...current.colors, [key]: color } })));
}

function clearViewColor(view: ViewName, key: keyof ThemeBaseColors): void {
  update((s) => {
    const current = s.viewOverrides[view];
    if (!current?.colors || !(key in current.colors)) return s;
    return withViewOverride(s, view, (entry) => {
      const colors = { ...entry.colors };
      delete colors[key];
      return { ...entry, colors };
    });
  });
}

export const themeSettings = {
  setViewColor,
  clearViewColor,

  /** Override one font size for a single view; null clears it. */
  setViewFontSize(view: ViewName, key: Exclude<keyof ThemeFontSizes, "globalScale">, value: number | null): void {
    // The loader drops out-of-range sizes; accepting one here would apply it until the next launch and then lose it.
    if (value != null && asOverrideFontSize(value, VIEW_FONT_SIZE_MIN, VIEW_FONT_SIZE_MAX) == null) return;
    update((s) =>
      withViewOverride(s, view, (current) => {
        const fontSizes = { ...current.fontSizes };
        if (value == null) delete fontSizes[key];
        else fontSizes[key] = value;
        return { ...current, fontSizes };
      }),
    );
  },

  clearViewOverrides(view: ViewName): void {
    update((s) => {
      if (!(view in s.viewOverrides)) return s;
      const viewOverrides = { ...s.viewOverrides };
      delete viewOverrides[view];
      return { ...s, viewOverrides };
    });
  },

  applyPreset(presetKey: string): void {
    update((s) => {
      const theme = THEME_PRESETS[presetKey] ?? s.customThemes.find((t) => t.id === presetKey);
      if (!theme) return s;
      return { ...s, activePreset: presetKey, colors: { ...theme.colors }, fontSizes: { ...theme.fontSizes }, effects: cloneThemeEffects(theme.effects) };
    });
  },

  setColor(key: keyof ThemeColors, value: string): void {
    update((s) => applyMutableThemeEdits(s, { colors: { ...s.colors, [key]: value } }));
  },

  setGlobalScale(globalScale: number): void {
    update((s) => applyMutableThemeEdits(s, { fontSizes: { ...s.fontSizes, globalScale } }));
  },

  setOptionalFontSize(key: Exclude<keyof ThemeFontSizes, "globalScale">, value: number | undefined): void {
    update((s) => {
      const fontSizes = { ...s.fontSizes };
      if (value != null) fontSizes[key] = value;
      else delete fontSizes[key];
      return applyMutableThemeEdits(s, { fontSizes });
    });
  },

  setEffects(effects: Partial<ThemeEffects>): void {
    update((s) =>
      applyMutableThemeEdits(s, {
        effects: { ...s.effects, ...effects, overlayOpacity: normalizeOverlayOpacity(effects.overlayOpacity ?? s.effects.overlayOpacity) },
      }),
    );
  },

  setOverlayOpacity(kind: OverlayLayoutKind, opacity: number | null): void {
    update((s) => {
      const overlayOpacityOverrides = { ...s.effects.overlayOpacityOverrides };
      if (opacity === null) delete overlayOpacityOverrides[kind];
      else overlayOpacityOverrides[kind] = opacity;
      return applyMutableThemeEdits(s, { effects: { ...s.effects, overlayOpacityOverrides } });
    });
  },

  saveCustomTheme(label: string): void {
    update((s) => {
      const id = createCustomThemeId();
      const custom = { id, label: sanitizeCustomThemeName(label), colors: { ...s.colors }, fontSizes: { ...s.fontSizes }, effects: cloneThemeEffects(s.effects) };
      return { ...s, activePreset: id, customThemes: [...s.customThemes, custom] };
    });
  },

  deleteCustomTheme(themeId: string): void {
    update((s) => {
      if (!isCustomThemeId(themeId)) return s;
      const customThemes = s.customThemes.filter((theme) => theme.id !== themeId);
      if (s.activePreset !== themeId) return { ...s, customThemes };
      const preset = THEME_PRESETS.default;
      return { ...s, activePreset: "default", colors: { ...preset.colors }, fontSizes: { ...preset.fontSizes }, effects: cloneThemeEffects(preset.effects), customThemes };
    });
  },

  setViewAccent: (view: ViewName, value: string) => setViewColor(view, "accent", value),
  clearViewAccent: (view: ViewName) => clearViewColor(view, "accent"),

  setContrastSafeMode(enabled: boolean): void {
    update((s) => ({ ...s, contrastSafeMode: enabled }));
  },

  resetAll(): void {
    set(cloneDefaultTheme());
  },

  resetColors(): void {
    update((s) => applyMutableThemeEdits(s, { colors: { ...(THEME_PRESETS[s.activePreset] || THEME_PRESETS.default).colors } }));
  },

  resetFontSizes(): void {
    update((s) => applyMutableThemeEdits(s, { fontSizes: { ...DEFAULT_FONT_SIZES } }));
  },

  resetColor(key: keyof ThemeColors): void {
    update((s) => {
      const preset = THEME_PRESETS[s.activePreset] ?? THEME_PRESETS.default;
      return applyMutableThemeEdits(s, { colors: { ...s.colors, [key]: preset.colors[key] } });
    });
  },
};

/** First load from the Rust settings. A pre-appearance `theme: "light"` with no stored theme maps to the light preset. */
export function hydrateFromSettings(s: { theme: string; theme_settings: string }): void {
  hydrateTheme(s.theme_settings);
  if (!s.theme_settings && s.theme === "light") themeSettings.applyPreset("light");
}

/** `data-theme` only carries the light token block, which belongs to the "light" preset. */
export const legacyThemeOf = (s: ThemeSettings): "dark" | "light" => (s.activePreset === "light" ? "light" : "dark");

// The element picker (ThemeInspector) is a mode, not a persisted setting.
let inspectorActive = false;
const inspectorListeners = new Set<() => void>();
export const setThemeInspectorActive = (on: boolean) => {
  inspectorActive = on;
  inspectorListeners.forEach((l) => l());
};
export const useThemeInspectorActive = () =>
  useSyncExternalStore((l) => (inspectorListeners.add(l), () => void inspectorListeners.delete(l)), () => inspectorActive);
