import type { PtPhase, PtRunRecord } from "./ipc";
import { percentDelta, runPersonalBest } from "./runPersonalBest";

const asRecord = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : null;

/** Middle value, or the mean of the two middles on an even count. 0 when empty. */
function median(values: readonly number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

const dayKey = (date: Date): string =>
  Number.isNaN(date.getTime())
    ? ""
    : `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;

/** Inclusive local-day range; an unusable bound is open. */
function localDateRange(from = "", to = ""): (timestamp: number) => boolean {
  const boundary = (key: string, after: boolean): number | null => {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(key)) return null;
    const date = new Date(`${key}T00:00:00`);
    if (dayKey(date) !== key) return null;
    // Calendar-day stepping keeps the inclusive end correct across DST changes.
    if (after) date.setDate(date.getDate() + 1);
    return date.getTime();
  };
  const start = boundary(from, false);
  const end = boundary(to, true);
  return (timestamp) =>
    (start === null && end === null) ||
    (Number.isFinite(timestamp) && (start === null || timestamp >= start) && (end === null || timestamp < end));
}

export type PtMetric = "total" | "flight" | "shield" | "leg" | "body" | "pylon";
export const PT_METRICS: readonly PtMetric[] = [
  "total",
  "flight",
  "shield",
  "leg",
  "body",
  "pylon",
];
export const PT_METRIC_KEYS = {
  total: "common.total",
  flight: "pt.stat.flight",
  shield: "pt.stat.shield",
  leg: "pt.stat.leg",
  body: "pt.stat.body",
  pylon: "pt.stat.pylon",
} as const;
export const PT_EXCLUSION_KEYS = {
  incomplete: "arbi.incomplete",
  aborted: "arbi.end.aborted",
  bugged: "pt.badge.bugged",
  duplicate: "arbi.duplicate",
  migrated: "pt.badge.migration",
  flight: "pt.badge.flightEstimate",
  telemetry: "pt.exclusion.telemetry",
  squad: "pt.exclusion.squad",
} as const;
type PtExclusion = keyof typeof PT_EXCLUSION_KEYS;
type PtSquadSize = 1 | 2 | 3 | 4;
export interface PtFilters {
  squad: "all" | "unknown" | "solo" | "2" | "3" | "4";
  status: "all" | "eligible" | "bugged" | "incomplete";
  from: string;
  to: string;
  tag: string;
  showDuplicates: boolean;
}

export function ptMetricValue(run: PtRunRecord, metric: PtMetric): number {
  const key = metric === "total" ? "durationSec" : (`${metric}Sec` as const);
  return run[key];
}

export function ptSquadSize(run: PtRunRecord): PtSquadSize | null {
  const count = new Set(
    (Array.isArray(run.players) ? run.players : [])
      .filter((name): name is string => typeof name === "string")
      .map((name) => name.trim().toLowerCase())
      .filter(Boolean),
  ).size;
  // This is a recorded-player count, not proof that capture saw the entire squad.
  return count >= 1 && count <= 4 ? (count as PtSquadSize) : null;
}

const timing = (value: number): boolean => Number.isFinite(value) && value >= 0;
const PHASE_METRICS = ["totalSec", "shieldSec", "legSec", "bodySec", "pylonSec"] as const;
const LEGS = new Set(["frontLeft", "frontRight", "backLeft", "backRight"]);

function validPhase(value: unknown): value is PtPhase {
  const phase = asRecord(value);
  return (
    !!phase &&
    [1, 2, 3, 4].includes(Number(phase.index)) &&
    typeof phase.index === "number" &&
    PHASE_METRICS.every((key) => typeof phase[key] === "number" && timing(phase[key])) &&
    Array.isArray(phase.legs) &&
    phase.legs.every((value) => {
      const leg = asRecord(value);
      return (
        !!leg &&
        typeof leg.leg === "string" &&
        LEGS.has(leg.leg) &&
        typeof leg.seconds === "number" &&
        timing(leg.seconds)
      );
    }) &&
    Array.isArray(phase.shields) &&
    phase.shields.every((value) => {
      const shield = asRecord(value);
      return (
        !!shield &&
        typeof shield.element === "string" &&
        shield.element.length > 0 &&
        typeof shield.seconds === "number" &&
        timing(shield.seconds)
      );
    })
  );
}

export function ptExclusionReason(run: PtRunRecord): PtExclusion | null {
  if (run.aborted) return "aborted";
  if (!run.complete) return "incomplete";
  if (run.bugged) return "bugged";
  if (run.duplicateOf != null) return "duplicate";
  if (run.hostMigration) return "migrated";
  if (run.flightUnreliable) return "flight";
  if (
    !PT_METRICS.every((metric) => timing(ptMetricValue(run, metric))) ||
    run.durationSec <= 0 ||
    !Number.isFinite(run.startedAt) ||
    !Number.isFinite(new Date(run.startedAt).getTime()) ||
    !Number.isFinite(run.endedAt) ||
    !Number.isFinite(new Date(run.endedAt).getTime()) ||
    run.endedAt < run.startedAt ||
    !Array.isArray(run.phases) ||
    run.phases.length !== 4 ||
    !run.phases.every(validPhase) ||
    new Set(run.phases.map((phase) => phase.index)).size !== 4
  )
    return "telemetry";
  // Log timestamps have millisecond precision; summed windows accumulate rounding error.
  const close = (left: number, right: number) =>
    Number.isFinite(left) && Number.isFinite(right) && Math.abs(left - right) <= 0.02;
  for (const phase of run.phases) {
    // Phase 2 goes straight to legs and has no shield window.
    if (
      phase.legs.length !== 4 ||
      new Set(phase.legs.map((leg) => leg.leg)).size !== 4 ||
      (phase.index !== 2 && phase.shields.length === 0) ||
      phase.shieldSec + phase.legSec + phase.bodySec + phase.pylonSec > phase.totalSec + 0.02 ||
      !close(
        phase.shieldSec,
        phase.shields.reduce((sum, entry) => sum + entry.seconds, 0),
      ) ||
      !close(
        phase.legSec,
        phase.legs.reduce((sum, entry) => sum + entry.seconds, 0),
      )
    )
      return "telemetry";
  }
  if (
    !close(
      run.durationSec,
      run.flightSec + run.phases.reduce((sum, phase) => sum + phase.totalSec, 0),
    )
  )
    return "telemetry";
  for (const metric of ["shield", "leg", "body", "pylon"] as const) {
    if (
      !close(
        ptMetricValue(run, metric),
        run.phases.reduce((sum, phase) => sum + phase[`${metric}Sec`], 0),
      )
    )
      return "telemetry";
  }
  return null;
}

export function ptEligibleRuns(runs: readonly PtRunRecord[]): PtRunRecord[] {
  return runs.filter((run) => ptExclusionReason(run) === null);
}

export function ptComparisonExclusionReason(run: PtRunRecord): PtExclusion | null {
  return ptExclusionReason(run) ?? (ptSquadSize(run) === null ? "squad" : null);
}

export function ptFilterRuns(runs: readonly PtRunRecord[], filters: PtFilters): PtRunRecord[] {
  const inDateRange = localDateRange(filters.from, filters.to);
  const tag = filters.tag.trim().toLocaleLowerCase();
  return runs
    .filter((run) => {
      if (!filters.showDuplicates && run.duplicateOf != null) return false;
      const size = ptSquadSize(run);
      if (
        filters.squad === "unknown"
          ? size !== null
          : filters.squad !== "all" &&
            size !== (filters.squad === "solo" ? 1 : Number(filters.squad))
      )
        return false;
      if (filters.status === "eligible" && ptExclusionReason(run) !== null) return false;
      if (filters.status === "bugged" && !run.bugged) return false;
      if (filters.status === "incomplete" && run.complete && !run.aborted) return false;
      if (!inDateRange(run.startedAt)) return false;
      return !tag || (run.tags ?? []).some((entry) => entry.toLocaleLowerCase() === tag);
    })
    .sort((a, b) => {
      const left = Number.isFinite(a.startedAt) ? a.startedAt : -Infinity;
      const right = Number.isFinite(b.startedAt) ? b.startedAt : -Infinity;
      return right - left || a.id.localeCompare(b.id);
    });
}

export function ptMetricSummary(runs: readonly PtRunRecord[]) {
  const eligible = ptEligibleRuns(runs).sort(
    (a, b) => a.startedAt - b.startedAt || a.id.localeCompare(b.id),
  );
  const metrics = PT_METRICS.map((metric) => {
    const values = eligible.map((run) => ptMetricValue(run, metric));
    return {
      metric,
      mean: values.length ? values.reduce((sum, value) => sum + value / values.length, 0) : null,
      median: values.length ? median(values) : null,
      best: values.length ? values.reduce((best, value) => Math.min(best, value), Infinity) : null,
    };
  });
  return { eligible, excluded: runs.length - eligible.length, metrics };
}

export function ptPhaseRows(
  run: PtRunRecord,
): { index: number; phase: PtPhase | null; elapsed: number | null }[] {
  let elapsed: number | null =
    timing(run.flightSec) && !run.flightUnreliable ? run.flightSec : null;
  return [1, 2, 3, 4].map((index) => {
    const found = (Array.isArray(run.phases) ? run.phases : []).filter(
      (phase) => asRecord(phase)?.index === index,
    );
    const phase = found.length === 1 && validPhase(found[0]) ? found[0] : null;
    elapsed = phase && timing(phase.totalSec) && elapsed !== null ? elapsed + phase.totalSec : null;
    return { index, phase, elapsed };
  });
}

export function ptComparison(run: PtRunRecord, baseline: PtRunRecord) {
  if (
    ptComparisonExclusionReason(run) ||
    ptComparisonExclusionReason(baseline) ||
    ptSquadSize(run) !== ptSquadSize(baseline)
  )
    return [];
  return PT_METRICS.map((metric) => ({
    metric,
    seconds: ptMetricValue(run, metric) - ptMetricValue(baseline, metric),
    percent: percentDelta(ptMetricValue(run, metric), ptMetricValue(baseline, metric)),
  }));
}

export function ptPersonalBest(run: PtRunRecord, all: readonly PtRunRecord[]) {
  if (ptComparisonExclusionReason(run)) return [];
  const pool = ptEligibleRuns(all).filter(
    (candidate) => ptSquadSize(candidate) === ptSquadSize(run),
  );
  if (!pool.some((candidate) => candidate.id === run.id)) return [];
  const others = pool.filter((candidate) => candidate.id !== run.id);
  return PT_METRICS.map((metric) => {
    const value = ptMetricValue(run, metric);
    return {
      metric,
      ...runPersonalBest(
        value,
        others.map((candidate) => ptMetricValue(candidate, metric)),
        false,
      ),
    };
  });
}

export function ptBestRunIds(runs: readonly PtRunRecord[]): ReadonlySet<string> {
  const eligible = ptEligibleRuns(runs);
  const best = new Map<PtSquadSize, number>();
  for (const run of eligible) {
    const size = ptSquadSize(run);
    if (size !== null) best.set(size, Math.min(best.get(size) ?? Infinity, run.durationSec));
  }
  return new Set(
    eligible
      .filter((run) => {
        const size = ptSquadSize(run);
        return size !== null && run.durationSec === best.get(size);
      })
      .map((run) => run.id),
  );
}

export function formatPtTime(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) return "\u2014";
  const millis = Math.round(seconds * 1000);
  if (!Number.isSafeInteger(millis)) return "\u2014";
  if (millis < 60000) return (millis / 1000).toFixed(3);
  const minutes = Math.floor(millis / 60000);
  return `${minutes}:${((millis % 60000) / 1000).toFixed(3).padStart(6, "0")}`;
}

export function formatPtLength(seconds: number | null): string {
  const text = seconds === null ? "\u2014" : formatPtTime(seconds);
  return text.includes(":") || text === "\u2014" ? text : `${text}s`;
}

export function formatPtSeconds(seconds: number | null): string {
  return seconds !== null && Number.isFinite(seconds) && seconds >= 0 ? seconds.toFixed(3) : "—";
}
