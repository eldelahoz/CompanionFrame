import { formatClock } from "../../lib/arbi/arbiChartData";
import { hasCadenceData } from "../../lib/arbi/arbiCadence";
import { ARBI_DRY_WAIT_SEC, ARBI_PEAK_WINDOW_SEC, computeDroneCadence } from "../../lib/arbi/arbiDroneCadence";
import type { Translator } from "../../lib/i18n";
import type { ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";
import { BucketBars } from "./ArbiSaturationChart";

const Box = ({ label, value, sub }: { label: string; value: string | number; sub?: string }) => (
  <div className="rounded-[var(--radius-md)] border border-border bg-bg-raised px-3 py-2.5">
    <div className="text-xs text-text-secondary">{label}</div>
    <div className="mt-0.5 text-2xl font-bold text-text-primary">{value}</div>
    {sub && <span className="text-xs text-text-muted">{sub}</span>}
  </div>
);

export default function ArbiDroneCadenceChart({ stats, tr }: { stats: ArbiRunStats; tr: Translator }) {
  // Without the pause windows every reward screen would read as a dry spell.
  const c = hasCadenceData(stats) ? computeDroneCadence(stats) : null;
  if (!c || c.totalWaitSec <= 0) return null;
  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr("arbi.droneCadence.title")} desc={tr("arbi.droneCadence.desc")} />
      <div data-arbi-drone-cadence>
        <BucketBars buckets={c.buckets} />
      </div>
      <div className="mt-4 grid grid-cols-2 gap-3">
        <Box label={tr("arbi.droneCadence.dry", { sec: String(ARBI_DRY_WAIT_SEC) })} value={`${c.dryPct.toFixed(1)}%`} />
        <Box
          label={tr("arbi.droneCadence.peak", { sec: String(ARBI_PEAK_WINDOW_SEC) })}
          value={c.peak ? c.peak.drones : "–"}
          sub={c.peak ? tr("arbi.cadence.atTime", { time: formatClock(c.peak.atSec) }) : undefined}
        />
      </div>
    </Panel>
  );
}
