import { WandSparkles } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import { Button } from "../ui";

export interface BulkRun {
  action: "hide" | "show" | "delete";
  done: number;
  total: number;
}

interface Props {
  count: number;
  total: number;
  /** The sell tab only. */
  canReprice: boolean;
  busy: boolean;
  /** The running bulk action, if any. */
  run: BulkRun | null;
  onAll: () => void;
  onHide: () => void;
  onShow: () => void;
  onReprice: () => void;
  onDelete: () => void;
  onCancel: () => void;
  tr: Translator;
}

/** Floats at the bottom centre while orders are selected. */
export default function MarketBulkBar({ count, total, canReprice, busy: rowBusy, run, onAll, onHide, onShow, onReprice, onDelete, onCancel, tr }: Props) {
  const busy = rowBusy || run !== null;
  const progress = { hide: "market.bulkHiding", show: "market.bulkShowing", delete: "market.bulkDeleting" } as const;
  return (
    <div
      className="fixed bottom-4 left-1/2 z-40 flex max-w-[calc(100%-2rem)] -translate-x-1/2 flex-wrap items-center gap-2 rounded-lg border border-accent bg-bg-raised px-3 py-2 shadow-lg"
      role="region"
      aria-label={tr("market.bulkBar")}
      data-market-bulk-bar
    >
      <span className="font-semibold tabular-nums">{run ? tr(progress[run.action], { done: run.done, total: run.total }) : tr("market.selectedOf", { count, total })}</span>
      <Button size="sm" disabled={busy || count === total} onClick={onAll}>
        {tr("common.all")}
      </Button>
      <Button size="sm" loading={run?.action === "hide"} disabled={busy} onClick={onHide}>
        {tr("market.bulkHide")}
      </Button>
      <Button size="sm" loading={run?.action === "show"} disabled={busy} onClick={onShow}>
        {tr("market.bulkShow")}
      </Button>
      {canReprice && (
        <Button size="sm" icon={<WandSparkles />} disabled={busy} onClick={onReprice}>
          {tr("market.repriceSelected")}
        </Button>
      )}
      <Button size="sm" variant="danger" loading={run?.action === "delete"} disabled={busy} onClick={onDelete}>
        {tr("common.delete")}
      </Button>
      <Button size="sm" variant="ghost" onClick={onCancel}>
        {tr(run ? "market.bulkStop" : "common.cancel")}
      </Button>
    </div>
  );
}
