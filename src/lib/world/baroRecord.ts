import { BARO_HISTORY_MAX_ITEMS, BARO_HISTORY_MAX_VISITS, normalizeBaroHistory, type BaroHistory } from "../../../config/shared/baroHistory";
import { storeItemPath } from "../../../config/shared/itemPath";
import type { VaultTrader } from "./baroPlanner";
import { activeWindow } from "../format";

const cost = (v: number | null | undefined) => (typeof v === "number" && Number.isSafeInteger(v) && v >= 0 ? v : null);

/**
 * Folds the current Baro visit into the locally kept history (there is no backend archive to download).
 * Returns the same object when nothing changed; a visit is recorded once, the first time it is seen active.
 */
export function recordBaroVisit(history: BaroHistory | null, trader: VaultTrader | null | undefined, now: number): BaroHistory | null {
  if (!trader || !activeWindow(trader.activation, trader.expiry, now)) return history;
  const activation = Date.parse(trader.activation ?? "");
  const expiry = Date.parse(trader.expiry ?? "");
  const id = `v${activation}`;
  if (history?.visits.some((v) => v.id === id)) return history;
  const items = new Map<string, { uniqueName: string; ducats: number | null; credits: number | null }>();
  for (const offer of trader.inventory ?? []) {
    if (typeof offer?.uniqueName !== "string" || !offer.uniqueName.startsWith("/Lotus/") || offer.uniqueName.length > 512) continue;
    const uniqueName = storeItemPath(offer.uniqueName);
    if (!items.has(uniqueName) && items.size < 500) items.set(uniqueName, { uniqueName, ducats: cost(offer.ducats), credits: cost(offer.credits) });
  }
  if (items.size === 0) return history;
  const visits = [...(history?.visits ?? []), { id, activation, expiry, node: (trader.location ?? "").slice(0, 128), items: [...items.values()] }]
    .sort((a, b) => b.activation - a.activation)
    .slice(0, BARO_HISTORY_MAX_VISITS);
  const lastSeen = new Map((history?.lastSeen ?? []).map((e) => [e.uniqueName, e]));
  for (const item of items.values()) {
    const prev = lastSeen.get(item.uniqueName);
    if (!prev || prev.lastSeen <= activation) lastSeen.set(item.uniqueName, { ...item, visitId: id, lastSeen: activation });
  }
  return normalizeBaroHistory({
    version: 1,
    updatedAt: now,
    coverageStart: Math.min(history?.coverageStart ?? activation, activation),
    visits,
    lastSeen: [...lastSeen.values()].slice(0, BARO_HISTORY_MAX_ITEMS),
  });
}
