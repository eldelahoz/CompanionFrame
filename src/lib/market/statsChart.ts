// Geometry of the Browse price/volume chart (port of MarketBrowseStats.svelte); pure so it can be tested without a DOM.
import type { MarketStatPoint } from "../../bindings";

/** The backend keeps one daily row per rank; "latest" is everything stored (up to a year). */
export type Period = "30days" | "90days" | "latest";
const PERIOD_DAYS: Record<Period, number> = { "30days": 30, "90days": 90, latest: Infinity };

/** A stored row that can be plotted. */
export type StatRow = MarketStatPoint & { time: number; median: number };

export interface Point {
  x: number;
  y: number;
}

export const MARGIN = { top: 20, right: 14, bottom: 26, left: 52 };
const PRICE_H = 300;
export const PRICE_BASE = MARGIN.top + PRICE_H;
export const VOLUME_TOP = PRICE_BASE + 30;
export const VOLUME_BASE = VOLUME_TOP + 96;
export const CHART_H = VOLUME_BASE + MARGIN.bottom;
const X_LABEL_SPACING = 150;

export interface ChartModel {
  width: number;
  band: string;
  median: string;
  movingAvg: string;
  avgPrice: string;
  candles: Array<{ x: number; width: number; wickTop: number; wickBottom: number; bodyY: number; bodyH: number; rising: boolean }>;
  priceTicks: Array<{ value: number; offset: number; label: string }>;
  volumeTicks: Array<{ value: number; offset: number; label: string }>;
  xLabels: Array<{ x: number; label: string; anchor: "start" | "middle" | "end" }>;
  bars: Array<{ x: number; y: number; width: number; height: number }>;
  markers: Array<{ x: number; medianY: number; movingY: number | null; avgY: number | null }>;
  step: number;
}

const round = (v: number): string => v.toFixed(1);

/** Rows with a time and a median, oldest first, within the period and (when several ranks interleave) of one rank. */
export function selectRows(all: readonly MarketStatPoint[], period: Period, rank: number | null, now = Date.now()): StatRow[] {
  const from = now - PERIOD_DAYS[period] * 86_400_000;
  return all
    .filter((r): r is StatRow => r.time != null && r.median != null && r.time >= from && (rank == null || r.rank === rank))
    .sort((a, b) => a.time - b.time);
}

/** Distinct ranks, ascending; a ranked item stores one row per rank per day. */
export const collectRanks = (all: readonly MarketStatPoint[]): number[] =>
  [...new Set(all.flatMap((r) => (r.rank == null ? [] : [r.rank])))].sort((a, b) => a - b);

/** Clamping Fritsch-Carlson tangents to neighbouring secants hits every sample without inventing price extremes. */
export function monotonePath(points: Point[]): string {
  const n = points.length;
  if (n === 0) return "";
  const head = `M${round(points[0].x)},${round(points[0].y)}`;
  if (n === 1) return head;
  if (n === 2) return `${head}L${round(points[1].x)},${round(points[1].y)}`;
  const dx: number[] = [];
  const slope: number[] = [];
  for (let i = 0; i < n - 1; i += 1) {
    const run = points[i + 1].x - points[i].x;
    dx.push(run);
    slope.push(run === 0 ? 0 : (points[i + 1].y - points[i].y) / run);
  }
  const tangent: number[] = [slope[0]];
  for (let i = 1; i < n - 1; i += 1) {
    if (slope[i - 1] * slope[i] <= 0) {
      tangent.push(0);
      continue;
    }
    const w1 = 2 * dx[i] + dx[i - 1];
    const w2 = dx[i] + 2 * dx[i - 1];
    tangent.push((w1 + w2) / (w1 / slope[i - 1] + w2 / slope[i]));
  }
  tangent.push(slope[n - 2]);
  let path = head;
  for (let i = 0; i < n - 1; i += 1) {
    const third = dx[i] / 3;
    path += `C${round(points[i].x + third)},${round(points[i].y + tangent[i] * third)} ${round(points[i + 1].x - third)},${round(points[i + 1].y - tangent[i + 1] * third)} ${round(points[i + 1].x)},${round(points[i + 1].y)}`;
  }
  return path;
}

/** A null breaks the line; runs of one point draw nothing. */
export function segmentedMonotonePath(points: Array<Point | null>): string {
  let path = "";
  let run: Point[] = [];
  for (const p of [...points, null]) {
    if (p) run.push(p);
    else {
      if (run.length > 1) path += monotonePath(run);
      run = [];
    }
  }
  return path;
}

export function niceTicks(min: number, max: number, count: number): number[] {
  const span = max - min;
  if (!(span > 0)) return [min];
  const target = span / Math.max(1, count - 1);
  const magnitude = 10 ** Math.floor(Math.log10(target));
  // Nearest nice step, not rounded up: rounding up collapses a 5-tick request to 2 labels on narrow domains.
  const scaled = target / magnitude;
  const size = magnitude * (scaled < 1.5 ? 1 : scaled < 3 ? 2 : scaled < 7 ? 5 : 10);
  const ticks: number[] = [];
  for (let v = Math.ceil(min / size) * size; v <= max + size * 0.001; v += size) ticks.push(Number(v.toFixed(4)));
  return ticks;
}

