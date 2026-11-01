export type SortDirection = "asc" | "desc";

/** Header-click sort: a new column starts at `start(key)`, the active one flips. */
export function nextColumnSort<K extends string>(
  current: { sortBy: K; sortDirection: SortDirection },
  sortKey: K,
  start: (sortKey: K) => SortDirection,
): { sortBy: K; sortDirection: SortDirection } {
  if (current.sortBy !== sortKey) return { sortBy: sortKey, sortDirection: start(sortKey) };
  return { sortBy: sortKey, sortDirection: current.sortDirection === "asc" ? "desc" : "asc" };
}
