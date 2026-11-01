import { useState } from "react";
import { dpmSeries } from "../../lib/arbi/arbiChartData";
import type { Translator } from "../../lib/i18n";
import type { ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";

const W = 440;
const H = 230;
const M = { top: 16, right: 14, bottom: 24, left: 36 };
const GW = W - M.left - M.right;
const GH = H - M.top - M.bottom;

export default function ArbiDpmChart({ stats, tr }: { stats: ArbiRunStats; tr: Translator }) {
  const [tip, setTip] = useState<{ text: string; x: number; y: number } | null>(null);
  const series = dpmSeries(stats);
  const min = series.length ? Math.floor(Math.min(...series)) : 0;
  const max = series.length ? Math.ceil(Math.max(...series)) : 1;
  const range = max - min || 1;
  const avg = series.length ? series.reduce((a, b) => a + b, 0) / series.length : 0;
  const yOf = (v: number) => GH - ((v - min) / range) * GH;
  const points = series.map((val, i) => ({ x: series.length > 1 ? (i / (series.length - 1)) * GW : GW / 2, y: yOf(val), val, rotation: i + 1 }));
  const target = range / 6;
  const mag = Math.pow(10, Math.floor(Math.log10(Math.max(1, target))));
  const step = [1, 2, 5].map((m) => mag * m).find((s) => s >= target) ?? mag * 10;
  const ticks: number[] = [];
  for (let v = min; v <= max; v += step) ticks.push(v);
  const stride = series.length > 12 ? Math.ceil(series.length / 12) : 1;

  return (
    <Panel className="flex flex-col p-5">
      {tip && (
        <div
          className="pointer-events-none fixed z-[500] whitespace-nowrap rounded-[var(--radius-sm)] border border-border-strong bg-bg-raised px-2.5 py-1 text-xs text-text-primary"
          style={{ left: tip.x + 14, top: tip.y - 34 }}
          aria-hidden="true"
        >
          {tip.text}
        </div>
      )}
      <PanelHeading title={tr("arbi.dpm.title")} desc={tr("arbi.dpm.desc")} />
      {series.length > 1 ? (
        <svg viewBox={`0 0 ${W} ${H}`} className="block w-full font-mono text-[10px]" aria-hidden="true">
          <g transform={`translate(${M.left}, ${M.top})`}>
            {ticks.map((t) => (
              <g key={t}>
                <line x1="0" y1={yOf(t)} x2={GW} y2={yOf(t)} stroke="var(--border)" strokeWidth="1" />
                <text x="-6" y={yOf(t)} dy="3" textAnchor="end" fill="var(--text-muted)">
                  {t}
                </text>
              </g>
            ))}
            <path d={points.map((p, i) => `${i === 0 ? "M" : "L"} ${p.x},${p.y}`).join(" ")} fill="none" stroke="var(--accent)" strokeWidth="2" strokeLinejoin="round" />
            <line x1="0" y1={yOf(avg)} x2={GW} y2={yOf(avg)} stroke="currentColor" strokeWidth="1" strokeDasharray="4" opacity="0.8" className="text-text-primary" />
            <text x={GW} y={yOf(avg) - 6} textAnchor="end" fill="currentColor" className="text-text-primary" fontSize="11">
              {tr("common.avg", { value: avg.toFixed(1) })}
            </text>
            {points.map((p, i) => (
              <g key={p.rotation}>
                <circle cx={p.x} cy={p.y} r="2.5" fill="var(--accent)" />
                <circle
                  cx={p.x}
                  cy={p.y}
                  r="8"
                  fill="transparent"
                  style={{ cursor: "pointer" }}
                  onMouseEnter={(e) => setTip({ text: `${tr("arbi.rotations.round", { n: String(p.rotation) })}: ${p.val.toFixed(1)} DPM`, x: e.clientX, y: e.clientY })}
                  onMouseLeave={() => setTip(null)}
                />
                {(i === 0 || i === points.length - 1 || (i + 1) % stride === 0) && (
                  <text x={p.x} y={GH + 15} textAnchor={i === 0 ? "start" : i === points.length - 1 ? "end" : "middle"} fill="var(--text-muted)">
                    {p.rotation}
                  </text>
                )}
              </g>
            ))}
          </g>
        </svg>
      ) : (
        <p className="py-4 text-center text-sm text-text-muted">–</p>
      )}
    </Panel>
  );
}
