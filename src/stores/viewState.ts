// Per-view UI state that outlives the view (App unmounts a view on every switch). Memory only: reset when the app closes.
import { useCallback, type SetStateAction } from "react";
import { createSignal } from "../lib/signal";

const resets = new Set<() => void>();

/** Back to every store's initial value (tests, so state does not leak between cases). */
export const resetViewStates = (): void => resets.forEach((reset) => reset());

/** One object of related fields per view. `useField("x")` reads and writes like `useState`, with a stable setter. */
export function createViewState<T extends object>(initial: T) {
  const sig = createSignal(initial);
  resets.add(() => sig.set(initial));
  const useField = <K extends keyof T>(key: K): [T[K], (next: SetStateAction<T[K]>) => void] => {
    const value = sig.use()[key];
    const set = useCallback(
      (next: SetStateAction<T[K]>) => {
        const cur = sig.get();
        const v = typeof next === "function" ? (next as (prev: T[K]) => T[K])(cur[key]) : next;
        if (!Object.is(v, cur[key])) sig.set({ ...cur, [key]: v });
      },
      [key],
    );
    return [value, set];
  };
  return { useField, get: sig.get, patch: (p: Partial<T>) => sig.set({ ...sig.get(), ...p }) };
}
