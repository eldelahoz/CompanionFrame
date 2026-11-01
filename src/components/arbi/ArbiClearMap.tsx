import { ARBI_SATURATION_THRESHOLD, formatClock, median, relativePerformanceHue, hueColor, rotationClearCells, waveClearCells } from "../../lib/arbi/arbiChartData";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { ArbiMissionType } from "../../../config/shared/arbiTypes";
import type { ArbiRunStats } from "../../lib/ipc";
import { Panel, PanelHeading } from "../ui";

/** Above this per-wave clear time the box is flagged slow (reference threshold). */
const SLOW_WAVE_SEC = 25;
/** Disruption rounds vary by squad, so judge them against this run's own median. */
const SLOW_ROUND_FACTOR = 1.25;
const FAST_ROUND_FACTOR = 0.8;
const BAD = "border-danger/50 bg-danger/15 text-danger";
const GOOD = "border-success/50 bg-success/15 text-success";
const NEUTRAL = "border-border bg-surface-2 text-text-secondary";

const KEYS: Record<"wave" | "round" | "rotation", [MessageKey, MessageKey]> = {
  wave: ["arbi.waveMap.title", "arbi.waveMap.desc"],
  round: ["arbi.roundMap.title", "arbi.roundMap.desc"],
  rotation: ["arbi.clearMap.rotation.title", "arbi.clearMap.rotation.desc"],
};

export default function ArbiClearMap({ stats, missionType, tr }: { stats: ArbiRunStats; missionType: ArbiMissionType; tr: Translator }) {
  const mode = missionType === "disruption" ? "round" : missionType === "defense" ? "wave" : "rotation";
  const cells = mode === "rotation" ? rotationClearCells(stats) : waveClearCells(stats);
  if (cells.length === 0) return null;
  const durations = cells.map((c) => c.durationSec);
  const med = mode === "round" ? median(durations) : 0;
  const fastest = Math.min(...durations);
  const slowest = Math.max(...durations);

  const tone = (sec: number) => {
    if (mode === "wave") return sec > SLOW_WAVE_SEC ? BAD : GOOD;
    if (mode === "round") {
      if (med <= 0) return NEUTRAL;
      return sec > med * SLOW_ROUND_FACTOR ? BAD : sec < med * FAST_ROUND_FACTOR ? GOOD : NEUTRAL;
    }
    return "border-border bg-bg-raised";
  };
  // Rotation cells are judged against this run only; a single rotation is not painted red.
  const relColor = (sec: number) => hueColor(slowest === fastest ? 120 : relativePerformanceHue(slowest - sec, 0, slowest - fastest));
  const [titleKey, descKey] = KEYS[mode];

  return (
    <Panel className="flex flex-col p-5">
      <PanelHeading title={tr(titleKey)} desc={tr(descKey)} />
      <div className="grid grid-cols-[repeat(auto-fill,minmax(3.25rem,1fr))] gap-1.5" data-arbi-clear-map>
        {cells.map((cell) => (
          <div
            key={cell.index}
            className={`flex h-11 flex-col items-center justify-center rounded-sm border leading-tight ${tone(cell.durationSec)}`}
            style={mode === "rotation" ? { color: relColor(cell.durationSec) } : undefined}
            title={
              cell.saturationPct === null
                ? tr("arbi.clearMap.cellPlain", { index: String(cell.index), time: formatClock(cell.durationSec) })
                : tr("arbi.clearMap.cell", { index: String(cell.index), time: formatClock(cell.durationSec), pct: cell.saturationPct.toFixed(0), count: String(ARBI_SATURATION_THRESHOLD) })
            }
          >
            <span className="font-mono text-xs font-bold">{formatClock(cell.durationSec)}</span>
            <span className="font-mono text-[10px] opacity-70">{cell.saturationPct === null ? "–" : `${cell.saturationPct.toFixed(0)}%`}</span>
          </div>
        ))}
      </div>
      <p className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-text-muted">
        {mode === "wave" ? (
          <>
            <span className="inline-flex items-center gap-1.5">
              <span className="inline-block h-2 w-2 rounded-full bg-success" />
              {tr("arbi.clearMap.legend.fast", { sec: String(SLOW_WAVE_SEC) })}
            </span>
            <span className="inline-flex items-center gap-1.5">
              <span className="inline-block h-2 w-2 rounded-full bg-danger" />
              {tr("arbi.clearMap.legend.slow", { sec: String(SLOW_WAVE_SEC) })}
            </span>
          </>
        ) : (
          <span>{tr("arbi.clearMap.legend.range", { fast: formatClock(fastest), slow: formatClock(slowest) })}</span>
        )}
        <span>{tr("arbi.clearMap.legend.saturation", { count: String(ARBI_SATURATION_THRESHOLD) })}</span>
      </p>
    </Panel>
  );
}
