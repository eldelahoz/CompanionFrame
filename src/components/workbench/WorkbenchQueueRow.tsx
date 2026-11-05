import type { MessageKey, Translator } from "../../lib/i18n";
import type { WorkbenchDampingReason } from "../../lib/tradeWorkbench/pricingStrategies";
import { effectivePrice, rowNeedsOverride, rowWarnings, type WorkbenchQueueRow as QueueRow, type WorkbenchQueueWarning } from "../../lib/tradeWorkbench/queueModel";
import { shiftOf } from "../../lib/useRangeSelect";

interface Props {
  row: QueueRow;
  /** Set-keep only applies to assembled-set rows. */
  canSetKeep: boolean;
  isLocked: boolean;
  isSetKept: boolean;
  spareOverride: number | null;
  tr: Translator;
  onToggleSelect: (row: QueueRow, shift: boolean) => void;
  onQuantity: (row: QueueRow, quantity: number) => void;
  onManualPrice: (row: QueueRow, price: number | null) => void;
  onOverride: (row: QueueRow) => void;
  onToggleLock: (row: QueueRow) => void;
  onToggleSetKeep: (row: QueueRow) => void;
  onSpare: (row: QueueRow, spare: number | null) => void;
}

const WARNING_KEYS: Record<WorkbenchQueueWarning, MessageKey> = {
  "no-listing-data": "workbench.warning.no-listing-data",
  "low-liquidity": "workbench.warning.low-liquidity",
  "no-price": "workbench.warning.no-price",
  "override-needed": "workbench.warning.override-needed",
  "fully-protected": "workbench.warning.fully-protected",
};

const DAMPING_KEYS: Record<WorkbenchDampingReason, MessageKey> = {
  depth: "workbench.damping.depth",
  "max-drop": "workbench.damping.max-drop",
};

const FIELD = "rounded-md border border-border bg-surface-input px-2 py-1 text-right text-sm text-text-primary outline-none focus:border-accent/70";

const numberOf = (el: HTMLInputElement): number | null => {
  const v = el.value.trim();
  if (!v) return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
};

export default function WorkbenchQueueRow({ row, canSetKeep, isLocked, isSetKept, spareOverride, tr, onToggleSelect, onQuantity, onManualPrice, onOverride, onToggleLock, onToggleSetKeep, onSpare }: Props) {
  const warnings = rowWarnings(row);
  const needsOverride = rowNeedsOverride(row);
  const price = effectivePrice(row);
  const reservationTitle = row.verdict.reservations
    .map((r) => {
      const count = r.params?.count;
      return `${tr(r.reasonKey as MessageKey, count != null ? { count } : {})} (${r.quantity})${r.binding ? " *" : ""}`;
    })
    .join("\n");

  return (
    <div className="grid grid-cols-[auto_1fr_auto_auto_auto_auto_auto] items-center gap-3 rounded-md border border-border/60 bg-surface-card px-3 py-2 text-sm" data-workbench-row={row.rowId}>
      <input type="checkbox" checked={row.selected} data-workbench-select aria-label={row.itemName} onChange={(e) => onToggleSelect(row, shiftOf(e))} />

      <div className="min-w-0">
        <span className="truncate font-medium">{row.item.displayName ?? row.itemName}</span>
        {row.rank != null && <span className="ml-1 opacity-70">R{row.rank}</span>}
        {row.existingOrder && <span className="ml-2 text-xs opacity-70">{tr("workbench.row.listed", { price: row.existingOrder.platinum })}</span>}
        {warnings.length > 0 && <div className="text-xs text-warning">{warnings.map((w) => tr(WARNING_KEYS[w])).join(", ")}</div>}
      </div>

      <div className="text-right" title={reservationTitle}>
        <span>{row.verdict.safe}</span>
        <span className="opacity-60">/{row.verdict.total}</span>
        {row.verdict.reserved > 0 && <span className="ml-1 rounded bg-warning/20 px-1.5 py-0.5 text-xs text-warning">{tr("workbench.row.reserved", { count: row.verdict.reserved })}</span>}
      </div>

      <div className="flex items-center gap-2">
        <input
          key={`q${row.quantity}`}
          className={`${FIELD} w-16`}
          type="number"
          min={0}
          max={row.verdict.total}
          defaultValue={row.quantity}
          data-workbench-qty
          aria-label={tr("common.quantity")}
          onBlur={(e) => onQuantity(row, numberOf(e.currentTarget) ?? 0)}
        />
        {needsOverride && (
          <label className="flex items-center gap-1 text-xs text-danger" title={reservationTitle}>
            <input type="checkbox" checked={row.overrideAcknowledged} onChange={() => onOverride(row)} />
            {tr("workbench.row.override")}
          </label>
        )}
      </div>

      <div className="text-right text-xs opacity-80">
        {row.market ? (
          <>
            <div>{tr("workbench.row.lowest", { price: row.market.lowestSell ?? 0 })}</div>
            <div>
              {tr("workbench.row.liquidity", { count: row.market.activeSellers })}
              {row.market.spread != null && `, ${tr("workbench.row.spread", { spread: row.market.spread })}`}
            </div>
          </>
        ) : (
          <span className="opacity-50">{tr("workbench.row.noMarketData")}</span>
        )}
      </div>

      <div className="flex items-center gap-1">
        {row.suggestion?.price != null && (
          <span className="text-xs opacity-70" title={JSON.stringify(row.suggestion.inputs)}>
            {row.suggestion.price}p ({Math.round(row.suggestion.confidence * 100)}%)
            {row.suggestion.damping && <span className="text-warning"> {tr(DAMPING_KEYS[row.suggestion.damping.reason])}</span>}
          </span>
        )}
        <input
          key={`p${row.manualPrice ?? ""}`}
          className={`${FIELD} w-20`}
          type="number"
          min={1}
          placeholder={row.suggestion?.price != null ? String(row.suggestion.price) : ""}
          defaultValue={row.manualPrice ?? ""}
          data-workbench-price
          aria-label={tr("common.price")}
          onBlur={(e) => onManualPrice(row, numberOf(e.currentTarget))}
        />
        <span className="text-xs opacity-60">{price != null ? `${price}p` : "-"}</span>
      </div>

      <div className="flex items-center gap-1 text-xs">
        <button type="button" className={`rounded-sm border border-border px-2 py-1 ${isLocked ? "bg-danger/25 text-danger" : "bg-surface-hover"}`} title={tr("workbench.safety.lockHint")} onClick={() => onToggleLock(row)}>
          {tr(isLocked ? "workbench.safety.locked" : "workbench.safety.lock")}
        </button>
        {/* No placeholder: it clips in the field, so the label lives in the tooltip and accessible name. */}
        <input
          key={`s${spareOverride ?? ""}`}
          className={`${FIELD} w-16`}
          type="number"
          min={0}
          aria-label={tr("workbench.safety.sparePlaceholder")}
          title={tr("workbench.safety.spareHint")}
          defaultValue={spareOverride ?? ""}
          onBlur={(e) => onSpare(row, numberOf(e.currentTarget))}
        />
        {canSetKeep && (
          <button type="button" className={`rounded-sm border border-border px-2 py-1 ${isSetKept ? "bg-warning/25 text-warning" : "bg-surface-hover"}`} title={tr("workbench.safety.setKeepHint")} onClick={() => onToggleSetKeep(row)}>
            {tr("workbench.safety.setKeep")}
          </button>
        )}
      </div>
    </div>
  );
}
