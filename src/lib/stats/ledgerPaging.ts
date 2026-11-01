import { ledgerGetRange, type LedgerQuery } from "../ipc";
import type { TradeEvent } from "../../types/stats";

// Matches the backend's per-page ceiling.
export const LEDGER_QUERY_MAX_LIMIT = 200;

/** Null means the caller's load is no longer current. Pages by offset: the ledger is newest-first. */
export async function pageLedgerRange(
  query: LedgerQuery,
  maxRows: number,
  isCurrent: () => boolean = () => true,
): Promise<{ events: TradeEvent[]; total: number } | null> {
  const collected: TradeEvent[] = [];
  let total = 0;
  while (collected.length < maxRows) {
    const limit = Math.min(LEDGER_QUERY_MAX_LIMIT, maxRows - collected.length);
    const page = await ledgerGetRange({ ...query, offset: collected.length, limit });
    if (!isCurrent()) return null;
    total = page.total;
    collected.push(...page.events);
    if (page.events.length < limit) break;
  }
  return { events: collected, total };
}
