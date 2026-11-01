export const DAY_MS = 86_400_000;
export const WEEK_MS = 7 * DAY_MS;

export function parseIsoDate(value: string | null | undefined): Date | null {
  if (!value) return null;
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? null : d;
}

type DurationMode = "countdown" | "strictCountdown" | "remaining" | "buildCompact";

function formatDurationMs(durationMs: number, mode: DurationMode): string {
  if (!Number.isFinite(durationMs) || durationMs <= 0) {
    return mode === "remaining" ? "Ready!" : mode === "buildCompact" ? "" : "Refreshing...";
  }

  const totalSeconds = Math.floor(durationMs / 1000);
  const totalMinutes =
    mode === "buildCompact" ? Math.round(durationMs / 60_000) : Math.floor(durationMs / 60_000);
  const totalHours = Math.floor(durationMs / 3_600_000);
  const days = Math.floor(totalHours / 24);
  const hours = mode === "buildCompact" ? Math.floor(totalMinutes / 60) % 24 : totalHours % 24;
  const minutes =
    mode === "buildCompact" ? totalMinutes % 60 : Math.floor((durationMs % 3_600_000) / 60_000);
  const seconds = totalSeconds % 60;

  if (mode === "buildCompact") {
    if (durationMs < 60_000) return `${Math.max(1, totalSeconds)}s`;
    if (days > 0) {
      if (hours === 0 && minutes === 0) return `${days}d`;
      if (minutes === 0) return `${days}d ${hours}h`;
      if (hours === 0) return `${days}d ${minutes}m`;
      return `${days}d ${hours}h ${minutes}m`;
    }
    if (hours === 0) return `${minutes}m`;
    if (minutes === 0) return `${hours}h`;
    return `${hours}h ${minutes}m`;
  }

  if (mode === "remaining") {
    return totalHours > 24 ? `${days}d ${hours}h` : `${totalHours}h ${minutes}m`;
  }

  if (mode === "strictCountdown") {
    return totalHours > 0 ? `${totalHours}h ${minutes}m ${seconds}s` : `${minutes}m ${seconds}s`;
  }

  return totalHours > 24 ? `${days}d ${hours}h` : `${totalHours}h ${minutes}m ${seconds}s`;
}

export function timeTo(date: Date | null, nowMs: number = Date.now()): string {
  if (!date) return "N/A";
  return formatDurationMs(date.getTime() - nowMs, "countdown");
}

export function timeToStrict(date: Date | null, nowMs: number = Date.now()): string {
  if (!date) return "N/A";
  return formatDurationMs(date.getTime() - nowMs, "strictCountdown");
}

function compactUnit(value: number): string {
  return value.toFixed(1).replace(/\.0$/, "");
}

// One decimal of thousands rounds to the nearest 100: 8% off a four-digit count,
// under 1% from five digits up.
const COMPACT_FROM = 10_000;

let uiLocale: string | undefined;
/** Set once by the app shell, so every number without an explicit locale follows the UI language. */
export const setUiLocale = (locale: string | undefined): void => {
  uiLocale = locale;
};

/** Number in the UI locale with a fixed number of decimals. */
export const formatFixed = (value: number, digits = 0, locale: string | undefined = uiLocale): string =>
  value.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits });

/** Whole-number grouping in the UI locale. */
export const formatInt = (n: number, locale: string | undefined = uiLocale): string => n.toLocaleString(locale);

export function formatNumber(num: number, locale: string | undefined = uiLocale): string {
  // 999,950+ would render as "1000.0K" - bump to the next unit instead.
  if (num >= 999_950) return `${compactUnit(num / 1e6)}M`;
  if (num >= COMPACT_FROM) return `${compactUnit(num / 1e3)}K`;
  return num.toLocaleString(locale);
}

export function formatTimeRemaining(endDate: Date, nowMs: number = Date.now()): string {
  return formatDurationMs(endDate.getTime() - nowMs, "remaining");
}

export function formatBuildTime(seconds: number): string {
  return formatDurationMs(seconds * 1000, "buildCompact");
}

/** True only inside a fully dated window; an unparseable bound reads as closed. */
export function activeWindow(
  activationIso: string | null | undefined,
  expiryIso: string | null | undefined,
  clock: number,
): boolean {
  const activation = parseIsoDate(activationIso ?? null);
  const expiry = parseIsoDate(expiryIso ?? null);
  return !!(activation && expiry && clock >= +activation && clock < +expiry);
}

export function nextDailyResetUtc(now: Date = new Date()): Date {
  return new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1));
}

export function nextWeeklyResetUtc(now: Date = new Date()): Date {
  const day = now.getUTCDay();
  let daysUntilMonday = (8 - day) % 7;
  if (daysUntilMonday === 0) daysUntilMonday = 7;
  return new Date(
    Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + daysUntilMonday),
  );
}

export function cycleTimeDisplay(
  apiTimeLeft: string | null | undefined,
  expiryIso: string | null | undefined,
  nowMs: number = Date.now(),
): string {
  const expiry = parseIsoDate(expiryIso ?? null);
  if (expiry) {
    return timeToStrict(expiry, nowMs);
  }

  const api = (apiTimeLeft ?? "").trim();
  if (api && !/^0h?\s*0m?\s*(0s)?$/i.test(api) && !/^0m?\s*0s?$/i.test(api)) {
    return api;
  }

  return timeTo(expiry, nowMs);
}
