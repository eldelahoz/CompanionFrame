import { Footprints, Rocket, Shield, Swords, Timer, Zap, type LucideIcon } from "lucide-react";
import { createElement } from "react";
import { PT_METRIC_KEYS, formatPtLength, formatPtTime, type PtMetric } from "../../lib/profitTakerStats";
import type { Translator } from "../../lib/i18n";

export interface PtMetricRow {
  metric: PtMetric;
  value: number | null;
  detail?: { text: string; good: boolean } | null;
  delta?: { seconds: number; percent: number | null } | null;
}

const ICONS: Record<PtMetric, LucideIcon> = { total: Timer, flight: Rocket, shield: Shield, leg: Footprints, body: Swords, pylon: Zap };

export default function PtMetricCards({ rows, mode = "run", tr }: { rows: readonly PtMetricRow[]; mode?: "run" | "mean"; tr: Translator }) {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-3 xl:grid-cols-6" data-pt-metric-cards>
      {rows.map((row) => {
        const value = formatPtTime(row.value);
        const unit = formatPtLength(row.value) !== value;
        return (
          <section key={row.metric} className="ui-card min-w-0 p-4" data-pt-stat={row.metric}>
            <div className="mb-4 flex items-center gap-2.5">
              <span className="flex size-9 shrink-0 items-center justify-center rounded-[var(--radius-md)] bg-bg-raised text-text-secondary">
                {createElement(ICONS[row.metric], { "aria-hidden": true, className: "size-5" })}
              </span>
              <h3 className="m-0 text-sm font-semibold leading-tight text-text-secondary">{tr(PT_METRIC_KEYS[row.metric])}</h3>
            </div>
            <div className="whitespace-nowrap text-2xl font-semibold leading-none tabular-nums text-text-primary">
              <span data-pt-stat-value={row.metric} data-pt-mean={mode === "mean" ? row.metric : undefined}>
                {value}
              </span>
              {unit && <span className="ml-0.5 text-sm font-normal text-text-muted">s</span>}
            </div>
            {row.detail && <p className={`mb-0 mt-2 text-xs ${row.detail.good ? "text-success" : "text-text-muted"}`}>{row.detail.text}</p>}
            {row.delta && (
              <p data-pt-delta={row.metric} className={`mb-0 mt-2 text-xs font-semibold ${row.delta.seconds < 0 ? "text-success" : row.delta.seconds > 0 ? "text-warning" : "text-text-muted"}`}>
                {row.delta.seconds >= 0 ? "+" : ""}
                {row.delta.seconds.toFixed(3)}s
                {row.delta.percent !== null && ` (${row.delta.percent >= 0 ? "+" : ""}${row.delta.percent.toFixed(1)}%)`}
              </p>
            )}
          </section>
        );
      })}
    </div>
  );
}
