import { formatInt } from "../format";
import { toFiniteNumber } from "../../../config/shared/numeric";
import { DAY_MS, nextDailyResetUtc, nextWeeklyResetUtc, WEEK_MS } from "../format";
import type { MessageKey } from "../i18n";
import type { RawInventoryData } from "../../types/inventory";
import type { WorldState } from "../ipc";

/** Derives daily/weekly completion from the raw DE inventory payload. Rules only
 *  ever ADD doneness: each signal is scoped to the current period (world-state
 *  ids, reset dates, week counts, file mtime), so stale data yields "not done", never a
 *  false "done"; the user can still tick manually. */
interface AutoTask {
  /** Completed runs; for checkbox tasks 1 means done. */
  count: number;
  /** Live sub-line, translated at the render site. */
  detail?: { key: MessageKey; params: Record<string, string> };
  /** Partial progress toward a requirement (nightwave acts). */
  progress?: { current: number; required: number };
}

type AutoState = Record<string, AutoTask>;

/** Mongo-extended date ({$date:{$numberLong}}) to epoch ms; DE boxes the inner
 *  value under several $number* names, so it is unwrapped generically. */
function deDateMs(value: unknown): number | null {
  if (!value || typeof value !== "object") return null;
  const ms = toFiniteNumber((value as { $date?: unknown }).$date);
  return ms !== null && ms > 0 ? ms : null;
}

