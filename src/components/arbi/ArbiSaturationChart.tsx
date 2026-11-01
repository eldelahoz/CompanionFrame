import { bucketSuccessMixPct, saturationAboveThresholdPct, thresholdHue } from "../../lib/arbi/arbiChartData";
import type { Translator } from "../../lib/i18n";
import type { ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";

const THRESHOLD = 15;
const mix = (pct: number) => `color-mix(in oklab, var(--success) ${pct}%, var(--danger))`;

/** Share bars in the success-to-danger ramp; also used for drone cadence. */
export const BucketBars = ({ buckets }: { buckets: Array<{ label: string; pct: number }> }) => (
  <div className="flex flex-col gap-1.5">
    {buckets.map((b, i) => (
      <div key={b.label} className="flex items-center gap-2">
        <span className="w-12 shrink-0 text-right font-mono text-xs text-text-secondary">{b.label}</span>
        <div className="h-3.5 flex-1 overflow-hidden rounded-sm bg-bg-raised">
          <div className="h-full rounded-sm" style={{ width: `${Math.min(100, b.pct)}%`, backgroundColor: mix(bucketSuccessMixPct(i)) }} />
        </div>
        <span className="w-12 shrink-0 text-right font-mono text-xs font-semibold text-text-primary">{b.pct.toFixed(1)}%</span>
      </div>
    ))}
  </div>
);

export default function ArbiSaturationChart({ stats, tr }: { stats: ArbiRunStats; tr: Translator }) {
  const buckets = stats.saturationBuckets;
  const abovePct = saturationAboveThresholdPct(buckets, THRESHOLD);
  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr("arbi.saturation.title")} desc={tr("arbi.saturation.desc")} />
      {buckets.some((b) => b.seconds > 0) ? (
        <>
          <BucketBars buckets={buckets} />
          <div className="mt-4 rounded-[var(--radius-md)] border border-border bg-bg-raised px-3 py-2.5">
            <div className="text-xs text-text-secondary">{tr("arbi.saturation.threshold", { count: String(THRESHOLD) })}</div>
            <div className="mt-0.5 text-2xl font-bold" style={{ color: mix(thresholdHue(abovePct) / 1.2) }}>
              {abovePct.toFixed(1)}%
            </div>
          </div>
        </>
      ) : (
        <p className="py-4 text-center text-sm text-text-muted">–</p>
      )}
    </Panel>
  );
}
