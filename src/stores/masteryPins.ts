// Mastery items pinned for the Planned tab. Module-level like the other view stores, persisted as JSON lists.
import { createSignal } from "../lib/signal";
import { readStorage, writeStorage } from "../lib/persistence";

// Each pin costs one crafting-tree walk per inventory generation, so the list stays short.
const MAX_PINS = 40;
const PINS_KEY = "mastery.pinnedItems";
const KEEP_KEY = "mastery.pinnedKeepMastered";

const load = (key: string): string[] => {
  try {
    const v: unknown = JSON.parse(readStorage(key) ?? "[]");
    return Array.isArray(v) ? [...new Set(v.filter((s): s is string => typeof s === "string"))].slice(0, MAX_PINS) : [];
  } catch {
    return [];
  }
};

const pins = createSignal<string[]>(load(PINS_KEY));
// Pins to keep even once the account masters them, so an undone auto-unpin does not re-fire.
const keep = createSignal<string[]>(load(KEEP_KEY));
const save = (sig: typeof pins, key: string, next: string[]) => (sig.set(next), writeStorage(key, JSON.stringify(next)));

export const useMasteryPins = pins.use;
export const useMasteryPinKeep = keep.use;

export function toggleMasteryPin(uniqueName: string): void {
  if (!uniqueName) return;
  const list = pins.get();
  if (list.includes(uniqueName)) {
    save(pins, PINS_KEY, list.filter((u) => u !== uniqueName));
    save(keep, KEEP_KEY, keep.get().filter((u) => u !== uniqueName));
  } else save(pins, PINS_KEY, [...list, uniqueName].slice(-MAX_PINS));
}

/** One write for several drops, so an undo restores a single snapshot. */
export function unpinMasteryItems(uniqueNames: readonly string[]): void {
  if (uniqueNames.length === 0) return;
  const drop = new Set(uniqueNames);
  save(pins, PINS_KEY, pins.get().filter((u) => !drop.has(u)));
  save(keep, KEEP_KEY, keep.get().filter((u) => !drop.has(u)));
}

/** Undo of one auto-unpin: re-pin those items and keep them through mastery. */
export function restoreMasteryPins(uniqueNames: readonly string[]): void {
  const have = new Set(pins.get());
  const back = uniqueNames.filter((u) => !have.has(u));
  save(pins, PINS_KEY, [...pins.get(), ...back].slice(-MAX_PINS));
  save(keep, KEEP_KEY, [...new Set([...keep.get(), ...uniqueNames])].slice(-MAX_PINS));
}
