import { describe, expect, it } from "vitest";
import type { PtRunRecord } from "./ipc";
import fixture from "./ptRun.fixture.json";
import {
  PT_METRICS,
  formatPtLength,
  formatPtTime,
  ptBestRunIds,
  ptComparison,
  ptComparisonExclusionReason,
  ptEligibleRuns,
  ptExclusionReason,
  ptFilterRuns,
  ptMetricSummary,
  ptPersonalBest,
  ptPhaseRows,
  ptSquadSize,
  type PtFilters,
} from "./profitTakerStats";

/** The record the backend saves for the recorded host fixture (EE.log host-single-run), two players. */
function recorded(): PtRunRecord {
  return {
    ...structuredClone(fixture),
    id: "recorded",
    startedAt: new Date(2026, 8, 8, 12).getTime(),
    endedAt: new Date(2026, 8, 8, 12, 2).getTime(),
  } as unknown as PtRunRecord;
}
function scaled(id: string, factor: number, players = ["A", "B", "C", "D"]): PtRunRecord {
  const run = recorded();
  const keys = ["durationSec", "flightSec", "shieldSec", "legSec", "bodySec", "pylonSec"] as const;
  for (const key of keys) run[key] *= factor;
  for (const phase of run.phases) {
    for (const key of ["totalSec", "shieldSec", "legSec", "bodySec", "pylonSec"] as const)
      phase[key] *= factor;
    for (const entry of [...phase.shields, ...phase.legs]) entry.seconds *= factor;
  }
  return { ...run, id, players };
}
const filters: PtFilters = {
  squad: "all",
  status: "all",
  from: "",
  to: "",
  tag: "",
  showDuplicates: false,
};

