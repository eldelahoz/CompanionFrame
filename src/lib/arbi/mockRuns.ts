import type { ArbiRun, ArbiRunStats } from "../ipc";
import ptFixture from "../ptRun.fixture.json";
import type { PtRunRecord } from "../ipc";

/** Browser-dev stand-ins for the run stores (mock ipc only; the Tauri build reads SQLite). */
const stats = (rotations: number, withWaves: boolean): ArbiRunStats => {
  const droneTimestamps: number[] = [];
  const rewardTimestamps: number[] = [];
  const pauseIntervals: { start: number; end: number }[] = [];
  let t = 12;
  for (let r = 0; r < rotations; r++) {
    for (let d = 0; d < 9 + (r % 3); d++) {
      droneTimestamps.push(t);
      t += 14 + ((d * 7 + r * 3) % 17);
    }
    rewardTimestamps.push(t);
    pauseIntervals.push({ start: t, end: t + 9 });
    t += 9;
  }
  return {
    killsPerDrone: 4.4,
    avgDroneIntervalSec: 19.6,
    expectedVitusMean: 215,
    expectedVitusStd: 38,
    vitusPerMin: 7.1,
    wavesPerRotation: 3,
    droneTimestamps,
    rewardTimestamps,
    preciseStartSec: 0,
    lastActivitySec: t,
    saturationBuckets: ["0-2", "3-5", "6-8", "9-11", "12-14", "15+"].map((label, i) => ({ minCount: i * 3, label, seconds: [40, 90, 160, 220, 180, 110][i], pct: [5.3, 12, 21.3, 29.3, 24, 14.7][i] })),
    waves: withWaves ? Array.from({ length: rotations * 3 }, (_, i) => ({ index: i + 1, durationSec: 18 + ((i * 5) % 14), saturationPct: 20 + ((i * 11) % 60) })) : null,
    pauseIntervals,
    idleIntervals: [],
    rotationSaturationPct: rewardTimestamps.map((_, i) => 40 + ((i * 13) % 45)),
    spawnPoints: [],
    spawnDataVersion: 2,
  };
};

const base = {
  endedAt: 0,
  solNode: null,
  missionTypeRaw: null,
  hasLog: true,
  logSizeBytes: 48_213,
  source: "live" as const,
  tags: [] as string[],
  players: ["Tenno"],
  notes: null,
  duplicateOf: null,
};

export const MOCK_ARBI_RUNS = (): ArbiRun[] => [
  { ...base, id: "2026-05-30_20-11-02", startedAt: 1780171862000, missionName: "Arbitration: Casta Defense (Ceres)", node: "Casta (Ceres)", missionType: "defense", durationSec: 1480, rotations: 8, drones: 82, totalEnemies: 3300, vitusActual: 231, endReason: "mission-end", tags: ["boar run"], stats: stats(8, true) },
  { ...base, id: "2026-05-29_19-02-40", startedAt: 1780081360000, missionName: "Arbitration: Hydron Defense (Sedna)", node: "Hydron (Sedna)", missionType: "defense", durationSec: 1120, rotations: 6, drones: 58, totalEnemies: 2400, vitusActual: 168, endReason: "mission-end", stats: stats(6, true) },
  { ...base, id: "2026-05-28_21-40-15", startedAt: 1779997215000, missionName: "Arbitration: Cinxia Interception (Ceres)", node: "Cinxia (Ceres)", missionType: "interception", durationSec: 900, rotations: 5, drones: 44, totalEnemies: 1800, vitusActual: null, endReason: "aborted", source: "imported", stats: stats(5, false) },
  { ...base, id: "2026-05-27_18-00-00", startedAt: 1779904800000, missionName: "Arbitration: Oestrus (Eris)", node: "Oestrus (Eris)", missionType: "other", durationSec: 300, rotations: 1, drones: 0, totalEnemies: 120, vitusActual: null, endReason: "imported", source: "imported", hasLog: false, logSizeBytes: 0, stats: null },
];

const pt = (id: string, startedAt: number, scale: number, extra: Partial<PtRunRecord> = {}): PtRunRecord => {
  const f = ptFixture as unknown as PtRunRecord;
  const s = (v: number) => v * scale;
  return {
    ...f,
    id,
    startedAt,
    endedAt: startedAt + 120_000,
    durationSec: s(f.durationSec),
    flightSec: s(f.flightSec),
    shieldSec: s(f.shieldSec),
    legSec: s(f.legSec),
    bodySec: s(f.bodySec),
    pylonSec: s(f.pylonSec),
    phases: f.phases.map((p) => ({
      ...p,
      totalSec: s(p.totalSec),
      shieldSec: s(p.shieldSec),
      legSec: s(p.legSec),
      bodySec: s(p.bodySec),
      pylonSec: s(p.pylonSec),
      shields: p.shields.map((x) => ({ ...x, seconds: s(x.seconds) })),
      legs: p.legs.map((x) => ({ ...x, seconds: s(x.seconds) })),
    })),
    ...extra,
  };
};

export const MOCK_PT_RUNS = (): PtRunRecord[] => [
  pt("2026-05-30_22-01-00", 1780178460000, 1.04),
  pt("2026-05-29_21-10-30", 1780089030000, 0.97, { tags: ["solo"] }),
  pt("2026-05-28_15-28-09", 1780000089248, 1),
  pt("2026-05-27_12-00-00", 1779883200000, 1.2, { bugged: true }),
];