export const formatPlat = (value: number, locale: string): string => {
  const digits = Number.isInteger(value) ? 0 : 1;
  return value.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits });
};

// Days are UTC midnights; the label is read in UTC too so it never slips over a timezone boundary.
export const formatStamp = (time: number, locale: string): string => new Date(time).toLocaleDateString(locale, { month: "short", day: "numeric", timeZone: "UTC" });

/** `rows` must hold at least two points. */
export function buildChart(rows: readonly StatRow[], width: number, locale: string): ChartModel {
  const plotW = width - MARGIN.left - MARGIN.right;
  const barWidth = Math.max(1, Math.min(18, plotW / (rows.length - 1) - 2));
  const candleWidth = Math.max(3, barWidth * 0.6);
  const inset = Math.max(barWidth, candleWidth) / 2;
  const step = (plotW - 2 * inset) / (rows.length - 1);
  const xAt = (i: number): number => MARGIN.left + inset + i * step;

  let low = Infinity;
  let high = -Infinity;
  let peakVolume = 0;
  // Candle wicks share the price domain so the scale never shifts when a series is toggled.
  for (const r of rows) {
    low = Math.min(low, r.donchBot ?? r.median, r.minPrice ?? r.median);
    high = Math.max(high, r.donchTop ?? r.median, r.maxPrice ?? r.median);
    peakVolume = Math.max(peakVolume, r.volume ?? 0);
  }
  const spread = high - low;
  const pad = spread > 0 ? spread * 0.08 : Math.max(1, high * 0.05);
  const domainLow = Math.max(0, low - pad);
  const domainHigh = high + pad;
  const priceSpan = domainHigh - domainLow || 1;
  const priceY = (v: number): number => PRICE_BASE - ((v - domainLow) / priceSpan) * PRICE_H;
  const volumeY = (v: number): number => VOLUME_BASE - (peakVolume > 0 ? (v / peakVolume) * (VOLUME_BASE - VOLUME_TOP) : 0);

  const bands: string[] = [];
  let top: string[] = [];
  let bottom: string[] = [];
  const finishBand = (): void => {
    if (top.length > 0) bands.push(`${top.join("")}${bottom.reverse().join("")}Z`);
    top = [];
    bottom = [];
  };
  rows.forEach((r, i) => {
    if (r.donchTop == null || r.donchBot == null) return finishBand();
    top.push(`${top.length === 0 ? "M" : "L"}${round(xAt(i))},${round(priceY(r.donchTop))}`);
    bottom.push(`L${round(xAt(i))},${round(priceY(r.donchBot))}`);
  });
  finishBand();

  const labelCount = Math.max(2, Math.min(rows.length, Math.round(plotW / X_LABEL_SPACING)));
  const xLabels = Array.from({ length: labelCount }, (_, i) => {
    const index = Math.round((i / (labelCount - 1)) * (rows.length - 1));
    const anchor: "start" | "middle" | "end" = i === 0 ? "start" : i === labelCount - 1 ? "end" : "middle";
    return { x: xAt(index), label: formatStamp(rows[index].time, locale), anchor };
  });
  const opt = (v: number | null | undefined, i: number): Point | null => (v == null ? null : { x: xAt(i), y: priceY(v) });

  return {
    width,
    band: bands.join(""),
    median: monotonePath(rows.map((r, i) => ({ x: xAt(i), y: priceY(r.median) }))),
    movingAvg: segmentedMonotonePath(rows.map((r, i) => opt(r.movingAvg, i))),
    avgPrice: segmentedMonotonePath(rows.map((r, i) => opt(r.avgPrice, i))),
    candles: rows.flatMap((r, i) => {
      if (r.openPrice == null || r.closedPrice == null || r.maxPrice == null || r.minPrice == null) return [];
      const openY = priceY(r.openPrice);
      const closeY = priceY(r.closedPrice);
      return [{ x: xAt(i) - candleWidth / 2, width: candleWidth, wickTop: priceY(r.maxPrice), wickBottom: priceY(r.minPrice), bodyY: Math.min(openY, closeY), bodyH: Math.max(1, Math.abs(openY - closeY)), rising: r.closedPrice >= r.openPrice }];
    }),
    priceTicks: niceTicks(domainLow, domainHigh, 5).map((value) => ({ value, offset: priceY(value), label: formatPlat(value, locale) })),
    volumeTicks: niceTicks(0, peakVolume, 3).map((value) => ({ value, offset: volumeY(value), label: formatPlat(value, locale) })),
    xLabels,
    bars: rows.map((r, i) => {
      const y = volumeY(r.volume ?? 0);
      return { x: xAt(i) - barWidth / 2, y, width: barWidth, height: Math.max((r.volume ?? 0) > 0 ? 1 : 0, VOLUME_BASE - y) };
    }),
    markers: rows.map((r, i) => ({ x: xAt(i), medianY: priceY(r.median), movingY: r.movingAvg == null ? null : priceY(r.movingAvg), avgY: r.avgPrice == null ? null : priceY(r.avgPrice) })),
    step,
  };
}
