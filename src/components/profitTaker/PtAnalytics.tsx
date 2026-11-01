import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Translator } from "../../lib/i18n";
import type { PtRunRecord } from "../../lib/ipc";
import { PT_METRICS, PT_METRIC_KEYS, formatPtLength, ptMetricSummary, ptMetricValue, ptSquadSize, type PtMetric } from "../../lib/profitTakerStats";
import { Button, Chip } from "../ui";
import PtMetricCards from "./PtMetricCards";

const COLORS: Record<PtMetric, string> = {
  total: "var(--info)",
  flight: "var(--warning)",
  shield: "var(--accent)",
  leg: "var(--success)",
  body: "var(--danger)",
  pylon: "var(--text-secondary)",
};
const HEIGHT = 280;

const dateLabel = (time: number, locale: string) =>
  new Date(time).toLocaleString(locale, { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

interface Props {
  runs: PtRunRecord[];
  onSelect: (id: string) => void;
  tr: Translator;
  locale: string;
}

export default function PtAnalytics({ runs, onSelect, tr, locale }: Props) {
  const summary = ptMetricSummary(runs);
  const [enabled, setEnabled] = useState<PtMetric[]>([...PT_METRICS]);
  const [selectedId, setSelectedId] = useState("");
  const [width, setWidth] = useState(900);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = box.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    // Deferred to a frame so the resize it causes is not reported inside the observer's own delivery loop.
    let frame = 0;
    const ro = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => setWidth(el.clientWidth));
    });
    ro.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      ro.disconnect();
    };
  }, [summary.eligible.length]);

  const selected = summary.eligible.find((r) => r.id === selectedId) ?? null;
  const squads = summary.eligible.map(ptSquadSize);
  const mixed = new Set(squads.filter((s) => s !== null)).size > 1;
  const unknownSquad = squads.includes(null);

  const plotWidth = Math.max(300, width);
  const first = summary.eligible[0]?.startedAt ?? 0;
  const last = summary.eligible[summary.eligible.length - 1]?.startedAt ?? first;
  const max = summary.eligible.reduce((hi, run) => enabled.reduce((v, m) => Math.max(v, ptMetricValue(run, m)), hi), 1) * 1.1;
  const x = (run: PtRunRecord) => (last > first ? 48 + ((run.startedAt - first) / (last - first)) * (plotWidth - 70) : plotWidth / 2);
  const y = (value: number) => HEIGHT - 36 - (value / max) * (HEIGHT - 60);

  const toggle = (metric: PtMetric) => setEnabled(enabled.includes(metric) ? enabled.filter((m) => m !== metric) : PT_METRICS.filter((m) => m === metric || enabled.includes(m)));
  const selectKey = (e: KeyboardEvent, run: PtRunRecord) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(run.id);
    }
  };

  return (
    <div className="flex min-w-0 flex-col gap-4" data-pt-analytics>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p data-pt-analytics-scope data-pt-eligible-count={summary.eligible.length} data-pt-excluded-count={summary.excluded} className="m-0 text-sm text-text-secondary">
          {tr("pt.analytics.scope", { eligible: summary.eligible.length, excluded: summary.excluded })}
        </p>
        {mixed && (
          <span data-pt-mixed-squads className="rounded border border-warning/40 px-2 py-1 text-xs text-warning">
            {tr("pt.combinedSquads")}
          </span>
        )}
        {unknownSquad && (
          <span data-pt-unknown-squad className="rounded border border-warning/40 px-2 py-1 text-xs text-warning">
            {tr("pt.unknownSquad")}
          </span>
        )}
      </div>
      <p data-pt-recorded-roster className="m-0 text-xs text-text-muted">
        {tr("pt.recordedRosterHint")}
      </p>
      {summary.eligible.length === 0 ? (
        <p data-pt-analytics-empty className="m-0 rounded-xl border border-border bg-bg-surface p-10 text-center text-sm text-text-muted">
          {tr("pt.analytics.empty")}
        </p>
      ) : (
        <>
          <h3 className="m-0 text-sm font-semibold text-text-secondary">{tr("arbi.vitus.scenario.average")}</h3>
          <PtMetricCards
            tr={tr}
            mode="mean"
            rows={summary.metrics.map((row) => ({
              metric: row.metric,
              value: row.mean,
              detail: { text: `${tr("common.median")}: ${formatPtLength(row.median)} | ${tr("pt.analytics.fastestFiltered")}: ${formatPtLength(row.best)}`, good: false },
            }))}
          />
          <section className="ui-card min-w-0 p-4">
            <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
              <h3 className="m-0 text-md font-semibold text-text-primary">{tr("pt.analytics.trend")}</h3>
              <div className="flex flex-wrap gap-1.5">
                {PT_METRICS.map((metric) => (
                  <Chip key={metric} data-pt-chart-metric={metric} pressed={enabled.includes(metric)} onClick={() => toggle(metric)}>
                    <span className="size-2 rounded-full" style={{ background: COLORS[metric] }} />
                    {tr(PT_METRIC_KEYS[metric])}
                  </Chip>
                ))}
              </div>
            </div>
            <span className="text-xs text-text-muted">{tr("pt.analytics.seconds")}</span>
            <div ref={box} className="min-w-0 overflow-x-auto">
              <svg viewBox={`0 0 ${plotWidth} ${HEIGHT}`} className="block w-full min-w-[300px]" role="group" aria-label={tr("pt.analytics.trend")}>
                {[0, 1, 2, 3, 4].map((tick) => {
                  const value = (max * tick) / 4;
                  return (
                    <g key={tick}>
                      <line x1="48" x2={plotWidth - 22} y1={y(value)} y2={y(value)} stroke="var(--border)" strokeWidth="1" />
                      <text x="42" y={y(value) + 3} textAnchor="end" fill="var(--text-muted)" fontSize="10">
                        {value.toFixed(value < 10 ? 1 : 0)}
                      </text>
                    </g>
                  );
                })}
                {enabled.map((metric) => (
                  <polyline key={metric} data-pt-chart-series={metric} points={summary.eligible.map((run) => `${x(run)},${y(ptMetricValue(run, metric))}`).join(" ")} fill="none" stroke={COLORS[metric]} strokeWidth="1.8" />
                ))}
                {summary.eligible.map((run) => (
                  <g
                    key={run.id}
                    data-pt-chart-run={run.id}
                    role="button"
                    tabIndex={enabled.length ? 0 : -1}
                    aria-label={`${dateLabel(run.startedAt, locale)}: ${formatPtLength(run.durationSec)}`}
                    className="cursor-pointer outline-none"
                    onFocus={() => setSelectedId(run.id)}
                    onMouseEnter={() => setSelectedId(run.id)}
                    onClick={() => onSelect(run.id)}
                    onKeyDown={(e) => selectKey(e, run)}
                  >
                    {enabled.map((metric) => (
                      <circle key={metric} data-pt-chart-point={metric} cx={x(run)} cy={y(ptMetricValue(run, metric))} r={selectedId === run.id ? 4.5 : 3} fill={COLORS[metric]} stroke={selectedId === run.id ? "var(--text-primary)" : "var(--bg-surface)"} strokeWidth="1.5">
                        <title>
                          {tr(PT_METRIC_KEYS[metric])}: {formatPtLength(ptMetricValue(run, metric))}
                        </title>
                      </circle>
                    ))}
                  </g>
                ))}
                <text x="48" y={HEIGHT - 10} fill="var(--text-muted)" fontSize="10">
                  {dateLabel(summary.eligible[0].startedAt, locale)}
                </text>
                {summary.eligible.length > 1 && (
                  <text x={plotWidth - 22} y={HEIGHT - 10} textAnchor="end" fill="var(--text-muted)" fontSize="10">
                    {dateLabel(summary.eligible[summary.eligible.length - 1].startedAt, locale)}
                  </text>
                )}
              </svg>
            </div>
            {selected ? (
              <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-border pt-3 text-xs">
                <span className="font-semibold text-text-primary">{dateLabel(selected.startedAt, locale)}</span>
                {enabled.map((metric) => (
                  <span key={metric} className="text-text-muted">
                    {tr(PT_METRIC_KEYS[metric])} <strong className="font-mono text-text-secondary">{formatPtLength(ptMetricValue(selected, metric))}</strong>
                  </span>
                ))}
                <Button size="sm" className="ml-auto" onClick={() => onSelect(selected.id)}>
                  {tr("pt.analytics.openRun")}
                </Button>
              </div>
            ) : (
              <p className="mb-0 mt-3 text-xs text-text-muted">{tr("pt.analytics.selectRun")}</p>
            )}
          </section>
        </>
      )}
    </div>
  );
}
