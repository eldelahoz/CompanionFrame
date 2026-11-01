import type { MessageKey, Translator } from "./i18n";
import type { InventoryStatus } from "./ipc";

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
/** An import older than this is shown as stale. */
const STALE_MS = 3 * DAY;

export type PillTone = "success" | "warning" | "danger" | "muted";

/** "just now", "5m ago", "3h ago", "3d ago". */
export function ageLabel(at: number, now: number, tr: Translator): string {
  const age = Math.max(0, now - at);
  if (age < MIN) return tr("inventory.ago.now");
  if (age < HOUR) return tr("inventory.ago.m", { n: String(Math.floor(age / MIN)) });
  if (age < DAY) return tr("inventory.ago.h", { n: String(Math.floor(age / HOUR)) });
  return tr("inventory.ago.d", { n: String(Math.floor(age / DAY)) });
}

/** "Imported 3d ago, 5 trades, 2 est. picks", or the not-imported text. */
export function freshnessText(status: InventoryStatus | null, now: number, tr: Translator): string {
  if (!status?.found || status.importedAt == null) return tr("inventory.pill.none");
  const parts = [tr("inventory.pill.imported", { age: ageLabel(status.importedAt, now, tr) })];
  if (status.tradesApplied > 0) parts.push(tr(status.tradesApplied === 1 ? "inventory.deltas.trade" : "inventory.deltas.trades", { n: String(status.tradesApplied) }));
  if (status.picksApplied > 0) parts.push(tr("inventory.deltas.picks", { n: String(status.picksApplied) }));
  return parts.join(", ");
}

export function freshnessTone(status: InventoryStatus | null, now: number): PillTone {
  if (!status?.found || status.importedAt == null) return "muted";
  return status.gameSyncedSinceImport || now - status.importedAt > STALE_MS ? "warning" : "success";
}

/** The sentence under the pill/header when the game synced since the import, else null. */
export function staleHint(status: InventoryStatus | null): MessageKey | null {
  return status?.found && status.gameSyncedSinceImport ? "inventory.stale" : null;
}
