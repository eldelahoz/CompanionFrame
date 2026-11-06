import { useEffect, useMemo, useState } from "react";
import { onPricesRefreshed, wfmPricesLoad, wfmStatsHistory, type MarketStatPoint, type PriceEntry } from "../ipc";

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

const NONE: readonly string[] = [];
const LOADING: PriceInfo = { state: "loading", price: null, maxPrice: null, avg: null, maxAvg: null, avgState: "loading" };
const STATS_WORKERS = 3;
const AVG_DAYS = 7;
const DAY_MS = 86_400_000;

// Module level, so switching tabs or views never refetches. A `failed` answer (transient error, #59) is never stored:
// the slug is simply asked again on the next load, whereas `no_data` is a real answer and stays.
// The backend does the loading (one request, its own queue, the Prices job in the titlebar pill); this only keeps the answers.
const prices = new Map<string, { price: number | null; maxPrice: number | null }>();
const averages = new Map<string, { avg: number | null; maxAvg: number | null }>();
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

export const resetItemPricesForTest = (): void => {
  prices.clear();
  averages.clear();
  statsQueue.length = 0;
  queued.clear();
  failedPrices.clear();
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

/** Slugs the backend gave up on: shown as a retry state until `retryFailedPrices`. */
const failedPrices = new Set<string>();

/** Stores an answer from a bulk call or a background refresh. Returns false for a `failed` one. */
function applyEntry(entry: PriceEntry): boolean {
  if (entry.status === "ok") prices.set(entry.slug, { price: entry.price ?? null, maxPrice: entry.maxPrice ?? null });
  // An empty answer never replaces a price already known.
  else if (entry.status === "no_data" && !prices.get(entry.slug)?.price) prices.set(entry.slug, { price: null, maxPrice: null });
  else if (entry.status === "failed") return false;
  return true;
}

/** What the backend answered, from the call itself (its cache) or from `wfm-prices-refreshed` (its queue). */
export function applyRefreshed(entries: readonly PriceEntry[], recompute = true): void {
  for (const e of entries) {
    if (e.status === "failed") prices.has(e.slug) || failedPrices.add(e.slug);
    else (failedPrices.delete(e.slug), applyEntry(e));
  }
  notify();
  if (!recompute) return;
  // The refresh also rewrote the statistics history: recompute the medians already shown (a local read, no request).
  void Promise.all(entries.filter((e) => e.status === "ok" && averages.has(e.slug)).map((e) => loadAverage(e.slug)));
}

let listening = false;
function listenForRefresh(): void {
  if (listening) return;
  listening = true;
  void onPricesRefreshed(applyRefreshed);
}
/**
 * One backend request for everything wanted, rows on screen first. The backend answers from its cache at once and
 * fetches the rest in its single worker queue, the latest request ahead of older ones, so a new tab's slugs jump the queue.
 */
function requestPrices(slugs: readonly string[]): void {
  const ask = [...new Set(slugs)].filter((s) => !prices.has(s) && !failedPrices.has(s));
  if (ask.length === 0) return;
  // One retry: the custom IPC protocol sometimes drops a call right after a reload or a burst.
  void wfmPricesLoad(ask)
    .catch(() => wfmPricesLoad(ask))
    // The call's own answer is the backend's cache: nothing was rewritten, so the medians stay.
    .then((entries) => applyRefreshed(entries, false), () => {
      ask.forEach((s) => failedPrices.add(s));
      notify();
    });
}

/** Asks again for the slugs that gave up. */
function retryFailedPrices(): void {
  const again = [...failedPrices];
  failedPrices.clear();
  requestPrices(again);
  notify();
}

const statsQueue: string[] = [];
const queued = new Set<string>();
let workers = 0;

async function loadAverage(slug: string): Promise<void> {
  try {
    averages.set(slug, mediansOfRecent(await wfmStatsHistory(slug)));
    notify();
  } catch {
    // transient: not stored, asked again next time
  }
}

async function statsWorker(): Promise<void> {
  workers++;
  for (let slug = statsQueue.shift(); slug !== undefined; slug = statsQueue.shift()) {
    await loadAverage(slug);
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
 * `slugs` are asked first, `later` queues behind them (and is part of the returned map); the backend loads them as one
 * background job. `avgSlugs` also get the weekly median from the statistics history, one call per slug, three at a time.
 */
export function useItemPrices(slugs: readonly string[], avgSlugs: readonly string[], later: readonly string[] = NONE): ReadonlyMap<string, PriceInfo> {
  const version = useVersion();
  useEffect(listenForRefresh, []);
  useEffect(() => {
    requestPrices([...slugs, ...later]);
  }, [slugs, later]);
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