describe("Profit-Taker comparison data", () => {
  it("keeps missing and impossible recorded rosters out of comparison groups", () => {
    for (const players of [[], ["A", "B", "C", "D", "E"]]) {
      const run = { ...recorded(), players };
      expect(ptSquadSize(run)).toBeNull();
      expect(ptExclusionReason(run)).toBeNull();
      expect(ptComparisonExclusionReason(run)).toBe("squad");
      expect(ptMetricSummary([run]).eligible).toHaveLength(1);
      expect(ptPersonalBest(run, [run])).toEqual([]);
      expect(ptBestRunIds([run]).size).toBe(0);
      expect(ptComparison(run, { ...run, id: "other" })).toEqual([]);
    }
  });
  it.each([1, 2, 3, 4])("compares runs with %s recorded players without mixing counts", (count) => {
    const players = ["A", "B", "C", "D"].slice(0, count);
    const fast = scaled("fast", 0.8, players);
    const slow = scaled("slow", 1, players);
    const other = scaled("other", 0.5, count === 4 ? ["A"] : [...players, "E"]);
    expect(ptSquadSize(fast)).toBe(count);
    expect(ptComparisonExclusionReason(fast)).toBeNull();
    expect(ptPersonalBest(fast, [fast, slow, other])[0]).toMatchObject({
      isPb: true,
      poolSize: 2,
      vsSecondPct: expect.closeTo(-20, 5),
    });
    expect(ptComparison(fast, slow)[0].seconds).toBeCloseTo(-22.5942, 4);
    expect(ptComparison(fast, other)).toEqual([]);
    expect(ptBestRunIds([fast, slow])).toEqual(new Set(["fast"]));
    expect(
      ptFilterRuns([fast, slow, other], {
        ...filters,
        squad: count === 1 ? "solo" : (String(count) as "2" | "3" | "4"),
      }),
    ).toEqual([fast, slow]);
  });
  it.each([null, "2026-09-08T12:00:00Z"])("excludes nonnumeric persisted dates: %s", (value) => {
    const run = { ...recorded(), startedAt: value, endedAt: value } as unknown as PtRunRecord;
    expect(ptExclusionReason(run)).toBe("telemetry");
    expect(ptEligibleRuns([run])).toEqual([]);
    expect(ptBestRunIds([run]).size).toBe(0);
  });

  it("keeps malformed dates after valid runs in a stable order", () => {
    const good = recorded();
    const invalid = [
      { ...good, id: "bad-b", startedAt: null },
      { ...good, id: "bad-a", startedAt: "2026-09-08T12:00:00Z" },
    ] as unknown as PtRunRecord[];
    const input = [invalid[0], good, invalid[1]];
    expect(ptFilterRuns(input, filters).map((run) => run.id)).toEqual([good.id, "bad-a", "bad-b"]);
    expect(ptFilterRuns([...input].reverse(), filters)).toEqual(ptFilterRuns(input, filters));
  });

  it("reconciles the recorded host metrics and all four cumulative phase clocks", () => {
    const run = recorded();
    expect(ptExclusionReason(run)).toBeNull();
    expect(ptSquadSize(run)).toBe(2);
    expect(ptMetricSummary([run]).metrics.map((row) => row.mean)).toEqual([
      expect.closeTo(112.971, 3),
      expect.closeTo(11.339, 3),
      expect.closeTo(27.25, 3),
      expect.closeTo(27.107, 3),
      expect.closeTo(2.131, 3),
      expect.closeTo(24.573, 3),
    ]);
    expect(ptPhaseRows(run).map((row) => row.elapsed)).toEqual(
      [51.001, 60.729, 99.922, 112.971].map((value) => expect.closeTo(value, 3)),
    );
    expect(run.phases[1].shieldSec).toBeCloseTo(0);
    expect(run.phases[1].pylonSec).toBeCloseTo(0);
  });

  it.each([
    ["incomplete", { complete: false }],
    ["aborted", { aborted: true }],
    ["bugged", { bugged: true }],
    ["duplicate", { duplicateOf: "another" }],
    ["migrated", { hostMigration: true }],
    ["flight", { flightUnreliable: true }],
  ] as const)("excludes %s consistently from every metric pool", (reason, patch) => {
    const run = { ...recorded(), ...patch } as PtRunRecord;
    expect(ptExclusionReason(run)).toBe(reason);
    expect(ptEligibleRuns([run])).toEqual([]);
    expect(ptPersonalBest(run, [run])).toEqual([]);
    expect(ptMetricSummary([run]).metrics.every((row) => row.mean === null)).toBe(true);
    expect(ptBestRunIds([run]).size).toBe(0);
  });

  it.each([
    { phases: [null] },
    { phases: null },
    { phases: [{ index: 1, legs: [null], shields: [null] }] },
    { durationSec: NaN },
    { flightSec: Infinity },
    { legSec: -1 },
    { durationSec: 1 },
    { shieldSec: 1000 },
    { startedAt: Infinity },
  ])("contains malformed persisted data without crashing: %j", (patch) => {
    const run = { ...recorded(), ...patch } as unknown as PtRunRecord;
    expect(() => ptExclusionReason(run)).not.toThrow();
    expect(ptExclusionReason(run)).not.toBeNull();
    expect(() => ptPhaseRows(run)).not.toThrow();
    expect(() => ptSquadSize(run)).not.toThrow();
  });

  it("rejects duplicated phases, missing legs and impossible component sums", () => {
    const repeated = recorded();
    repeated.phases[3] = repeated.phases[0];
    expect(ptExclusionReason(repeated)).toBe("telemetry");
    const missingLeg = recorded();
    missingLeg.phases[0].legs.pop();
    expect(ptExclusionReason(missingLeg)).toBe("telemetry");
    const impossible = recorded();
    impossible.phases[0].bodySec = 500;
    expect(ptExclusionReason(impossible)).toBe("telemetry");
    const missingShield = recorded();
    missingShield.phases[2].shields = [];
    expect(ptExclusionReason(missingShield)).toBe("telemetry");
  });

  it("partitions personal bests by recorded squad, keeps ties and compares the true runner-up", () => {
    const fastest = scaled("fastest", 0.8);
    const tied = { ...fastest, id: "tied" };
    const runner = scaled("runner", 1);
    const third = scaled("third", 1.2);
    const solo = scaled("solo", 0.5, ["Solo"]);
    const three = scaled("three", 0.4, ["A", "B", "C"]);
    expect(ptPersonalBest(fastest, [third, solo, runner, fastest])[0]).toMatchObject({
      isPb: true,
      poolSize: 3,
      vsSecondPct: expect.closeTo(-20, 5),
    });
    const runs = [third, solo, runner, fastest, tied, three];
    expect(ptBestRunIds(runs)).toEqual(new Set(["fastest", "tied", "solo", "three"]));
    expect(ptBestRunIds([...runs].reverse())).toEqual(ptBestRunIds(runs));
    expect(ptComparison(fastest, solo)).toEqual([]);
    expect(ptComparison(fastest, runner)[0].seconds).toBeCloseTo(-22.5942, 4);
    expect(ptSquadSize({ ...solo, players: [" A ", "a", "B", "C", "D", ""] })).toBe(4);
    expect(ptSquadSize({ ...solo, players: [] })).toBeNull();
  });

  it("preserves legitimate zero mechanics without division by zero", () => {
    const zero = scaled("zero", 1);
    zero.bodySec = 0;
    zero.phases.forEach((phase) => (phase.bodySec = 0));
    expect(ptExclusionReason(zero)).toBeNull();
    expect(
      ptComparison({ ...zero, id: "other" }, zero).find((row) => row.metric === "body"),
    ).toMatchObject({ seconds: 0, percent: null });
  });

  it("keeps missing phases unknown and stops elapsed clocks after a gap", () => {
    const run = recorded();
    run.phases.splice(1, 1);
    const rows = ptPhaseRows(run);
    expect(rows).toHaveLength(4);
    expect(rows[1]).toEqual({ index: 2, phase: null, elapsed: null });
    expect(rows[2].phase).not.toBeNull();
    expect(rows[2].elapsed).toBeNull();
    expect(
      ptPhaseRows({ ...recorded(), flightUnreliable: true }).every((row) => row.elapsed === null),
    ).toBe(true);
  });

  it("uses one shared eligible denominator for all six means", () => {
    const runs = [
      scaled("fast", 0.8, ["Solo"]),
      scaled("normal", 1),
      scaled("slow", 1.2),
      { ...recorded(), id: "bad", bugged: true },
    ];
    const summary = ptMetricSummary(runs);
    expect(summary.eligible).toHaveLength(3);
    expect(summary.excluded).toBe(1);
    expect(summary.metrics.map((row) => row.metric)).toEqual(PT_METRICS);
    expect(summary.metrics[0].mean).toBeCloseTo(112.971, 3);
    expect(
      ptMetricSummary(ptFilterRuns(runs, { ...filters, squad: "4" })).metrics[0].mean,
    ).toBeCloseTo(112.971 * 1.1, 3);
  });

  it("applies local inclusive dates, squad, status, case-insensitive tags and duplicates", () => {
    const first = {
      ...recorded(),
      id: "first",
      startedAt: new Date(2026, 8, 8, 0).getTime(),
      tags: ["Fast"],
    };
    const last = {
      ...first,
      id: "last",
      startedAt: new Date(2026, 8, 8, 23, 59, 59, 999).getTime(),
    };
    const next = { ...first, id: "next", startedAt: new Date(2026, 8, 9, 0).getTime() };
    const duplicate = { ...first, id: "copy", duplicateOf: "first" };
    expect(
      ptFilterRuns([first, last, next, duplicate], {
        ...filters,
        from: "2026-09-08",
        to: "2026-09-08",
        tag: "fast",
        squad: "2",
      }).map((run) => run.id),
    ).toEqual(["last", "first"]);
    expect(ptFilterRuns([first], { ...filters, from: "2026-09-09", to: "2026-09-08" })).toEqual([]);
    expect(ptFilterRuns([duplicate], { ...filters, showDuplicates: true })).toHaveLength(1);
    expect(ptFilterRuns([{ ...first, bugged: true }], { ...filters, status: "eligible" })).toEqual(
      [],
    );
    expect(ptFilterRuns([first], { ...filters, from: "2026-02-31" })).toHaveLength(1);
  });

  it("formats finite clocks at rounded minute boundaries", () => {
    expect(formatPtTime(59.9999)).toBe("1:00.000");
    expect(formatPtTime(112.971)).toBe("1:52.971");
    expect(formatPtTime(NaN)).toBe("—");
    expect(formatPtLength(41.5)).toBe("41.500s");
    expect(formatPtLength(112.971)).toBe("1:52.971");
    expect(formatPtLength(null)).toBe("—");
    expect(formatPtLength(NaN)).toBe("—");
    expect(formatPtTime(Infinity)).toBe("—");
  });
});
