/** Size of an ingredient chip and the gap between chips, in rem; `.fd-chip` and `.fd-chips` in mastery.css use the same numbers. */
export const CHIP_REM = 3.5;
export const CHIP_GAP_REM = 0.375;

/**
 * How many ingredient chips to draw in a row `width` px wide for `count` ingredients. When they do not all fit, the last slot
 * becomes a "+N" chip, so one fewer ingredient is drawn. An unmeasured row (`width` 0, as in jsdom) draws them all.
 */
export function fitChips(count: number, width: number, chip: number, gap: number): number {
  if (width <= 0) return count;
  const fit = Math.floor((width + gap) / (chip + gap));
  return count <= fit ? count : Math.max(0, fit - 1);
}
