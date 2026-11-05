import { useRef, type ChangeEvent } from "react";

/** Whether the click behind a checkbox change had Shift held. */
export const shiftOf = (e: ChangeEvent<HTMLInputElement>): boolean => (e.nativeEvent as MouseEvent).shiftKey === true;

/** A copy of `set` with `ids` selected (`on`) or unselected. */
export function applyIds(set: ReadonlySet<string>, ids: readonly string[], on: boolean): Set<string> {
  const next = new Set(set);
  for (const id of ids) on ? next.add(id) : next.delete(id);
  return next;
}

/**
 * Shift+click range selection. `order` is the displayed order of ids (after sorting and filtering).
 * Returns `pick(id, shift, on)`: the ids to set to `on`: just `id`, or the whole range back to the
 * last clicked id when `shift` is held and that anchor is still displayed. Every pick moves the anchor.
 */
export function useRangeSelect(order: readonly string[]) {
  const anchor = useRef<string | null>(null);
  return (id: string, shift: boolean): string[] => {
    const from = shift && anchor.current !== null ? order.indexOf(anchor.current) : -1;
    const to = order.indexOf(id);
    anchor.current = id;
    if (from < 0 || to < 0) return [id];
    return order.slice(Math.min(from, to), Math.max(from, to) + 1);
  };
}
