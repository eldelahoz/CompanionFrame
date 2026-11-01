import { clampNumber } from "./numeric";
import { normalizeForSlug, sanitizeWfmSlug } from "./textNormalize";

/** Warframe.market user presence status. */
export type WfmStatus = "online" | "ingame" | "invisible";

/** Selectable "keep my status for" durations; 0 means the status never expires. */
export const WFM_STATUS_HOLD_MINUTES: readonly number[] = Object.freeze([0, 30, 60, 120, 240]);

/** Snap an arbitrary value onto the offered hold durations. */
export function normalizeWfmHoldMinutes(value: unknown, fallback = 0): number {
  const minutes = Math.round(Number(value));
  if (!Number.isFinite(minutes)) return fallback;
  return WFM_STATUS_HOLD_MINUTES.includes(minutes) ? minutes : fallback;
}

/** Minutes without keyboard or mouse input before the away rule hides the status. */
const WFM_AWAY_IDLE_MINUTES_DEFAULT = 10;

/** Snap the away idle delay onto the 1-60 minute range the control offers. */
export function normalizeWfmAwayIdleMinutes(
  value: unknown,
  fallback: number = WFM_AWAY_IDLE_MINUTES_DEFAULT,
): number {
  return Math.round(clampNumber(value, 1, 60, fallback));
}

/** Base URL for warframe.market static assets (icons, thumbnails). */
const WFM_ASSET_BASE = "https://warframe.market/static/assets/";

/** Normalize a WFM asset path to an absolute URL. Same rule as `asset_url` in services/wfm/types.rs. */
export function formatWfmAssetUrl(path: unknown): string | null {
  if (typeof path !== "string" || !path.trim()) return null;
  const trimmed = path.trim();
  return /^https?:\/\//i.test(trimmed) ? trimmed : `${WFM_ASSET_BASE}${trimmed}`;
}

export function titleFromSlug(slug: string): string {
  return String(slug)
    .replace(/_/g, " ")
    .replace(/\b[a-z]/g, (letter) => letter.toUpperCase());
}

export { isWfmSlug, sanitizeWfmSlug } from "./textNormalize";

// A slug warframe.market minted survives verbatim; only a display name gets its
// punctuation folded into underscores. Zid-an Asheir is slugged with hyphens,
// so folding one would point every lookup at an item that does not exist.
export function normalizeWfmSlug(value: string | null | undefined): string | null {
  return sanitizeWfmSlug(value) ?? normalizeForSlug(value);
}

export function normalizeWfmSlugKey(value: unknown): string {
  return normalizeForSlug(typeof value === "string" ? value : null) ?? "";
}
