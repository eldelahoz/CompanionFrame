import { useState } from "react";

/** A string choice kept in localStorage; anything outside `allowed` (or no storage) falls back to `fallback`. */
export function usePersisted<T extends string>(key: string, allowed: readonly T[], fallback: T): [T, (value: T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const stored = localStorage.getItem(key);
      return allowed.includes(stored as T) ? (stored as T) : fallback;
    } catch {
      return fallback;
    }
  });
  return [
    value,
    (next) => {
      setValue(next);
      try {
        localStorage.setItem(key, next);
      } catch {
        // the preference is best-effort
      }
    },
  ];
}
