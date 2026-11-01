/** Reward and drop-table names carry the bonus count ("2X Forma Blueprint"),
 *  and a card prints it as "2 X", so "2X", "2 X", "2x" and "2 x" all fold to
 *  one before a name is compared. No real item name opens with a count, but
 *  "X3lp Glyph" does open with an x, so a leading "x2" is never a count. */
const QUANTITY_PREFIX = /^\d+\s*x\s+/i;

export function hasQuantityPrefix(value: string): boolean {
  return QUANTITY_PREFIX.test(value);
}

export function stripQuantityPrefix(value: string): string {
  return value.replace(QUANTITY_PREFIX, "");
}
