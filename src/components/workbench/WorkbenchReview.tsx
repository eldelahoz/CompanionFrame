import { useState } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { WorkbenchReviewClassification, WorkbenchReviewReport } from "../../lib/ipc";
import type { WorkbenchState } from "../../../config/shared/tradeWorkbenchTypes";

interface Props {
  state: WorkbenchState;
  report: WorkbenchReviewReport | null;
  busy: boolean;
  tr: Translator;
  onReconcile: () => void;
  onResolve: (resolutions: Array<{ intentId: string; classification: WorkbenchReviewClassification }>) => void;
  onResetCorruptJournal: () => void;
}

const CLASSIFICATIONS: readonly WorkbenchReviewClassification[] = ["confirmed", "failed", "unknown"];
const CLASS_KEYS: Record<WorkbenchReviewClassification, MessageKey> = {
  confirmed: "workbench.review.class.confirmed",
  failed: "workbench.review.class.failed",
  unknown: "workbench.review.class.unknown",
};
const CLASS_STYLE: Record<WorkbenchReviewClassification, string> = {
  confirmed: "bg-success-bg text-success",
  failed: "bg-danger-bg text-danger",
  unknown: "bg-warning-bg text-warning",
};

export default function WorkbenchReview({ state, report, busy, tr, onReconcile, onResolve, onResetCorruptJournal }: Props) {
  // The user may overrule a classification before confirming the repair.
  const [picks, setPicks] = useState<Record<string, WorkbenchReviewClassification>>({});
  const pickFor = (id: string, fallback: WorkbenchReviewClassification) => picks[id] ?? fallback;
  const rows = report?.rows ?? [];
  // "unknown" never settles an intent, so a row left on it stays open for the next reconcile.
  const resolutions = rows.map((r) => ({ intentId: r.intentId, classification: pickFor(r.intentId, r.classification) })).filter((r) => r.classification !== "unknown");

  return (
    <div className="rounded border border-warning-dim bg-warning-bg p-3 text-sm" data-workbench-review>
      <div className="mb-2 font-semibold text-warning">{tr("workbench.review.title")}</div>
      <p className="mb-2 opacity-80">{tr("workbench.review.explanation")}</p>

      {state.journalError && (
        <div className="mb-2 rounded bg-danger-bg p-2 text-danger">
          <div>{tr("workbench.review.journalError")}</div>
          <div className="text-xs opacity-80">{state.journalError}</div>
          <button type="button" className="mt-1 rounded bg-danger-bg px-2 py-0.5" onClick={onResetCorruptJournal}>
            {tr("workbench.review.resetJournal")}
          </button>
        </div>
      )}

      {state.unsettledCount > 0 && (
        <button type="button" className="rounded bg-surface-hover px-2 py-0.5" disabled={busy} onClick={onReconcile}>
          {tr("workbench.review.reconcile")}
        </button>
      )}

      {report && (
        <>
          {report.fetchError && (
            <div className="mt-2 text-danger">
              {tr("workbench.review.fetchError")}: {report.fetchError}
            </div>
          )}
          <div className="mt-2 space-y-1">
            {rows.map((row) => (
              <div key={row.intentId} className="flex items-center gap-2 border-b border-border-subtle py-1">
                <span className="min-w-0 flex-1 truncate">
                  {row.itemName}, {row.quantity}x @ {row.platinum}p ({row.mode})
                </span>
                <span className={`rounded px-1 text-xs ${CLASS_STYLE[row.classification]}`}>{tr(CLASS_KEYS[row.classification])}</span>
                <select
                  className="rounded bg-surface-input px-1 text-xs"
                  value={pickFor(row.intentId, row.classification)}
                  onChange={(e) => setPicks({ ...picks, [row.intentId]: e.target.value as WorkbenchReviewClassification })}
                >
                  {CLASSIFICATIONS.map((c) => (
                    <option key={c} value={c}>
                      {c === "unknown" ? tr("workbench.review.leaveOpen") : tr(CLASS_KEYS[c])}
                    </option>
                  ))}
                </select>
              </div>
            ))}
          </div>
          {rows.length > 0 && (
            // A failed fetch classified nothing, so resolving would settle rows on no evidence at all.
            <button type="button" className="mt-2 rounded bg-warning-bg px-2 py-0.5" disabled={busy || report.fetchError != null || resolutions.length === 0} data-workbench-resolve onClick={() => onResolve(resolutions)}>
              {tr("workbench.review.confirmResolutions")}
            </button>
          )}
        </>
      )}
    </div>
  );
}
