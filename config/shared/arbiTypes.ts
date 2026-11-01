export type ArbiMissionType = "defense" | "interception" | "disruption" | "other";

export type ArbiRunEndReason =
  | "mission-end"
  | "aborted"
  | "new-mission"
  | "log-truncated"
  | "app-quit"
  | "inactivity"
  | "imported";

interface ArbiSaturationBucket {
  /** Lower bound of the enemy-count bucket (buckets are 3 wide, last is open-ended). */
  minCount: number;
  label: string;
  seconds: number;
  pct: number;
}

interface ArbiWaveEntry {
  index: number;
  durationSec: number;
  /** Share of this wave/round at or above the saturation threshold, 0-100.
   * Absent on records written before per-wave saturation existed. */
  saturationPct?: number;
}

/** Waves the parser keeps a separate count for. A floor filter cuts inside this
 * window, so it only has to cover the first few waves of a run. */
export const ARBI_EARLY_WAVE_CAP = 15;

/** One WaveDefend spawn point, aggregated over the run. Engine coordinates: y is up. */
export interface ArbiSpawnPoint {
  /** Full engine path, e.g. "/Layer1/Layer1/NpcSpawnPoint37". */
  id: string;
  x: number;
  y: number;
  z: number;
  count: number;
  /** Spawns in waves 1..ARBI_EARLY_WAVE_CAP, index 0 = wave 1. Absent when the
   * point never fired that early, and on records written before version 2. */
  early?: number[] | null;
}

/** Spawns this point produced from `minWave` on. Without per-wave data (never
 * fired inside the tracked window, or a record predating version 2) the full
 * count stands. Past the window every tracked wave is below `minWave`, so
 * cutting them all is the upper bound and the result never rises with it. */
export function countFromWave(point: ArbiSpawnPoint, minWave: number): number {
  if (!point.early || minWave <= 1) return point.count;
  const lastCutWave = Math.min(minWave - 1, ARBI_EARLY_WAVE_CAP);
  let cut = 0;
  for (let wave = 1; wave <= lastCutWave; wave++) cut += point.early[wave - 1] ?? 0;
  return Math.max(0, point.count - cut);
}

/** Half-open window in game-relative seconds (EE.log float timestamps). */
export interface ArbiInterval {
  start: number;
  end: number;
}

/** Full computed stats for defense/interception runs; null for other mission types. */
export interface ArbiRunStats {
  killsPerDrone: number;
  avgDroneIntervalSec: number | null;
  expectedVitusMean: number;
  expectedVitusStd: number;
  vitusPerMin: number;
  wavesPerRotation: number;
  /** Game-relative seconds (EE.log float timestamps). */
  droneTimestamps: number[];
  /** Rotation reward boundaries, game-relative seconds. */
  rewardTimestamps: number[];
  preciseStartSec: number | null;
  lastActivitySec: number;
  saturationBuckets: ArbiSaturationBucket[];
  /** Defense only (wave clear map); null for interception. */
  waves: ArbiWaveEntry[] | null;
  /** Reward-screen and between-wave downtime. Absent on records written before
   * cadence tracking, which is what hides the timeline for them. */
  pauseIntervals?: ArbiInterval[];
  /** Windows with no AI tick line at all (load screen, host stall, migration). */
  idleIntervals?: ArbiInterval[];
  /** One saturation share (0-100) per rewardTimestamps entry. Absent on records
   * written before rotation saturation existed. */
  rotationSaturationPct?: number[];
  /** Defense only - no other mode logs spawn points. Empty when none were seen. */
  spawnPoints?: ArbiSpawnPoint[];
  /** Shape version of `spawnPoints`; absent on records written before versioning. */
  spawnDataVersion?: number;
}
