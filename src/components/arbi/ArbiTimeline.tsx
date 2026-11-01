import { computeCadence, type ArbiSegment, type ArbiSegmentKind } from "../../lib/arbi/arbiCadence";
import { formatDuration } from "../../lib/arbi/arbiChartData";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";

const COLOR: Record<ArbiSegmentKind, string> = { active: "var(--success)", dry: "var(--warning)", reward: "var(--info)", gap: "var(--text-muted)" };
const LEGEND: ArbiSegmentKind[] = ["active", "dry", "reward", "gap"];
const KIND_KEYS: Record<ArbiSegmentKind, MessageKey> = {
  active: "arbi.cadence.kind.active",
  dry: "arbi.cadence.kind.dry",
  reward: "arbi.cadence.kind.reward",
  gap: "arbi.cadence.kind.gap",
};

const Stat = ({ label, value, sub }: { label: string; value: string | number; sub?: string }) => (
  <div className="flex flex-col gap-0.5">
    <dt className="text-xs text-text-muted">{label}</dt>
    <dd className="m-0 font-bold text-text-primary">{value}</dd>
    {sub && <span className="text-xs text-text-muted">{sub}</span>}
  </div>
);

export default function ArbiTimeline({ stats, tr }: { stats: ArbiRunStats; tr: Translator }) {
  const c = computeCadence(stats);
  if (!c) return null;
  const at = (sec: number) => tr("arbi.cadence.atTime", { time: formatDuration(sec - c.startSec) });
  const title = (s: ArbiSegment) =>
    `${tr(KIND_KEYS[s.kind])}, ${formatDuration(s.end - s.start)}, ${at(s.start)}, ${tr("arbi.cadence.dronesCount", { count: String(s.drones) })}`;
  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr("arbi.cadence.title")} desc={tr("arbi.cadence.desc")} />
      <div className="flex h-6 w-full overflow-hidden rounded-sm border border-border bg-bg-raised" data-arbi-timeline>
        {c.segments.map((s) => (
          <div key={s.start} className="h-full min-w-[2px]" style={{ flex: `${Math.max(s.end - s.start, 0.001)} 1 0`, backgroundColor: COLOR[s.kind] }} title={title(s)} />
        ))}
      </div>
      <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-text-muted">
        {LEGEND.map((k) => (
          <span key={k} className="inline-flex items-center gap-1.5">
            <span className="inline-block h-2 w-2 rounded-full" style={{ backgroundColor: COLOR[k] }} />
            {tr(KIND_KEYS[k])}
          </span>
        ))}
      </div>
      <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-2.5 text-sm sm:grid-cols-4">
        <Stat label={tr("arbi.cadence.longestDry")} value={c.longestDry ? formatDuration(c.longestDry.end - c.longestDry.start) : "–"} sub={c.longestDry ? at(c.longestDry.start) : undefined} />
        <Stat label={tr("arbi.cadence.drySpells")} value={c.drySpellCount} sub={tr("arbi.cadence.threshold", { sec: c.dryThresholdSec.toFixed(0) })} />
        <Stat label={tr("arbi.cadence.busiestMinute")} value={c.busiestMinute ? c.busiestMinute.drones : "–"} sub={c.busiestMinute ? at(c.busiestMinute.start) : undefined} />
        <Stat label={tr("arbi.cadence.activeShare")} value={`${(c.activeShare * 100).toFixed(1)}%`} />
      </dl>
    </Panel>
  );
}
