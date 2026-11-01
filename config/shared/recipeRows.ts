/** DE lists a doubled ingredient as two rows of one. Summing by uniqueName keeps a
 *  single copy from satisfying both halves; rows without a uniqueName pass through. */
export function mergeDuplicateIngredients<T extends { uniqueName?: string | null }>(
  rows: readonly T[],
  countOf: (row: T) => number | undefined,
  withCount: (row: T, count: number) => T,
): T[] {
  const merged: T[] = [];
  const indexByUniqueName = new Map<string, number>();
  for (const row of rows) {
    const key = typeof row.uniqueName === "string" ? row.uniqueName : "";
    const count = countOf(row) || 1;
    const existing = key ? indexByUniqueName.get(key) : undefined;
    if (existing === undefined) {
      if (key) indexByUniqueName.set(key, merged.length);
      merged.push(withCount(row, count));
      continue;
    }
    const target = merged[existing];
    merged[existing] = withCount(target, (countOf(target) || 1) + count);
  }
  return merged;
}
