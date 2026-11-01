/** Pure chart-data computation - no Svelte, i18n, or IPC. */
import { localDayKey } from "../../../config/shared/dayKey.js";
import type { DailyStatEntry } from "../../types/stats.js";

type SessionStatKey =
  | "platDelta"
  | "creditsDelta"
  | "endoDelta"
  | "ducatsDelta"
  | "ayaDelta"
  | "vitusDelta";
export type ChartKey = SessionStatKey | "relicsOpened" | "dailyTrades";

interface BarData {
  x: number;
  y: number;
  h: number;
  value: number;
  date: string;
  positive: boolean;
}

interface YTick {
  label: string;
  value: number;
  /** Fraction 0 = top of SVG, 1 = bottom */
  yFrac: number;
}

export interface ChartResult {
  bars: BarData[];
  zeroY: number;
  bw: number;
  absLine: Array<{ x: number; y: number; idx: number; recorded: boolean }> | null;
  absValues: number[];
  hasAbsData: boolean;
  yTicks: YTick[];
  /** Upper bound of the shared balance/change axis. */
  niceMax: number;
}

const BAR_H = 64;
const BAR_H_EXPAND = 300;
const BAR_GAP = 2;
const SVG_W = 800;

export const TIMEFRAME_OPTIONS = [7, 14, 30, 90, 180, 365] as const;

/** Map chart keys to the stored absolute value field on DailyStatEntry. */
const ABS_FIELD_MAP: Partial<Record<ChartKey, keyof DailyStatEntry>> = {
  platDelta: "absPlat",
  creditsDelta: "absCredits",
  endoDelta: "absEndo",
  ducatsDelta: "absDucats",
  ayaDelta: "absAya",
  vitusDelta: "absVitus",
};

type ValueFormatter = (abs: number, locale: string) => string;

function fixed(value: number, digits: number, locale: string): string {
  return value.toLocaleString(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  });
}

function fmtPlat(abs: number, locale: string): string {
  return abs.toLocaleString(locale);
}

function fmtCredits(abs: number, locale: string): string {
  if (abs >= 1_000_000) return `${fixed(abs / 1_000_000, 2, locale)}M`;
  if (abs >= 1_000) return `${fixed(abs / 1_000, 1, locale)}k`;
  return abs.toLocaleString(locale);
}

function fmtEndo(abs: number, locale: string): string {
  if (abs >= 1_000) return `${fixed(abs / 1_000, 1, locale)}k`;
  return abs.toLocaleString(locale);
}

function fmtCount(abs: number, locale: string): string {
  return abs.toLocaleString(locale);
}

export const formatters: Record<ChartKey, ValueFormatter> = {
  platDelta: fmtPlat,
  ducatsDelta: fmtPlat,
  ayaDelta: fmtCount,
  creditsDelta: fmtCredits,
  endoDelta: fmtEndo,
  vitusDelta: fmtCount,
  relicsOpened: fmtCount,
  dailyTrades: fmtCount,
};

export function formatAbsolute(n: number | null, locale: string): string {
  if (n === null) return "-";
  const abs = Math.abs(n);
  if (abs >= 1_000_000) return `${fixed(n / 1_000_000, 2, locale)}M`;
  if (abs >= 100_000) return `${fixed(n / 1_000, 1, locale)}k`;
  return n.toLocaleString(locale);
}

