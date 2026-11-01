import { useSyncExternalStore } from "react";

/**
 * A module-level store read with useSyncExternalStore. `start` runs when the first component subscribes (fetch + event
 * listeners) and its cleanup when the last leaves; the value is kept, so a returning view paints it at once while `start`
 * refreshes in the background.
 */
export function createSharedStore<T>(initial: T, start: (patch: (p: Partial<T>) => void) => () => void) {
  let state = initial;
  let stop: (() => void) | null = null;
  const listeners = new Set<() => void>();
  const patch = (p: Partial<T>) => {
    state = { ...state, ...p };
    listeners.forEach((l) => l());
  };
  const subscribe = (l: () => void) => {
    if (listeners.size === 0) stop = start(patch);
    listeners.add(l);
    return () => {
      listeners.delete(l);
      if (listeners.size === 0) {
        stop?.();
        stop = null;
      }
    };
  };
  const getSnapshot = () => state;
  return {
    use: () => useSyncExternalStore(subscribe, getSnapshot),
    /** Test seam: forget the cached value. */
    reset: () => {
      state = initial;
    },
  };
}

const memos = new WeakMap<object, Map<string, WeakMap<object, unknown>>>();

/** Computes `fn` once per `(data, db, key)` identity pair; a new `data` object from an event recomputes it. */
export function memoByIdentity<R>(key: string, data: object, db: object, fn: () => R): R {
  let byKey = memos.get(data);
  if (!byKey) memos.set(data, (byKey = new Map()));
  let byDb = byKey.get(key);
  if (!byDb) byKey.set(key, (byDb = new WeakMap()));
  if (!byDb.has(db)) byDb.set(db, fn());
  return byDb.get(db) as R;
}
