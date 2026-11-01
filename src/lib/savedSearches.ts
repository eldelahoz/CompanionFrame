import { useCallback, useState } from "react";
import { readStorage, writeStorage } from "./persistence";

const load = (key: string): string[] => {
  try {
    const parsed: unknown = JSON.parse(readStorage(key) ?? "[]");
    return Array.isArray(parsed) ? parsed.filter((s): s is string => typeof s === "string") : [];
  } catch {
    return [];
  }
};

/** Add `text` unless it is blank or already saved (case-insensitive). */
export const withSearch = (list: string[], text: string): string[] => {
  const q = text.trim();
  return !q || list.some((s) => s.toLowerCase() === q.toLowerCase()) ? list : [...list, q];
};

/** A per-scope saved-search list kept in localStorage (`savedSearches.<scope>`). */
export function useSavedSearches(scope: string) {
  const key = `savedSearches.${scope}`;
  const [state, setState] = useState(() => ({ key, list: load(key) }));
  // Switching scope (the Wiki's search mode) swaps the list during render rather than in an effect.
  const list = state.key === key ? state.list : load(key);
  if (state.key !== key) setState({ key, list });
  const commit = useCallback((next: string[]) => { setState({ key, list: next }); writeStorage(key, JSON.stringify(next)); }, [key]);
  return {
    list,
    add: (text: string) => commit(withSearch(list, text)),
    remove: (text: string) => commit(list.filter((s) => s !== text)),
  };
}
