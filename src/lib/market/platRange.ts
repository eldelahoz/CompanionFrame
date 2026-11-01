// A plat bound applies to both selling lists, so how to treat a row whose price
// is unknown lives here: it drops out instead of quietly passing an unmeasured filter.

export interface PlatRange {
  min: number | null;
  max: number | null;
}

export const NO_PLAT_RANGE: PlatRange = { min: null, max: null };

export function platRangeActive(range: PlatRange): boolean {
  return range.min !== null || range.max !== null;
}

/** An unknown price passes only while neither bound is set. */
export function withinPlatRange(price: number | null | undefined, range: PlatRange): boolean {
  if (price == null) return !platRangeActive(range);
  if (range.min !== null && price < range.min) return false;
  if (range.max !== null && price > range.max) return false;
  return true;
}
