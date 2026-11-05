import type { ParsedItem } from "../../types/inventory";
import { selectionKeyFor } from "../tradeWorkbench/queueModel";
import { safeToList, type SafetyContext, type SafetyReservation, type SafetyVerdict } from "./safetyRules";

/** Mods and arcanes: ranked upgrades that stack by copy and are judged together, whatever their rank row. */
export const isUpgrade = (item: { inventoryGroup?: string }): boolean => item.inventoryGroup === "mods" || item.inventoryGroup === "arcanes";

/**
 * Verdicts of every mod and arcane row, by row key.
 *
 * Every upgrade keeps one copy at upgrade level, whatever its mastery: the fitted one when a row is equipped (the
 * equipped floor already holds it), else one copy of the highest rank. Other floors (lock, spare, pinned goal,
 * set keep, ...) apply per row through `safeToList` and win when they keep more. The per-row `lastCopy` floor
 * is ignored: it only fires for masterable items and would keep a copy in every rank row.
 */
export function upgradeVerdicts(items: readonly ParsedItem[], context: SafetyContext): Map<string, SafetyVerdict> {
  const byName = new Map<string, { item: ParsedItem; verdict: SafetyVerdict; kept: number }[]>();
  for (const item of items) {
    if (!isUpgrade(item)) continue;
    const verdict = safeToList(item, context);
    let kept = 0;
    for (const r of verdict.reservations) if (r.rule !== "lastCopy") kept = Math.max(kept, r.quantity);
    const entry = { item, verdict, kept: Math.min(verdict.total, kept) };
    const list = byName.get(item.internalName);
    if (list) list.push(entry);
    else byName.set(item.internalName, [entry]);
  }
  const out = new Map<string, SafetyVerdict>();
  for (const list of byName.values()) {
    let added: SafetyReservation | null = null;
    let top: (typeof list)[number] | null = null;
    if (list.every((e) => e.kept === 0)) {
      top = list.reduce((a, b) => ((b.item.rank || 0) > (a.item.rank || 0) ? b : a));
      top.kept = Math.min(1, top.verdict.total);
      added = top.verdict.reservations.find((r) => r.rule === "lastCopy") ?? {
        rule: "lastCopy",
        quantity: 1,
        binding: true,
        reasonKey: "inventory.safety.reason.lastCopy",
      };
    }
    for (const e of list) {
      const reservations = e.verdict.reservations.filter((r) => r.rule !== "lastCopy").map((r) => ({ ...r, binding: r.quantity === e.kept }));
      if (e === top && added && e.kept > 0) reservations.push({ ...added, binding: true });
      out.set(selectionKeyFor(e.item), { total: e.verdict.total, reserved: e.kept, safe: e.verdict.total - e.kept, reservations });
    }
  }
  return out;
}

/** The verdict every sell surface uses: the upgrade rule for mods and arcanes, `safeToList` for the rest. */
export function verdictResolver(items: readonly ParsedItem[], context: SafetyContext): (item: ParsedItem) => SafetyVerdict {
  const upgrades = upgradeVerdicts(items, context);
  return (item) => (isUpgrade(item) && upgrades.get(selectionKeyFor(item))) || safeToList(item, context);
}

/** Sellable copies of a row and the main rule that keeps the others. */
export interface Sellable {
  count: number;
  reasonKey?: SafetyReservation["reasonKey"];
  params?: Readonly<Record<string, string | number>>;
}

export function toSellable(verdict: SafetyVerdict): Sellable {
  const why = verdict.reserved > 0 ? verdict.reservations.find((r) => r.binding && r.quantity > 0) : undefined;
  return { count: verdict.safe, reasonKey: why?.reasonKey, params: why?.params ?? { count: verdict.reserved } };
}
