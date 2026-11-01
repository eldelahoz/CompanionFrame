import { useEffect, useMemo, useState } from "react";
import { onPricesRefreshed, wfmPricesBulk, wfmStatsHistory, type MarketStatPoint, type PriceEntry } from "../ipc";

/** `loading` until the first answer; `none` is a settled "no price" (untradable or no sale in 48 h). */
type PriceState = "loading" | "ok" | "none";

export interface PriceInfo {
  state: PriceState;
  /** warframe.market 48 h average sale, the same figure the Market and Relics views show. */
  price: number | null;
  /** The 48 h average at the item's max mod rank (same response); null for unranked items or without a sale there. */
  maxPrice: number | null;
  /** Median of the last week of daily medians from the statistics history; null until loaded or without sales. */
  avg: number | null;
  /** The same median over the item's highest-rank history rows; null for unranked items or without sales there. */
  maxAvg: number | null;
  avgState: PriceState;
}

const LOADING: PriceInfo = { state: "loading", price: null, maxPrice: null, avg: null, maxAvg: null, avgState: "loading" };
/** Small chunks, one on the wire at a time: the backend already fans each out 8-wide, one warframe.market request per slug. */
const BULK_CHUNK = 40;
const MAX_ATTEMPTS = 4;
const BACKOFF_MS = 2_000;
const STATS_WORKERS = 3;
const AVG_DAYS = 7;
const DAY_MS = 86_400_000;

// Module level, so switching tabs or views never refetches. A `failed` answer (transient error, #59) is never stored:
// the slug is simply asked again on the next load, whereas `no_data` is a real answer and stays.
const prices = new Map<string, { price: number | null; maxPrice: number | null }>();
const averages = new Map<string, { avg: number | null; maxAvg: number | null }>();
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

export const resetItemPricesForTest = (): void => {
  prices.clear();
  averages.clear();
  statsQueue.length = 0;
  queued.clear();
  inFlight.clear();
  priceQueue = [];
  queuedPrices.clear();
  attempts.clear();
  failedPrices.clear();
  streak = 0;
};

/** Rank 0 by default; `rank` picks one rank's rows. */
export function medianOfRecent(points: readonly MarketStatPoint[], now = Date.now(), rank = 0): number | null {
  const values = points
    .filter((p) => p.time != null && p.median != null && now - p.time <= AVG_DAYS * DAY_MS && (p.rank ?? 0) === rank)
    .map((p) => p.median as number)
    .sort((a, b) => a - b);
  if (values.length === 0) return null;
  const mid = values.length >> 1;
  return values.length % 2 ? values[mid] : (values[mid - 1] + values[mid]) / 2;
}

/** Medians for rank 0 and for the highest rank the recent history has rows for (the same payload). */
export function mediansOfRecent(points: readonly MarketStatPoint[], now = Date.now()): { avg: number | null; maxAvg: number | null } {
  const top = Math.max(0, ...points.filter((p) => p.time != null && now - p.time <= AVG_DAYS * DAY_MS).map((p) => p.rank ?? 0));
  return { avg: medianOfRecent(points, now), maxAvg: top > 0 ? medianOfRecent(points, now, top) : null };
}

const inFlight = new Set<string>();

/** Stores an answer from a bulk call or a background refresh. Returns false for a `failed` one. */
function applyEntry(entry: PriceEntry): boolean {
  if (entry.status === "ok") prices.set(entry.slug, { price: entry.price ?? null, maxPrice: entry.maxPrice ?? null });
  // An empty answer never replaces a price already known.
  else if (entry.status === "no_data" && !prices.get(entry.slug)?.price) prices.set(entry.slug, { price: null, maxPrice: null });
  else if (entry.status === "failed") return false;
  return true;
}

/** Prices the backend refreshed after serving the cached ones: replace what the module cache holds. */
export function applyRefreshed(entries: readonly PriceEntry[]): void {
  entries.forEach((e) => e.status === "ok" && applyEntry(e));
  notify();
}

let listening = false;
function listenForRefresh(): void {
  if (listening) return;
  listening = true;
  void onPricesRefreshed(applyRefreshed);
}
/** One retry: the custom IPC protocol sometimes drops a call right after a reload or a burst. */
async function bulkWithRetry(chunk: string[]) {
  try {
    return await wfmPricesBulk(chunk);
  } catch {
    return wfmPricesBulk(chunk);
  }
}

// Visible rows go to the front of the queue, the rest of the inventory behind them; one chunk is asked at a time.
let priceQueue: string[] = [];
const queuedPrices = new Set<string>();
/** Slugs that kept failing: shown as a retry state until `retryFailedPrices`. */
const failedPrices = new Set<string>();
const attempts = new Map<string, number>();
let pumping = false;
let streak = 0;

