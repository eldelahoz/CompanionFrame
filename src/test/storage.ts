import { vi } from "vitest";

/** Stubs `localStorage` with a Map-backed store and returns the Map so tests can seed and inspect it. */
export function stubStorage(seed: Record<string, string> = {}): Map<string, string> {
  const mem = new Map(Object.entries(seed));
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => mem.get(key) ?? null,
    setItem: (key: string, value: string) => void mem.set(key, value),
    removeItem: (key: string) => void mem.delete(key),
  });
  return mem;
}
