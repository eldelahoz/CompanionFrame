import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { SearchX, TriangleAlert } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import { Button, Card, Checkbox, Chip, EmptyState, Field, Skeleton } from "../ui";
import { wfmStatsHistory, type MarketStatPoint } from "../../lib/ipc";
import { buildChart, CHART_H, collectRanks, formatPlat, formatStamp, MARGIN, PRICE_BASE, selectRows, VOLUME_BASE, VOLUME_TOP, type Period } from "../../lib/market/statsChart";

interface Props {
  slug: string;
  tr: Translator;
  locale: string;
}

type Load = { state: "loading" } | { state: "error" } | { state: "ready"; points: MarketStatPoint[] };

const MIN_WIDTH = 480;
const FALLBACK_WIDTH = 900;
const PERIODS: ReadonlyArray<readonly [Period, "browse.days30" | "browse.days90" | "browse.latest"]> = [
  ["30days", "browse.days30"],
  ["90days", "browse.days90"],
  ["latest", "browse.latest"],
];

export default function MarketBrowseStats({ slug, tr, locale }: Props) {
  const [load, setLoad] = useState<Load>({ state: "loading" });
  const [period, setPeriod] = useState<Period>("90days");
  const [rank, setRank] = useState<number | null>(null);
  const [hover, setHover] = useState<number | null>(null);
  const [width, setWidth] = useState(0);
  const [show, setShow] = useState({ candles: false, median: true, movingAvg: true, avgPrice: false, donchian: true });
  const box = useRef<HTMLDivElement>(null);

  const fetchHistory = useCallback(() => {
    let live = true;
    setLoad({ state: "loading" });
    setHover(null);
    wfmStatsHistory(slug).then(
      (points) => live && setLoad({ state: "ready", points }),
      () => live && setLoad({ state: "error" }),
    );
    return () => {
      live = false;
    };
  }, [slug]);
  useEffect(() => fetchHistory(), [fetchHistory]);

  const points = load.state === "ready" ? load.points : null;
  const ranks = useMemo(() => (points ? collectRanks(points) : []), [points]);
  // A ranked item defaults to rank 0 (or its lowest), and a rank that vanished falls back the same way.
  const activeRank = ranks.length > 1 ? (rank != null && ranks.includes(rank) ? rank : ranks.includes(0) ? 0 : ranks[0]) : null;
  const rows = useMemo(() => (points ? selectRows(points, period, activeRank) : []), [points, period, activeRank]);

  // The svg is drawn at 1 user unit per CSS pixel, so only the width reflows.
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    setWidth(el.clientWidth);
    if (typeof ResizeObserver === "undefined") return;
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
  }, [load.state, rows.length > 1]);

  const chart = useMemo(() => (rows.length > 1 ? buildChart(rows, Math.max(MIN_WIDTH, width || FALLBACK_WIDTH), locale) : null), [rows, width, locale]);
  const hovered = chart && hover != null && hover < rows.length ? { row: rows[hover], marker: chart.markers[hover], bar: chart.bars[hover] } : null;

  const trackHover = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!chart) return;
    const b = e.currentTarget.getBoundingClientRect();
    if (b.width <= 0) return;
    const index = Math.round((((e.clientX - b.left) / b.width) * chart.width - chart.markers[0].x) / chart.step);
    setHover(Math.min(chart.markers.length - 1, Math.max(0, index)));
  };
  // Past the midpoint the card is anchored by its right edge so it flips to the other side of the crosshair.
  const tooltipStyle = hovered && chart ? (hovered.marker.x > chart.width / 2 ? { right: `calc(${(100 - (hovered.marker.x / chart.width) * 100).toFixed(2)}% + 12px)` } : { left: `calc(${((hovered.marker.x / chart.width) * 100).toFixed(2)}% + 12px)` }) : {};
  const toggle = (k: keyof typeof show) => setShow((s) => ({ ...s, [k]: !s[k] }));
  const num = (v: number) => formatPlat(v, locale);

  const checks: ReadonlyArray<readonly [keyof typeof show, string, string, string]> = [
    ["candles", tr("browse.candleChart"), "h-2.5 w-2 rounded-sm", "var(--success)"],
    ["median", tr("common.median"), "h-[3px] w-4 rounded-full", "var(--accent)"],
    ["movingAvg", tr("browse.movingAvg"), "h-[2px] w-4 rounded-full", "var(--info)"],
    ["avgPrice", tr("browse.avgPrice"), "h-[2px] w-4 rounded-full", "var(--danger)"],
    ["donchian", tr("browse.donchianChannel"), "h-2.5 w-4 rounded-sm", "color-mix(in oklab, var(--info) 25%, transparent)"],
  ];

  return (
    <div className="flex flex-col gap-4" data-market-stats>
      <div className="flex flex-wrap items-end gap-x-4 gap-y-3">
        <Field label={tr("browse.period")}>
          <div className="flex gap-1">
            {PERIODS.map(([key, text]) => (
              <Chip key={key} pressed={period === key} data-market-stats-period={key} onClick={() => (setPeriod(key), setHover(null))}>
                {tr(text)}
              </Chip>
            ))}
          </div>
        </Field>
        {ranks.length > 1 && (
          <Field label={tr("common.rank")}>
            <div className="flex gap-1">
              {ranks.map((value) => (
                <Chip key={value} pressed={activeRank === value} onClick={() => (setRank(value), setHover(null))}>
                  {tr("browse.rankValue", { value })}
                </Chip>
              ))}
            </div>
          </Field>
        )}
      </div>

      {load.state === "loading" ? (
        <Card className="flex flex-col gap-3 p-4" aria-busy="true" aria-label={tr("browse.loadingStats")}>
          <Skeleton height="10rem" />
        </Card>
      ) : load.state === "error" ? (
        <Card>
          <div role="alert">
            <EmptyState icon={<TriangleAlert />} title={tr("browse.statsFailed")} action={<Button size="sm" onClick={fetchHistory}>{tr("common.retry")}</Button>} />
          </div>
        </Card>
      ) : !chart ? (
        <Card>
          <EmptyState icon={<SearchX />} title={tr("browse.noStats")} />
        </Card>
      ) : (
        <>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-text-secondary">
            {checks.map(([key, text, cls, color]) => (
              <Checkbox key={key} checked={show[key]} onChange={() => toggle(key)} data-market-stats-toggle={key} label={<span className="inline-flex items-center gap-2"><span className={`inline-block ${cls}`} style={{ background: color }} />{text}</span>} />
            ))}
          </div>
          {/* min-w-0 + overflow-hidden: the fixed-width svg must never hold the layout open, or the chart could grow but never shrink back. */}
          <div className="ui-card min-w-0 overflow-hidden p-3">
            <div className="relative" ref={box} data-market-stats-points={rows.length}>
              <svg width={chart.width} height={CHART_H} viewBox={`0 0 ${chart.width} ${CHART_H}`} className="block" role="img" aria-label={tr("browse.chartAriaLabel")} onMouseMove={trackHover} onMouseLeave={() => setHover(null)}>
                <rect x="0" y="0" width={chart.width} height={CHART_H} fill="transparent" />
                <text x={MARGIN.left} y={MARGIN.top - 7} fontSize="10" fill="var(--text-muted)">
                  {tr("common.platinum")}
                </text>
                <text x={MARGIN.left} y={VOLUME_TOP - 7} fontSize="10" fill="var(--text-muted)">
                  {tr("browse.volume")}
                </text>
                {chart.priceTicks.map((t) => (
                  <g key={t.value}>
                    <line x1={MARGIN.left} y1={t.offset} x2={chart.width - MARGIN.right} y2={t.offset} stroke="rgba(255,255,255,0.06)" />
                    <text x={MARGIN.left - 8} y={t.offset} dy="4" fontSize="11" textAnchor="end" fill="var(--text-muted)">
                      {t.label}
                    </text>
                  </g>
                ))}
                {show.donchian && <path d={chart.band} fill="var(--info)" opacity="0.1" />}
                {show.candles && (
                  <g opacity="0.9" data-market-stats-candle-series>
                    {chart.candles.map((c, i) => {
                      const tone = c.rising ? "var(--success)" : "var(--danger)";
                      return (
                        <g key={i}>
                          <line x1={c.x + c.width / 2} y1={c.wickTop} x2={c.x + c.width / 2} y2={c.wickBottom} stroke={tone} />
                          <rect x={c.x} y={c.bodyY} width={c.width} height={c.bodyH} fill={tone} />
                        </g>
                      );
                    })}
                  </g>
                )}
                {show.median && <path d={chart.median} fill="none" stroke="var(--accent)" strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" />}
                {show.movingAvg && chart.movingAvg && <path d={chart.movingAvg} fill="none" stroke="var(--info)" strokeWidth="1.4" strokeLinejoin="round" strokeLinecap="round" />}
                {show.avgPrice && <path d={chart.avgPrice} fill="none" stroke="var(--danger)" strokeWidth="1.4" strokeLinejoin="round" strokeLinecap="round" />}
                {chart.volumeTicks.map((t) => (
                  <text key={t.value} x={MARGIN.left - 8} y={t.offset} dy="4" fontSize="11" textAnchor="end" fill="var(--text-muted)">
                    {t.label}
                  </text>
                ))}
                <g data-market-stats-volume-series>
                  {chart.bars.map((b, i) => (
                    <rect key={i} x={b.x} y={b.y} width={b.width} height={b.height} fill="var(--info)" opacity="0.65" />
                  ))}
                </g>
                {hovered && <rect x={hovered.bar.x} y={hovered.bar.y} width={hovered.bar.width} height={hovered.bar.height} fill="var(--accent)" />}
                <g stroke="rgba(255,255,255,0.16)" shapeRendering="crispEdges">
                  <line x1={MARGIN.left} y1={MARGIN.top} x2={MARGIN.left} y2={PRICE_BASE} />
                  <line x1={MARGIN.left} y1={PRICE_BASE} x2={chart.width - MARGIN.right} y2={PRICE_BASE} />
                  <line x1={MARGIN.left} y1={VOLUME_TOP} x2={MARGIN.left} y2={VOLUME_BASE} />
                  <line x1={MARGIN.left} y1={VOLUME_BASE} x2={chart.width - MARGIN.right} y2={VOLUME_BASE} />
                </g>
                <g data-market-stats-date-labels>
                  {chart.xLabels.map((l, i) => (
                    <text key={i} x={l.x} y={CHART_H - 7} fontSize="11" textAnchor={l.anchor} fill="var(--text-muted)">
                      {l.label}
                    </text>
                  ))}
                </g>
                {hovered && (
                  <>
                    <line x1={hovered.marker.x} y1={MARGIN.top} x2={hovered.marker.x} y2={VOLUME_BASE} stroke="rgba(255,255,255,0.15)" />
                    {show.median && <circle cx={hovered.marker.x} cy={hovered.marker.medianY} r="3.5" fill="var(--accent)" />}
                    {show.movingAvg && hovered.marker.movingY != null && <circle cx={hovered.marker.x} cy={hovered.marker.movingY} r="3" fill="var(--info)" />}
                    {show.avgPrice && hovered.marker.avgY != null && <circle cx={hovered.marker.x} cy={hovered.marker.avgY} r="3" fill="var(--danger)" />}
                  </>
                )}
              </svg>
              {hovered && (
                <div className="pointer-events-none absolute top-2.5 z-10 whitespace-nowrap rounded-md border border-border bg-bg-raised px-2.5 py-1.5 text-xs" style={tooltipStyle} data-market-stats-tooltip>
                  <div className="font-semibold text-accent">{formatStamp(hovered.row.time, locale)}</div>
                  {show.median && <div className="text-text-secondary">{tr("browse.tooltipMedian", { value: `${num(hovered.row.median)}p` })}</div>}
                  {show.movingAvg && hovered.row.movingAvg != null && <div className="text-text-secondary">{tr("browse.tooltipMovingAvg", { value: num(hovered.row.movingAvg) })}</div>}
                  {show.avgPrice && hovered.row.avgPrice != null && <div className="text-text-secondary">{tr("common.avg", { value: num(hovered.row.avgPrice) })}</div>}
                  {show.candles && hovered.row.openPrice != null && hovered.row.closedPrice != null && hovered.row.maxPrice != null && hovered.row.minPrice != null && (
                    <div className="text-text-secondary">{tr("browse.tooltipCandle", { open: num(hovered.row.openPrice), close: num(hovered.row.closedPrice), high: num(hovered.row.maxPrice), low: num(hovered.row.minPrice) })}</div>
                  )}
                  {hovered.row.volume != null && <div className="text-text-secondary">{tr("browse.tooltipVolume", { value: hovered.row.volume })}</div>}
                </div>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
