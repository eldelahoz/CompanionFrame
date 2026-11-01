import type { ArbiRun } from "../ipc";
import { arbiMetricValue, arbiUsableRuns } from "./arbiCompare";
import { runPersonalBest } from "../runPersonalBest";

/** Both metrics are per-minute rates, so runs of different length compare. */
export type ArbiTrendMetric = "dronesPerMin" | "expectedVitusPerMin";

interface ArbiPbContext {
  metric: ArbiTrendMetric;
  value: number;
  /** 1-based position within the node + mission-type pool. */
  rank: number;
  poolSize: number;
  isPb: boolean;
  vsBestPct: number | null;
  vsSecondPct: number | null;
}

/** Where this run sits among the user's other runs on the same node and mission type. */
export function arbiPersonalBest(
  run: ArbiRun,
  all: readonly ArbiRun[],
  metrics: readonly ArbiTrendMetric[] = ["dronesPerMin", "expectedVitusPerMin"],
): ArbiPbContext[] {
  const pool = arbiUsableRuns(all).filter(
    (candidate) => candidate.node === run.node && candidate.missionType === run.missionType,
  );
  const out: ArbiPbContext[] = [];
  for (const metric of metrics) {
    const value = arbiMetricValue(run, metric);
    if (value === null || !Number.isFinite(value)) continue;
    const others = pool
      .filter((candidate) => candidate.id !== run.id)
      .map((candidate) => arbiMetricValue(candidate, metric))
      .filter((v): v is number => v !== null);
    out.push({
      metric,
      ...runPersonalBest(value, others, true),
    });
  }
  return out;
}
