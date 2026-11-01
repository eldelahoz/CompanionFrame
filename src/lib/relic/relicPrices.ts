import { useEffect, useState } from "react";
import { wfmPricesBulk } from "../ipc";

// Median platinum per warframe.market slug, kept for the session so the planner, the detail modal and the overlay share one lookup.
const prices = new Map<string, number | null>();
const asked = new Set<string>();
const listeners = new Set<() => void>();
const CHUNK = 40;

export const relicPrice = (urlName: string): number | null => prices.get(urlName) ?? null;

/** Fetches the slugs not seen yet; returns a revision that bumps as prices land so callers recompute. */
export function useRelicPrices(slugs: readonly string[]): number {
  const [rev, setRev] = useState(0);
  useEffect(() => {
    const bump = () => setRev((r) => r + 1);
    listeners.add(bump);
    return () => void listeners.delete(bump);
  }, []);
  const key = slugs.join("|");
  useEffect(() => {
    const missing = [...new Set(slugs)].filter((s) => !asked.has(s));
    missing.forEach((s) => asked.add(s));
    (async () => {
      for (let i = 0; i < missing.length; i += CHUNK) {
        const chunk = missing.slice(i, i + CHUNK);
        try {
          for (const entry of await wfmPricesBulk(chunk)) prices.set(entry.slug, entry.status === "ok" ? (entry.price ?? null) : null);
        } catch {
          chunk.forEach((s) => asked.delete(s)); // a failed fetch is retried the next time the slug is wanted
          return;
        }
        listeners.forEach((l) => l());
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return rev;
}
