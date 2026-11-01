import type { Translator } from "../../lib/i18n";
import { FLOW_BAR, FLOW_HEIGHT, FLOW_SLOT, flowAxis, flowBarHeight, flowBarX } from "../../lib/stats/chartData";
import { parseDateKey, type DayFlow, type MonthFlow, type PlatFlow } from "../../lib/stats/tradeAnalytics";
import { Card, CardHeader, Readout } from "../ui";

const fmt = (n: number, locale: string) => Math.round(n).toLocaleString(locale);

const monthLabel = (month: string, loc: string): string => {
  const p = parseDateKey(month);
  if (!p) return month;
  const short = new Date(p.year, p.month - 1, 1).toLocaleDateString(loc, { month: "short" });
  return p.month === 1 ? `${short} ${String(p.year).slice(2)}` : short;
};

const dayLabel = (day: string, loc: string): string => {
  const p = parseDateKey(day);
  return p ? new Date(p.year, p.month - 1, p.day).toLocaleDateString(loc, { month: "short", day: "numeric" }) : day;
};

const Axis = ({ width, zeroY }: { width: number; zeroY: number }) => (
  <line x1="0" y1={zeroY} x2={width} y2={zeroY} stroke="var(--border)" strokeWidth="0.5" />
);

export function MonthlyChart({ rows, tr, locale }: { rows: MonthFlow[]; tr: Translator; locale: string }) {
  const up = rows.reduce((m, r) => Math.max(m, r.net), 0);
  const down = rows.reduce((m, r) => Math.max(m, -r.net), 0);
  const { width, zeroY, span } = flowAxis(rows.length, up, down);
  const step = Math.max(1, Math.ceil(rows.length / 12));
  return (
    <Card className="min-w-0" data-analysis-monthly>
      <CardHeader title={tr("analysis.netByMonth")} />
      <div className="ui-card-body flex min-w-0 flex-col gap-2">
        {span <= 0 ? (
          <p className="m-0 py-6 text-center text-sm text-text-muted">{tr("stats.noDataTimeframe")}</p>
        ) : (
          <>
            <svg className="block h-32 w-full" viewBox={`0 0 ${width} ${FLOW_HEIGHT}`} preserveAspectRatio="none" aria-hidden="true">
              <Axis width={width} zeroY={zeroY} />
              {rows.map((row, i) => {
                const h = flowBarHeight(row.net, span);
                const title = tr("analysis.flowDetail", {
                  label: monthLabel(row.month, locale),
                  in: fmt(row.platIn, locale),
                  out: fmt(row.platOut, locale),
                  net: fmt(row.net, locale),
                });
                return (
                  <g key={row.month}>
                    <rect x={flowBarX(i)} y={row.net >= 0 ? zeroY - h : zeroY} width={FLOW_BAR} height={h} className={row.net >= 0 ? "fill-success" : "fill-danger"} />
                    {/* A near-zero month draws a sliver nobody can hover, so the whole column carries the tooltip. */}
                    <rect x={i * FLOW_SLOT} y="0" width={FLOW_SLOT} height={FLOW_HEIGHT} fill="none" pointerEvents="all">
                      <title>{title}</title>
                    </rect>
                  </g>
                );
              })}
            </svg>
            <div className="flex min-w-0">
              {rows.map((row, i) => (
                <span key={row.month} className="min-w-0 flex-1 truncate text-center text-xs text-text-muted">
                  {i % step === 0 ? monthLabel(row.month, locale) : ""}
                </span>
              ))}
            </div>
          </>
        )}
      </div>
    </Card>
  );
}

export function RecentDays({ days, today, loading, failed, tr, locale }: { days: DayFlow[]; today: PlatFlow; loading: boolean; failed: boolean; tr: Translator; locale: string }) {
  const up = days.reduce((m, d) => Math.max(m, d.platIn), 0);
  const down = days.reduce((m, d) => Math.max(m, d.platOut), 0);
  const { width, zeroY, span } = flowAxis(days.length, up, down);
  return (
    <Card className="min-w-0" data-analysis-recent-days>
      <CardHeader title={tr("analysis.lastDays", { count: days.length })} meta={tr("stats.today")} />
      <div className="ui-card-body flex min-w-0 flex-col gap-2">
        {loading || failed ? (
          <p className={`m-0 py-6 text-center text-sm ${failed ? "text-danger" : "text-text-muted"}`}>{failed ? tr("analysis.loadFailed") : tr("common.loading")}</p>
        ) : (
          <>
            <div className="flex flex-wrap items-baseline justify-end gap-x-4 gap-y-1" data-analysis-today>
              <span className="text-xs text-success">{tr("analysis.salesCount", { count: today.sales })}</span>
              <span className="text-xs text-danger">{tr("analysis.purchasesCount", { count: today.purchases })}</span>
              <Readout currency="platinum" label={tr("analysis.colProfit")} value={Math.round(today.net)} locale={locale} size="lg" sign />
            </div>
            {span <= 0 ? (
              <p className="m-0 py-6 text-center text-sm text-text-muted">{tr("stats.noDataTimeframe")}</p>
            ) : (
              <>
                <svg className="block h-24 w-full" viewBox={`0 0 ${width} ${FLOW_HEIGHT}`} preserveAspectRatio="none" aria-hidden="true">
                  <Axis width={width} zeroY={zeroY} />
                  {days.map((day, i) => {
                    // A day with no flow on one side draws nothing there, not a sliver.
                    const inH = day.platIn > 0 ? flowBarHeight(day.platIn, span) : 0;
                    const outH = day.platOut > 0 ? flowBarHeight(day.platOut, span) : 0;
                    const title = tr("analysis.flowDetail", { label: dayLabel(day.day, locale), in: fmt(day.platIn, locale), out: fmt(day.platOut, locale), net: fmt(day.net, locale) });
                    return (
                      <g key={day.day}>
                        {inH > 0 && (
                          <rect x={flowBarX(i)} y={zeroY - inH} width={FLOW_BAR} height={inH} className="fill-success">
                            <title>{title}</title>
                          </rect>
                        )}
                        {outH > 0 && (
                          <rect x={flowBarX(i)} y={zeroY} width={FLOW_BAR} height={outH} className="fill-danger">
                            <title>{title}</title>
                          </rect>
                        )}
                      </g>
                    );
                  })}
                </svg>
                <div className="flex min-w-0">
                  {days.map((day, i) => (
                    <span key={day.day} className="min-w-0 flex-1 truncate text-center text-xs text-text-muted">
                      {i % 2 === 0 ? dayLabel(day.day, locale) : ""}
                    </span>
                  ))}
                </div>
              </>
            )}
          </>
        )}
      </div>
    </Card>
  );
}
