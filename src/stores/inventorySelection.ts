import { createSignal } from "../lib/signal";
import { readStorage, writeStorage } from "../lib/persistence";

/** A named set of inventory selection keys the user can re-apply later. */
interface SavedSelection {
  name: string;
  keys: string[];
}

const SAVED_KEY = "wf_inventory_saved_selections";
const MAX_SAVED = 25;

const loadSaved = (): SavedSelection[] => {
  try {
    const parsed: unknown = JSON.parse(readStorage(SAVED_KEY) ?? "[]");
    return (Array.isArray(parsed) ? parsed : [])
      .filter((e): e is SavedSelection => typeof e?.name === "string" && e.name.length > 0 && Array.isArray(e.keys))
      .map((e) => ({ name: e.name, keys: e.keys.filter((k: unknown): k is string => typeof k === "string") }))
      .slice(0, MAX_SAVED);
  } catch {
    return [];
  }
};

const mode = createSignal(false);
const selected = createSignal<ReadonlySet<string>>(new Set());
const saved = createSignal<readonly SavedSelection[]>(loadSaved());

export const useSelectionMode = mode.use;
export const useSelection = selected.use;
export const useSavedSelections = saved.use;
export const setSelectionMode = mode.set;

/** Leaving the mode keeps the picks; only Clear or loading a saved set replaces them. */
export function toggleSelected(key: string): void {
  const next = new Set(selected.get());
  if (!next.delete(key)) next.add(key);
  selected.set(next);
}
export const selectKeys = (keys: Iterable<string>): void => selected.set(new Set(keys));
export const clearSelection = (): void => selected.set(new Set());

const persist = (next: SavedSelection[]): void => {
  saved.set(next);
  writeStorage(SAVED_KEY, JSON.stringify(next));
};

/** Saving under an existing name overwrites it. */
export function saveSelection(name: string): void {
  const trimmed = name.trim();
  if (!trimmed) return;
  persist([{ name: trimmed, keys: [...selected.get()] }, ...saved.get().filter((s) => s.name !== trimmed)].slice(0, MAX_SAVED));
}
export function loadSavedSelection(name: string): void {
  const entry = saved.get().find((s) => s.name === name);
  if (entry) selectKeys(entry.keys);
}
export const deleteSavedSelection = (name: string): void => persist(saved.get().filter((s) => s.name !== name));
