import { type OverlayLayoutKind } from "../../../config/shared/overlayLayout";
import { isOverlayOpacityPercent } from "../../../config/shared/overlayOpacity";
import { applyTheme } from "../../lib/theme/applyTheme";
import { normalizeThemeSettings } from "../../lib/theme/themeStorage";

export type OverlayKind = "reward" | "riven" | "arbi" | "trade";
export const OVERLAY_KINDS: readonly OverlayKind[] = ["reward", "riven", "arbi", "trade"];

/** The window kinds of the overlay manager and the per-layout opacity kinds are not 1:1. */
const OPACITY_KIND: Record<OverlayKind, OverlayLayoutKind> = {
  reward: "reward",
  riven: "rivenLeft",
  arbi: "arbiSummary",
  trade: "tradeNotification",
};

/** The appearance editor's tokens as a var map (the overlay windows have no theme store of their own). */
export function themeSettingsVars(json: string): Record<string, string> {
  const vars: Record<string, string> = {};
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return vars;
  }
  const collector = { style: { setProperty: (k: string, v: string) => void (vars[k] = v), removeProperty: (k: string) => (delete vars[k], "") } };
  applyTheme(normalizeThemeSettings(parsed), collector);
  // Opacity comes from the per-overlay Settings, not from the theme.
  for (const key of Object.keys(vars)) if (key.startsWith("--overlay-opacity")) delete vars[key];
  return vars;
}

/** Theme tokens, then the Settings opacity of one overlay as the shared opacity var (percent, validated by Rust). */
export function settingsThemeVars(s: { overlay_opacity: Record<string, number>; theme_settings?: string }, kind: OverlayKind): Record<string, string> {
  const vars = s.theme_settings ? themeSettingsVars(s.theme_settings) : {};
  const pct = s.overlay_opacity[kind];
  return pct == null ? vars : { ...vars, "--overlay-opacity": `${pct}%` };
}

export const overlayOpacityCssVar = (kind: OverlayLayoutKind) => `--overlay-opacity-${kind}`;

export function parseOverlayKind(search: string): OverlayKind | null {
  const kind = new URLSearchParams(search).get("kind");
  return OVERLAY_KINDS.find((k) => k === kind) ?? null;
}

/** Per-kind override, then the shared value, then fully opaque; anything out of range is skipped. */
export function resolveOverlayOpacity(vars: Record<string, unknown>, kind: OverlayKind): string {
  for (const key of [overlayOpacityCssVar(OPACITY_KIND[kind]), "--overlay-opacity"]) {
    const v = vars[key];
    if (typeof v === "string" && isOverlayOpacityPercent(v.trim())) return v.trim();
  }
  return "100%";
}

/** Only `--*` string values are applied; the opacity var is always re-derived from the whole map. */
export function applyOverlayThemeVars(
  root: HTMLElement,
  vars: Record<string, unknown>,
  kind: OverlayKind,
): void {
  for (const [key, value] of Object.entries(vars)) {
    if (key.startsWith("--") && typeof value === "string" && value.trim()) {
      root.style.setProperty(key, value.trim());
    }
  }
  root.style.setProperty("--overlay-opacity-current", resolveOverlayOpacity(vars, kind));
}
