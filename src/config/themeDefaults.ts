import type {
  ThemeBaseColors,
  ThemeBranding,
  ThemeColors,
  ThemeEffects,
  ThemeFontSizes,
  ThemeSettings,
} from "../types/theme";
import { deriveThemeColors } from "../lib/theme/derive";

export const DEFAULT_BASE_COLORS: Readonly<ThemeBaseColors> = Object.freeze({
  bgDeep: "#0e0f11",
  bgBase: "#131417",
  bgSurface: "#18191d",
  bgRaised: "#1f2126",
  bgHover: "#25282e",
  accent: "#3fb8af",
  accentDim: "#2d847e",
  accentBright: "#5ccac1",
  textPrimary: "#e7e8eb",
  textSecondary: "#a3a7b0",
  textMuted: "#80858f",
  success: "#4cc38a",
  warning: "#e2b54a",
  danger: "#ef6f6c",
  info: "#74a9ee",
  border: "rgba(255, 255, 255, 0.075)",
  borderStrong: "rgba(255, 255, 255, 0.13)",
  gradeS: "#4cc38a",
  gradeA: "#7cb98d",
  gradeB: "#e2c04a",
  gradeC: "#ec8f4f",
  gradeD: "#ec8f4f",
  gradeF: "#ef6f6c",
  gradeDefault: "#a3a7b0",
});

export const DEFAULT_COLORS: Readonly<ThemeColors> = Object.freeze({
  ...DEFAULT_BASE_COLORS,
  ...deriveThemeColors(DEFAULT_BASE_COLORS),
});

export const DEFAULT_FONT_SIZES: Readonly<ThemeFontSizes> = Object.freeze({
  globalScale: 1.0,
});

export const GLASS_BLUR_MIN_PX = 2;
export const GLASS_BLUR_MAX_PX = 24;
export const DEFAULT_EFFECTS: Readonly<ThemeEffects> = Object.freeze({
  cornerStyle: "soft",
  surfaceStyle: "full",
  glass: false,
  glassBlurPx: 5,
  overlayOpacity: 1,
  relicCardStyle: "plain",
});

export const DEFAULT_BRANDING: Readonly<ThemeBranding> = Object.freeze({
  logoDataUrl: null,
  appName: null,
});

export const DEFAULT_THEME: Readonly<ThemeSettings> = Object.freeze({
  version: 1 as const,
  activePreset: "default",
  colors: { ...DEFAULT_COLORS },
  fontSizes: { ...DEFAULT_FONT_SIZES },
  effects: { ...DEFAULT_EFFECTS },
  customThemes: [],
  branding: { ...DEFAULT_BRANDING },
  contrastSafeMode: false,
  viewAccents: {},
  viewOverrides: {},
});

export const FONT_SCALE_MIN = 0.75;
export const FONT_SCALE_MAX = 1.5;
export const FONT_SCALE_STEP = 0.05;

/** Limits for a per-view heading/body/small size override, in rem */
export const VIEW_FONT_SIZE_MIN = 0.5;
export const VIEW_FONT_SIZE_MAX = 3;

/** Root font size in px before the theme font scale (tokens.css type scale is in rem). */
export const BASE_FONT_SIZE_PX = 14;
