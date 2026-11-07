import { useEffect, useMemo, useState } from "react";
import { onPricesRefreshed, wfmPricesLoad, type PriceEntry } from "../ipc";

/** `loading` until the first answer; `none` is a settled "no price" (untradable or no sale in 48 h). */
type PriceState = "loading" | "ok" | "none";

export interface PriceInfo {
  state: PriceState;
  /** warframe.market 48 h average sale, the same figure the Market and Relics views show. */
  price: number | null;
  /** The 48 h average at the item's max mod rank (same response); null for unranked items or without a sale there. */
  maxPrice: number | null;
  /** Median of the last week of daily medians, computed by the backend with the price; null until loaded or without sales. */
  avg: number | null;
  /** The same median over the item's highest-rank history rows; null for unranked items or without sales there. */
  maxAvg: number | null;
  avgState: PriceState;
}

const NONE: readonly string[] = [];
const LOADING: PriceInfo = { state: "loading", price: null, maxPrice: null, avg: null, maxAvg: null, avgState: "loading" };

// Module level, so switching tabs or views never refetches. A `failed` answer (transient error, #59) is never stored:
// the slug is simply asked again on the next load, whereas `no_data` is a real answer and stays.
// The backend does the loading (one request, its own queue, the Prices job in the titlebar pill); this only keeps the answers.
const prices = new Map<string, { price: number | null; maxPrice: number | null; avg: number | null; maxAvg: number | null }>();
const listeners = new Set<() => void>();
/**
 * The first change is heard at once; the rest of a burst (price batches, medians, bulk answers) is folded into one more
 * call at the end of the window, so the last prices always land. A frame (16 ms) would still re-sort 60 times a second.
 */
const NOTIFY_MS = 250;
let notifyWindow: ReturnType<typeof setTimeout> | null = null;
let dirty = false;
function notify(): void {
  if (notifyWindow !== null) {
    dirty = true;
    return;
  }
  listeners.forEach((l) => l());
  notifyWindow = setTimeout(() => {
    notifyWindow = null;
    if (dirty) {
      dirty = false;
      notify();
    }
  }, NOTIFY_MS);
}

export const resetItemPricesForTest = (): void => {
  if (notifyWindow !== null) clearTimeout(notifyWindow);
  notifyWindow = null;
  dirty = false;
  prices.clear();
  failedPrices.clear();
};

/** Slugs the backend gave up on: shown as a retry state until `retryFailedPrices`. */
const failedPrices = new Set<string>();

/** Stores an answer from a bulk call or a background refresh. Returns false for a `failed` one. */
function applyEntry(entry: PriceEntry): boolean {
  if (entry.status === "ok") prices.set(entry.slug, { price: entry.price ?? null, maxPrice: entry.maxPrice ?? null, avg: entry.avg ?? null, maxAvg: entry.maxAvg ?? null });
  // An empty answer never replaces a price already known.
  else if (entry.status === "no_data" && !prices.get(entry.slug)?.price) prices.set(entry.slug, { price: null, maxPrice: null, avg: null, maxAvg: null });
  else if (entry.status === "failed") return false;
  return true;
}

/** What the backend answered, from the call itself (its cache) or from `wfm-prices-refreshed` (its queue). */
export function applyRefreshed(entries: readonly PriceEntry[]): void {
  for (const e of entries) {
    if (e.status === "failed") prices.has(e.slug) || failedPrices.add(e.slug);
    else (failedPrices.delete(e.slug), applyEntry(e));
  }
  notify();
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
    .then(applyRefreshed, () => {
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

/** Last known price of a slug, for code that cannot subscribe; `useItemPrices` is what loads it. */
export const itemPrice = (slug: string): number | null => prices.get(slug)?.price ?? null;

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
 * background job; each answer carries the weekly median too.
 */
export function useItemPrices(slugs: readonly string[], later: readonly string[] = NONE): ReadonlyMap<string, PriceInfo> {
  const version = useVersion();
  useEffect(listenForRefresh, []);
  useEffect(() => {
    requestPrices([...slugs, ...later]);
  }, [slugs, later]);
  return useMemo(() => {
    const out = new Map<string, PriceInfo>();
    for (const slug of [...slugs, ...later]) {
      const p = prices.get(slug);
      out.set(slug, {
        ...LOADING,
        ...(p ? { state: p.price == null ? "none" : "ok", price: p.price, maxPrice: p.maxPrice, avgState: p.avg == null ? "none" : "ok", avg: p.avg, maxAvg: p.maxAvg } : {}),
      } as PriceInfo);
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `version` marks cache changes
  }, [slugs, later, version]);
}
