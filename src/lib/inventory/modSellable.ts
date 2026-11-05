import type { InvRow } from "./inventoryRows";
import { safeToList, type SafetyContext } from "./safetyRules";

/**
 * Copies of each mod row that can be sold, by row key.
 *
 * `safeToList` runs per row, so its last-copy floor would keep one copy in every rank row. The last copy
 * belongs to the whole mod: it is dropped from each row verdict and re-applied once, on the highest rank,
 * and only when no other floor (equipped, lock, spare, ...) already keeps a copy somewhere in the mod.
 */
export function modSellable(rows: readonly InvRow[], context: SafetyContext): Map<string, number> {
  const byMod = new Map<string, { row: InvRow; total: number; kept: number; last: boolean }[]>();
  for (const row of rows) {
    if (row.item.inventoryGroup !== "mods") continue;
    const verdict = safeToList(row.item, context);
    let kept = 0;
    for (const r of verdict.reservations) if (r.rule !== "lastCopy") kept = Math.max(kept, r.quantity);
    const entry = { row, total: verdict.total, kept: Math.min(verdict.total, kept), last: verdict.reservations.some((r) => r.rule === "lastCopy") };
    const list = byMod.get(row.item.internalName);
    if (list) list.push(entry);
    else byMod.set(row.item.internalName, [entry]);
  }
  const out = new Map<string, number>();
  for (const list of byMod.values()) {
    if (list.some((e) => e.last) && list.every((e) => e.kept === 0)) {
      const top = list.reduce((a, b) => ((b.row.item.rank || 0) > (a.row.item.rank || 0) ? b : a));
      top.kept = Math.min(1, top.total);
    }
    for (const e of list) out.set(e.row.key, e.total - e.kept);
  }
  return out;
}
