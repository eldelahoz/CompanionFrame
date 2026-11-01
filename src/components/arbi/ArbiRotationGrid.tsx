import { dronesPerRotation, hueColor, relativePerformanceHue } from "../../lib/arbi/arbiChartData";
import type { Translator } from "../../lib/i18n";
import type { ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";

export default function ArbiRotationGrid({ stats, tr }: { stats: ArbiRunStats; tr: Translator }) {
  const counts = dronesPerRotation(stats);
  const min = counts.length ? Math.min(...counts) : 0;
  const max = counts.length ? Math.max(...counts) : 0;
  const avg = counts.length ? counts.reduce((s, c) => s + c, 0) / counts.length : 0;
  const color = (c: number) => (c === max ? "var(--success)" : hueColor(relativePerformanceHue(c, min, max)));
  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr("arbi.rotations.title")} desc={tr("arbi.rotations.desc")} />
      {counts.length === 0 ? (
        <p className="py-4 text-center text-sm text-text-muted">–</p>
      ) : (
        <>
          <div className="grid grid-cols-[repeat(auto-fill,minmax(2.25rem,1fr))] gap-1.5" data-arbi-rotation-grid>
            {counts.map((c, i) => (
              <div key={i} className="flex h-9 items-center justify-center rounded-sm bg-bg-raised text-xs font-bold" style={{ color: color(c) }} title={tr("arbi.rotations.round", { n: String(i + 1) })}>
                {c}
              </div>
            ))}
          </div>
          <p className="mt-3 text-xs text-text-muted">{tr("arbi.rotations.summary", { low: String(min), avg: avg.toFixed(1), high: String(max) })}</p>
        </>
      )}
    </Panel>
  );
}