function enqueuePrices(slugs: readonly string[], front: boolean): void {
  const ask = slugs.filter((s) => !prices.has(s) && !failedPrices.has(s) && !inFlight.has(s) && (front || !queuedPrices.has(s)));
  if (ask.length === 0) return;
  const asked = new Set(ask);
  const rest = priceQueue.filter((s) => !asked.has(s));
  priceQueue = front ? [...ask, ...rest] : [...rest, ...ask];
  ask.forEach((s) => queuedPrices.add(s));
  void pump();
}

async function pump(): Promise<void> {
  if (pumping) return;
  pumping = true;
  while (priceQueue.length > 0) {
    const chunk = priceQueue.splice(0, BULK_CHUNK);
    chunk.forEach((s) => (queuedPrices.delete(s), inFlight.add(s)));
    const bad = new Set<string>();
    try {
      for (const entry of await bulkWithRetry(chunk)) if (!applyEntry(entry)) bad.add(entry.slug);
    } catch {
      chunk.forEach((s) => bad.add(s));
    }
    chunk.forEach((s) => inFlight.delete(s));
    for (const s of bad) {
      const n = (attempts.get(s) ?? 0) + 1;
      attempts.set(s, n);
      if (n >= MAX_ATTEMPTS) failedPrices.add(s);
      else (priceQueue.push(s), queuedPrices.add(s));
    }
    notify();
    // A failing chunk usually means the rate limit: the whole queue waits, longer each time in a row.
    streak = bad.size > 0 ? streak + 1 : 0;
    if (streak > 0) await new Promise((r) => setTimeout(r, BACKOFF_MS * 2 ** Math.min(streak - 1, 4)));
  }
  pumping = false;
}

/** Asks again for the slugs that gave up. */
function retryFailedPrices(): void {
  const again = [...failedPrices];
  failedPrices.clear();
  again.forEach((s) => attempts.delete(s));
  streak = 0;
  enqueuePrices(again, true);
  notify();
}

const statsQueue: string[] = [];
const queued = new Set<string>();
let workers = 0;

async function statsWorker(): Promise<void> {
  workers++;
  for (let slug = statsQueue.shift(); slug !== undefined; slug = statsQueue.shift()) {
    try {
      averages.set(slug, mediansOfRecent(await wfmStatsHistory(slug)));
      notify();
    } catch {
      // transient: not stored, asked again next time
    }
    queued.delete(slug);
  }
  workers--;
}

function enqueueStats(slugs: string[]): void {
  for (const slug of slugs) {
    if (averages.has(slug) || queued.has(slug)) continue;
    queued.add(slug);
    statsQueue.push(slug);
  }
  while (workers < STATS_WORKERS && statsQueue.length > 0) void statsWorker();
}

function useVersion(): number {
  const [version, setVersion] = useState(0);
  useEffect(() => {
    const bump = () => setVersion((v) => v + 1);
    listeners.add(bump);
    return () => void listeners.delete(bump);
  }, []);
  return version;
}

/** How many of `slugs` have an answer, and how many gave up (retry with `retry`). */
export function useItemPriceProgress(slugs: readonly string[]): { done: number; failed: number; total: number; retry: () => void } {
  const version = useVersion();
  return useMemo(
    () => ({ done: slugs.filter((s) => prices.has(s)).length, failed: slugs.filter((s) => failedPrices.has(s)).length, total: slugs.length, retry: retryFailedPrices }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `version` marks cache changes
    [slugs, version],
  );
}

/**
 * `slugs` are asked first, `later` queues behind them (and is part of the returned map); one bulk call of 40 at a time with
 * backoff on failure. `avgSlugs` also get the weekly median from the statistics history, one call per slug, three at a time.
 */
export function useItemPrices(slugs: readonly string[], avgSlugs: readonly string[], later: readonly string[] = []): ReadonlyMap<string, PriceInfo> {
  const version = useVersion();
  useEffect(listenForRefresh, []);
  useEffect(() => {
    // The pump starts on the first call, so the rows on screen go in before the background ones.
    enqueuePrices(slugs, true);
    enqueuePrices(later, false);
  }, [slugs, later, version]);
  const avgKey = avgSlugs.join("|");
  useEffect(() => {
    enqueueStats([...avgSlugs]);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `avgKey` stands for the list
  }, [avgKey]);
  return useMemo(() => {
    const out = new Map<string, PriceInfo>();
    for (const slug of [...slugs, ...later]) {
      const p = prices.get(slug);
      const a = averages.get(slug);
      out.set(slug, {
        ...LOADING,
        ...(p ? { state: p.price == null ? "none" : "ok", price: p.price, maxPrice: p.maxPrice } : {}),
        ...(a !== undefined ? { avgState: a.avg == null ? "none" : "ok", avg: a.avg, maxAvg: a.maxAvg } : {}),
      } as PriceInfo);
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `version` marks cache changes
  }, [slugs, later, version]);
}
