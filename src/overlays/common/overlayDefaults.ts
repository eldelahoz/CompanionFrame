import type { OverlayKind } from "./overlayTheme";

export interface OverlayBox {
  /** Top-left corner and footprint at scale 1, as fractions of the game screen. */
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * The one place that says where each overlay's main panel sits until the user places it: the live
 * overlays (OverlayShell sets it as --ov-x/y/w/h), Setup's drag stand-ins and the guide images all
 * read it. Footprints are the overlay.css sizes on 16:9: 1em = --ov-font = 1.48% of the height,
 * i.e. 0.8325% of the width.
 */
export const OVERLAY_DEFAULTS: Record<OverlayKind, OverlayBox> = {
  // The 980x236 px card canvas (at 1080p), centred over the reward card row and ending just above it
  // (the cards start at y 0.225 in reward_scan/image.rs), so it never covers what it scans.
  reward: { x: 0.245, y: 0, w: 0.51, h: 0.22 },
  // 640px panel at 1080p (the compare layout; the single card sits at its right edge), 24px from the right edge and
  // under the game's top currency bar (y 45-90 at 1080p).
  riven: { x: 0.6542, y: 0.1, w: 0.3333, h: 0.43 },
  // 15em panel (13em min + padding) 1em from the top-right corner.
  arbi: { x: 0.867, y: 0.015, w: 0.125, h: 0.15 },
  // 22em toast whose bottom edge stays 1.5em above the screen edge; it grows upwards with more matches.
  trade: { x: 0.8085, y: 0.858, w: 0.183, h: 0.12 },
};

export const PLACEMENT_SCALE_MIN = 0.75;
export const PLACEMENT_SCALE_MAX = 1.5;
