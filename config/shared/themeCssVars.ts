import { OVERLAY_LAYOUT_KINDS, type OverlayLayoutKind } from "./overlayLayout";

// Overlay IPC accepts only these theme variables. Keep them aligned with THEME_*_CSS_MAP.
export function overlayOpacityCssVar(kind: OverlayLayoutKind): string {
  return `--overlay-opacity-${kind}`;
}

export const OVERLAY_OPACITY_CSS_VARS = OVERLAY_LAYOUT_KINDS.map(overlayOpacityCssVar);

export const OVERLAY_FORWARDED_EFFECT_VARS = [
  "--radius-sm",
  "--radius-md",
  "--radius-lg",
  "--radius-xl",
  "--ui-panel-bg",
  "--ui-panel-border",
  "--ui-panel-shadow",
  "--ui-control-bg",
  "--ui-control-border",
  "--ui-backdrop-blur",
  "--overlay-opacity",
  ...OVERLAY_OPACITY_CSS_VARS,
] as const;