export function shortDate(iso: string, locale: string): string {
  const date = new Date(`${iso}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleDateString(locale, { day: "numeric", month: "numeric", timeZone: "UTC" });
}

/** Compact SI tick label: 1.2M / 3.4K / raw. */
function fmtTickSI(value: number, locale: string): string {
  const abs = Math.abs(value);
  if (abs >= 1_000_000) return `${fixed(value / 1_000_000, 1, locale)}M`;
  if (abs >= 1_000) return `${fixed(value / 1_000, 1, locale)}K`;
  return value.toLocaleString(locale);
}

/** Round up to a "nice" number for axis scaling (1, 2, 5 multiples of powers of 10). */
function niceRoundUp(val: number): number {
  if (val <= 0) return 1;
  const exp = Math.floor(Math.log10(val));
  const base = Math.pow(10, exp);
  const frac = val / base;
  if (frac <= 1) return base;
  if (frac <= 2) return 2 * base;
  if (frac <= 5) return 5 * base;
  return 10 * base;
}

function computeNiceTicks(
  minVal: number,
  maxVal: number,
  locale: string,
  targetCount: number,
): { ticks: YTick[]; niceMin: number; niceMax: number } {
  const span = maxVal - minVal;
  const step = span <= targetCount ? 1 : niceRoundUp(span / targetCount);
  const niceMin = Math.floor(minVal / step) * step;
  const niceMax = Math.max(niceMin + step, Math.ceil(maxVal / step) * step);
  const ticks: YTick[] = [];
  for (let value = niceMin; value <= niceMax; value += step) {
    const yFrac = 0.02 + ((niceMax - value) / (niceMax - niceMin)) * 0.96;
    ticks.push({ label: fmtTickSI(value, locale), value, yFrac });
  }
  return { ticks, niceMin, niceMax };
}

/** Typed accessor for chart-keyed numeric fields on DailyStatEntry. */
function pickNumericField(entry: DailyStatEntry, key: ChartKey): number {
  switch (key) {
    case "platDelta":
      return entry.platDelta;
    case "creditsDelta":
      return entry.creditsDelta;
    case "endoDelta":
      return entry.endoDelta;
    case "ducatsDelta":
      return entry.ducatsDelta;
    case "ayaDelta":
      return entry.ayaDelta;
    case "vitusDelta":
      return entry.vitusDelta ?? 0;
    case "relicsOpened":
      return entry.relicsOpened;
    case "dailyTrades":
      return entry.dailyTrades;
  }
}

/** Generate all YYYY-MM-DD strings from startDate to today (inclusive). */
function allCalendarDays(startIso: string): string[] {
  const result: string[] = [];
  const d = new Date(startIso + "T00:00:00");
  const today = new Date();
  today.setHours(23, 59, 59, 999);
  while (d <= today) {
    result.push(localDayKey(d));
    d.setDate(d.getDate() + 1);
  }
  return result;
}

export function barsForKey(
  key: ChartKey,
  hist: DailyStatEntry[],
  days: number,
  barH: number = BAR_H,
  locale: string = "en",
  visibility: { showValue?: boolean; showChange?: boolean } = {},
): ChartResult {
  const cutoff = new Date();
  cutoff.setDate(cutoff.getDate() - days);
  const cutoffStr = localDayKey(cutoff);
  const calendarDays = allCalendarDays(cutoffStr);
  if (calendarDays.length === 0)
    return {
      bars: [],
      zeroY: barH,
      bw: 4,
      absLine: null,
      absValues: [],
      hasAbsData: false,
      yTicks: [],
      niceMax: 0,
    };

  // Index entries by date; newest pre-window entry seeds the carry-in.
  const entryMap = new Map<string, DailyStatEntry>();
  let carryIn: DailyStatEntry | null = null;
  for (const e of hist) {
    if (e.date >= cutoffStr) entryMap.set(e.date, e);
    else if (!carryIn || e.date > carryIn.date) carryIn = e;
  }

  let values: number[] = [];
  const absField = ABS_FIELD_MAP[key];
  const rawAbs: (number | undefined)[] = [];

  for (const day of calendarDays) {
    const entry = entryMap.get(day);
    values.push(entry ? (pickNumericField(entry, key) ?? 0) : 0);
    rawAbs.push(
      absField && entry ? ((entry[absField] as number | undefined) ?? undefined) : undefined,
    );
  }

  // Derive deltas from consecutive abs values when the recorded delta is 0
  if (absField) {
    for (let i = 1; i < values.length; i++) {
      if (values[i] === 0 && rawAbs[i] !== undefined && rawAbs[i - 1] !== undefined) {
        const derived = (rawAbs[i] as number) - (rawAbs[i - 1] as number);
        if (derived !== 0) values[i] = derived;
      }
    }
  }
  const n = calendarDays.length;
  // Dense timeframes must fit their bars and gaps inside the canvas.
  const gap = n > 120 ? 0 : BAR_GAP;
  const bw = Math.max(2, (SVG_W - gap * (n - 1)) / n);
  const xAt = (index: number): number => index * (bw + gap);

  const absSamples = rawAbs.map((value) => value !== undefined);
  if (absField) {
    let lastKnown = carryIn ? (carryIn[absField] as number | undefined) : undefined;
    for (let i = 0; i < rawAbs.length; i++) {
      if (rawAbs[i] !== undefined) lastKnown = rawAbs[i];
      else if (lastKnown !== undefined) rawAbs[i] = lastKnown;
    }
  }
  const validAbs = rawAbs.filter((value): value is number => value !== undefined);
  const absValues = rawAbs.map((value) => value ?? NaN);
  const shown = [
    ...(visibility.showChange === false ? [] : values),
    ...(visibility.showValue === false ? [] : validAbs),
  ];
  // Balances and changes use the same units, so both must match the labelled axis.
  const {
    ticks: yTicks,
    niceMin,
    niceMax,
  } = computeNiceTicks(
    Math.min(0, ...shown),
    Math.max(0, ...shown),
    locale,
    barH >= BAR_H_EXPAND ? 8 : 5,
  );
  const yAt = (value: number): number =>
    (0.02 + ((niceMax - value) / (niceMax - niceMin)) * 0.96) * barH;
  const zeroY = yAt(0);
  const bars: BarData[] = calendarDays.map((date, i) => {
    const value = values[i];
    const y = yAt(value);
    return {
      x: xAt(i),
      y: Math.min(y, zeroY),
      h: Math.abs(y - zeroY),
      value,
      date,
      positive: value >= 0,
    };
  });
  const absLine =
    validAbs.length === 0
      ? null
      : rawAbs.flatMap((value, idx) =>
          value === undefined
            ? []
            : [{ x: xAt(idx) + bw / 2, y: yAt(value), idx, recorded: absSamples[idx] }],
        );
  return {
    bars,
    zeroY,
    bw,
    absLine,
    absValues,
    hasAbsData: validAbs.length > 0,
    yTicks,
    niceMax,
  };
}

export function labelStep(days: number): number {
  if (days <= 7) return 1;
  if (days <= 14) return 2;
  if (days <= 30) return 5;
  if (days <= 90) return 10;
  if (days <= 180) return 20;
  return 30;
}

/** Divergent bar geometry for the analysis flow panels. The month and the day
 *  chart draw the same bars off different fields, so only the shape lives here
 *  and each component keeps its own markup, labels and tooltips. */
export const FLOW_SLOT = 10;
export const FLOW_BAR = 6.5;
export const FLOW_HEIGHT = 100;

interface FlowAxis {
  /** viewBox width for `count` slots. */
  width: number;
  /** Y of the zero line; the full height when there is nothing to plot. */
  zeroY: number;
  /** up + down; 0 means no bars. */
  span: number;
}

export function flowAxis(count: number, up: number, down: number): FlowAxis {
  const width = Math.max(1, count) * FLOW_SLOT;
  const span = up + down;
  if (span <= 0) return { width, zeroY: FLOW_HEIGHT, span: 0 };
  return { width, zeroY: (up / span) * FLOW_HEIGHT, span };
}

export function flowBarX(index: number): number {
  return index * FLOW_SLOT + (FLOW_SLOT - FLOW_BAR) / 2;
}

/** A tiny value still needs a visible sliver, so every bar keeps 1 unit. */
export function flowBarHeight(value: number, span: number): number {
  if (span <= 0) return 0;
  return Math.max(1, (Math.abs(value) / span) * FLOW_HEIGHT);
}
