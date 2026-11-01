import { useSyncExternalStore } from "react";

/** A tiny observable value: module-level UI state that components read with `use()`. */
export function createSignal<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  const get = (): T => value;
  const subscribe = (listener: () => void): (() => void) => {
    listeners.add(listener);
    return () => void listeners.delete(listener);
  };
  const set = (next: T): void => {
    if (Object.is(next, value)) return;
    value = next;
    listeners.forEach((listener) => listener());
  };
  return { get, set, subscribe, use: (): T => useSyncExternalStore(subscribe, get) };
}