function oidOf(value: unknown): string | null {
  if (!value || typeof value !== "object") return null;
  const oid = (value as { $oid?: unknown }).$oid;
  return typeof oid === "string" && oid.length > 0 ? oid : null;
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

/** LastSortieReward keeps past runs, so only the live world-state id dates one. */
function rewardMatches(rewards: unknown, activeId: string | undefined): boolean {
  if (!activeId) return false;
  return asArray(rewards).some((entry) => {
    if (!entry || typeof entry !== "object") return false;
    return oidOf((entry as { SortieId?: unknown }).SortieId) === activeId;
  });
}

/** Kahl WeeklyMissions count weeks from Monday 2014-02-10 00:00 UTC. */
const KAHL_EPOCH_MS = Date.UTC(2014, 1, 10);

/** PeriodicMissionCompletions upserts one entry per tag; the stored date is the
 *  period's activation (window-aligned tags) or the completion time, so either
 *  way a date at or past the current period start means "done this period". */
function periodicMissionsSince(
  inv: RawInventoryData,
  tagPrefix: string,
  periodStartMs: number,
): number {
  let count = 0;
  for (const entry of asArray(inv.PeriodicMissionCompletions)) {
    if (!entry || typeof entry !== "object") continue;
    const record = entry as { tag?: unknown; date?: unknown };
    if (typeof record.tag !== "string" || !record.tag.startsWith(tagPrefix)) continue;
    const ms = deDateMs(record.date);
    if (ms !== null && ms >= periodStartMs) count += 1;
  }
  return count;
}

/** WeeklyMissions retains older weeks, so WeekCount is what scopes the entry. */
function kahlDone(inv: RawInventoryData, nowMs: number): boolean {
  const kahl = asArray(inv.Affiliations).find(
    (entry) =>
      !!entry && typeof entry === "object" && (entry as { Tag?: unknown }).Tag === "KahlSyndicate",
  );
  if (!kahl) return false;
  const currentWeek = Math.floor((nowMs - KAHL_EPOCH_MS) / WEEK_MS);
  return asArray((kahl as { WeeklyMissions?: unknown }).WeeklyMissions).some((mission) => {
    if (!mission || typeof mission !== "object") return false;
    const record = mission as { WeekCount?: unknown; CompletedMission?: unknown };
    return record.WeekCount === currentWeek && record.CompletedMission === true;
  });
}

/** Netracell runs this week; the count is stale once its reset date has passed. */
function netracellCount(inv: RawInventoryData, nowMs: number): number | null {
  const count = toFiniteNumber(inv.EntratiVaultCountLastPeriod);
  if (count === null) return null;
  const resetMs = deDateMs(inv.EntratiVaultCountResetDate);
  if (resetMs === null) return null;
  return resetMs > nowMs ? count : 0;
}

/** Archimedea (conquest) fields share the netracell reset date. The score does
 *  not say which of the three missions are cleared, so it stays informational. */
function conquestScore(inv: RawInventoryData, field: string, nowMs: number): AutoTask | null {
  const score = toFiniteNumber(inv[field]);
  if (score === null || score <= 0) return null;
  const resetMs = deDateMs(inv.EntratiVaultCountResetDate);
  if (resetMs === null || resetMs <= nowMs) return null;
  return {
    count: 0,
    detail: { key: "dailies.conquestScore", params: { score: String(score) } },
  };
}

/** Remaining-pool fields (DailyAffiliation*, DailyFocus): 0 means capped. */
function remainingPool(
  inv: RawInventoryData,
  field: string,
  detailKey: MessageKey,
): AutoTask | null {
  const remaining = toFiniteNumber(inv[field]);
  if (remaining === null) return null;
  const task: AutoTask = { count: remaining === 0 ? 1 : 0 };
  if (remaining > 0) {
    task.detail = { key: detailKey, params: { amount: formatInt(remaining) } };
  }
  return task;
}

/** Simaris daily scan task, reported as progress and never as done. The accepted
 *  task carries no date and survives the reset until it is handed in, so a task
 *  finished yesterday and left unclaimed is indistinguishable from one finished
 *  today. Same call as nightwave. */
function simarisTask(inv: RawInventoryData): AutoTask | null {
  const info = inv.LibraryActiveDailyTaskInfo;
  if (!info || typeof info !== "object") return null;
  const scans = toFiniteNumber((info as { Scans?: unknown }).Scans);
  const required = toFiniteNumber((info as { ScansRequired?: unknown }).ScansRequired);
  if (required === null || required <= 0 || scans === null || scans < 0) return null;

  return {
    count: 0,
    detail: {
      key: "dailies.simarisScans",
      params: { scans: String(Math.min(scans, required)), required: String(required) },
    },
  };
}

// SeasonChallengeHistory logs instantiated acts, not completed ones: entries
// appear with ChallengeProgress 0 and far outnumber what the season standing
// accounts for. It must never tick a checkbox, so acts stay manual.
function nightwaveTasks(inv: RawInventoryData, wd: WorldState, out: AutoState): void {
  const acts = wd.nightwave?.challenges ?? [];
  if (acts.length === 0) return;

  const progressByName = new Map<string, number>();
  for (const entry of asArray(inv.ChallengeProgress)) {
    if (!entry || typeof entry !== "object") continue;
    const record = entry as { Name?: unknown; Progress?: unknown };
    const progress = toFiniteNumber(record.Progress);
    if (typeof record.Name === "string" && progress !== null) {
      progressByName.set(record.Name, progress);
    }
  }

  for (const act of acts) {
    if (!act.id) continue;
    const task: AutoTask = { count: 0 };
    const progress = act.name ? progressByName.get(act.name) : undefined;
    // At/above the requirement is unreliable either way: completion zeroes the
    // counter and recurring act names keep counts from earlier appearances.
    // Zero is equally ambiguous (fresh act or just-completed), so no line.
    if (
      act.requiredCount > 1 &&
      progress !== undefined &&
      progress > 0 &&
      progress < act.requiredCount
    ) {
      task.progress = { current: progress, required: act.requiredCount };
    }
    out[`nw:${act.id}`] = task;
  }
}

/** Season standing straight from the syndicate entry, the only Nightwave
 *  completion number DE reports that can be trusted. */
export function nightwaveSeasonStanding(
  inv: RawInventoryData | null,
  affiliationTag: string | undefined,
): number | null {
  if (!inv || !affiliationTag) return null;
  for (const entry of asArray(inv.Affiliations)) {
    if (!entry || typeof entry !== "object") continue;
    const record = entry as { Tag?: unknown; Standing?: unknown };
    if (record.Tag !== affiliationTag) continue;
    return toFiniteNumber(record.Standing);
  }
  return null;
}

/**
 * Auto progress per task id from the last inventory sync; empty without one.
 * Task ids missing from the result have no automation signal at all.
 */
export function autoTrackerState(
  inv: RawInventoryData | null,
  wd: WorldState | null,
  nowMs: number,
  inventoryModifiedAt: number | null,
): AutoState {
  const out: AutoState = {};
  if (!inv) return out;

  if (rewardMatches(inv.LastSortieReward, wd?.sortie?.id)) out.sortie = { count: 1 };
  if (rewardMatches(inv.LastLiteSortieReward, wd?.archonHunt?.id)) out.archonHunt = { count: 1 };

  const netracells = netracellCount(inv, nowMs);
  if (netracells !== null) out.netracells = { count: netracells };

  const deep = conquestScore(inv, "EntratiLabConquestCacheScoreMission", nowMs);
  if (deep) out.deepArchimedea = deep;
  const temporal = conquestScore(inv, "EchoesHexConquestCacheScoreMission", nowMs);
  if (temporal) out.temporalArchimedea = temporal;

  const now = new Date(nowMs);
  const weekStartMs = nextWeeklyResetUtc(now).getTime() - WEEK_MS;
  const dayStartMs = nextDailyResetUtc(now).getTime() - DAY_MS;

  // These three fields are point-in-time with no date of their own, so only the
  // file's mtime can place them in today's period; without it they say nothing.
  if (inventoryModifiedAt !== null && inventoryModifiedAt >= dayStartMs) {
    const simaris = simarisTask(inv);
    if (simaris) out.simaris = simaris;

    const standing = remainingPool(inv, "DailyAffiliation", "dailies.standingLeft");
    if (standing) out.syndicateStanding = standing;
    const focus = remainingPool(inv, "DailyFocus", "dailies.focusLeft");
    if (focus) out.dailyFocus = focus;
  }

  if (periodicMissionsSince(inv, "GetClem", weekStartMs) > 0) out.clem = { count: 1 };
  // Maroo's hunt rotates through TreasureHunt[A-G] variant tags.
  if (periodicMissionsSince(inv, "TreasureHunt", weekStartMs) > 0) out.ayatanHunt = { count: 1 };
  // One HardDaily<N> tag per Steel Path incursion alert, five per day.
  const incursions = periodicMissionsSince(inv, "HardDaily", dayStartMs);
  if (incursions > 0) out.spIncursions = { count: Math.min(incursions, 5) };

  if (kahlDone(inv, nowMs)) out.kahl = { count: 1 };

  if (wd) nightwaveTasks(inv, wd, out);

  return out;
}
