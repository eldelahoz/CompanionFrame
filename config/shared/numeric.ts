/** Shared numeric utilities (main, renderer, worker). */

/** Unknown -> finite number or null. Unwraps BSON-style boxed numbers. */
export function toFiniteNumber(value: unknown): number | null {
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    const boxed =
      record.$numberInt ??
      record.$numberLong ??
      record.$numberDouble ??
      record.$numberDecimal ??
      record.$numberFloat;
    if (boxed !== undefined) return toFiniteNumber(boxed);
  }
  return null;
}

// The 3-arg form rejects non-finite values; the 4-arg form coerces and falls back.
export function clampNumber(value: number, min: number, max: number): number;
export function clampNumber(value: unknown, min: number, max: number, fallback: number): number;
export function clampNumber(value: unknown, min: number, max: number, fallback?: number): number {
  // Number(null) and Number("") are both 0, which is finite, so an absent value
  // would clamp up to the range floor instead of reaching the fallback.
  const absent = value == null || (typeof value === "string" && value.trim().length === 0);
  const n = fallback !== undefined ? (absent ? Number.NaN : Number(value)) : (value as number);
  if (!Number.isFinite(n)) {
    if (fallback === undefined) {
      throw new TypeError(`clampNumber: value must be finite, got ${String(value)}`);
    }
    return fallback;
  }
  return Math.max(min, Math.min(max, n));
}

/** Finite positive integer or null. */
export function toFinitePositiveInt(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value) && value > 0) {
    return Math.floor(value);
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed) && parsed > 0) return Math.floor(parsed);
  }
  return null;
}

/** Whether an inventory group has rank-bearing entries. */
export function isRankedGroup(group: string | null | undefined): boolean {
  return group === "mods" || group === "arcanes";
}
