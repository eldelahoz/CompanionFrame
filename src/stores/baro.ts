import { useCallback, useEffect, useState } from "react";
import { normalizeBaroHistory, type BaroHistory } from "../../config/shared/baroHistory";
import { storeItemPath } from "../../config/shared/itemPath";
import type { VaultTrader } from "../lib/world/baroPlanner";
import { recordBaroVisit } from "../lib/world/baroRecord";

const HISTORY_KEY = "baro-history-v1";
const WISHLIST_KEY = "baro-wishlist-v1";
const BARO_WISHLIST_MAX = 500;

function read<T>(key: string, normalize: (v: unknown) => T, empty: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? normalize(JSON.parse(raw)) : empty;
  } catch {
    return empty;
  }
}
function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // best effort: the in-memory copy still serves this session
  }
}

function normalizeWishlist(value: unknown): Record<string, number> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return {};
  const result: Record<string, number> = {};
  for (const [key, quantity] of Object.entries(value).slice(0, BARO_WISHLIST_MAX)) {
    if (key.startsWith("/Lotus/") && key.length <= 512 && typeof quantity === "number" && Number.isSafeInteger(quantity) && quantity > 0 && quantity <= 99) {
      const canonical = storeItemPath(key);
      result[canonical] = Math.max(result[canonical] ?? 0, quantity);
    }
  }
  return result;
}

/** Wanted Baro items by uniqueName, kept in localStorage. */
export function useBaroWishlist() {
  const [wishlist, setWishlist] = useState(() => read(WISHLIST_KEY, normalizeWishlist, {}));
  useEffect(() => {
    const sync = (e: StorageEvent) => (e.key === WISHLIST_KEY || e.key === null) && setWishlist(read(WISHLIST_KEY, normalizeWishlist, {}));
    window.addEventListener("storage", sync);
    return () => window.removeEventListener("storage", sync);
  }, []);
  /** Returns false when the quantity was refused (invalid name or the list is full). */
  const setQuantity = useCallback((uniqueName: string, quantity: number): boolean => {
    if (!uniqueName.startsWith("/Lotus/") || uniqueName.length > 512 || !Number.isFinite(quantity)) return false;
    const canonical = storeItemPath(uniqueName);
    const next = read(WISHLIST_KEY, normalizeWishlist, {});
    if (quantity <= 0) delete next[canonical];
    else {
      if (!(canonical in next) && Object.keys(next).length >= BARO_WISHLIST_MAX) return false;
      next[canonical] = Math.min(99, Math.max(1, Math.floor(quantity)));
    }
    write(WISHLIST_KEY, next);
    setWishlist(next);
    return true;
  }, []);
  return { wishlist, setQuantity, full: Object.keys(wishlist).length >= BARO_WISHLIST_MAX };
}

/** The locally recorded Baro history; the visit in `trader` is folded in while it is active. */
export function useBaroHistory(trader: VaultTrader | null | undefined): BaroHistory | null {
  const [history, setHistory] = useState(() => read(HISTORY_KEY, normalizeBaroHistory, null));
  useEffect(() => {
    setHistory((prev) => {
      const next = recordBaroVisit(prev, trader, Date.now());
      if (next !== prev) write(HISTORY_KEY, next);
      return next;
    });
  }, [trader]);
  return history;
}
