import type { InvRow } from "./inventoryRows";
import { safeToList, type SafetyContext } from "./safetyRules";

/**
 * Copies of each mod row that can be sold, by row key.
 *
 * Every mod keeps one copy at mod level, whatever its mastery: the fitted one when a row is equipped (the
 * equipped floor already holds it), else one copy of the highest rank. Other floors (lock, spare, pinned goal,
 * set keep, ...) apply per row through `safeToList` and win when they keep more. The per-row `lastCopy` floor
 * is ignored: it only fires for masterable items and would keep a copy in every rank row.
 */
export function modSellable(rows: readonly InvRow[], context: SafetyContext): Map<string, number> {
  const byMod = new Map<string, { row: InvRow; total: number; kept: number }[]>();
  for (const row of rows) {
    if (row.item.inventoryGroup !== "mods") continue;
    const verdict = safeToList(row.item, context);
    let kept = 0;
    for (const r of verdict.reservations) if (r.rule !== "lastCopy") kept = Math.max(kept, r.quantity);
    const entry = { row, total: verdict.total, kept: Math.min(verdict.total, kept) };
    const list = byMod.get(row.item.internalName);
    if (list) list.push(entry);
    else byMod.set(row.item.internalName, [entry]);
  }
  const out = new Map<string, number>();
  for (const list of byMod.values()) {
    if (list.every((e) => e.kept === 0)) {
      const top = list.reduce((a, b) => ((b.row.item.rank || 0) > (a.row.item.rank || 0) ? b : a));
      top.kept = Math.min(1, top.total);
    }
    for (const e of list) out.set(e.row.key, e.total - e.kept);
  }
  return out;
}
