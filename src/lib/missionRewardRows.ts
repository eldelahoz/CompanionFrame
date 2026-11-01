import { fallbackNameFromUniqueName, sanitizeDisplayName } from "../../config/shared/displayName";
import { titleCase } from "../../config/shared/textNormalize";
import type { ItemDatabase, MissionRewardItem, MissionRewardSummary, MissionRewardsStatus } from "./ipc";
import type { MessageKey, Translator } from "./i18n";

export interface RewardRow {
  uniqueName: string;
  name: string;
  imageUrl: string | null;
  count: number;
  vaulted: boolean;
  ducats: number | null;
}

/** DE's own meanings for the MT_ enums the log names. */
const MISSION_TYPE_LABELS: Readonly<Record<string, string>> = {
  MT_ALCHEMY: "Alchemy",
  MT_ARENA: "Arena",
  MT_ARMAGEDDON: "Void Armageddon",
  MT_ARTIFACT: "Disruption",
  MT_ASSASSINATION: "Assassination",
  MT_ASSAULT: "Assault",
  MT_CAPTURE: "Capture",
  MT_CORRUPTION: "Void Flood",
  MT_COUNTER_INTEL: "Deception",
  MT_DEFENSE: "Defense",
  MT_ENDLESS_CAPTURE: "Legacyte Harvest",
  MT_ENDLESS_EXTERMINATION: "Sanctuary Onslaught",
  MT_EVACUATION: "Defection",
  MT_EXCAVATE: "Excavation",
  MT_EXTERMINATION: "Exterminate",
  MT_HIVE: "Hive",
  MT_INTEL: "Spy",
  MT_JUNCTION: "Solar Rail Junction",
  MT_LANDSCAPE: "Free Roam",
  MT_MOBILE_DEFENSE: "Mobile Defense",
  MT_PURIFY: "Infested Salvage",
  MT_PURSUIT: "Pursuit",
  MT_PVP: "Conclave",
  MT_RACE: "Rush",
  MT_RAILJACK: "Railjack",
  MT_RESCUE: "Rescue",
  MT_RETRIEVAL: "Hijack",
  MT_SABOTAGE: "Sabotage",
  MT_SURVIVAL: "Survival",
  MT_TERRITORY: "Interception",
  MT_VAULTS: "Netracell",
  MT_VOID_CASCADE: "Void Cascade",
};

export function missionTypeLabel(missionType: string | null | undefined): string | null {
  if (!missionType) return null;
  return MISSION_TYPE_LABELS[missionType] ?? titleCase(missionType.slice(3).replace(/_/g, " "));
}

/** The node's name when the star chart knows it, else the mission type. */
export function missionName(s: MissionRewardSummary, nodeLabel: string | null, unknown: string): string {
  return nodeLabel ?? missionTypeLabel(s.missionType) ?? unknown;
}

/** Most valuable first: ducats, then name. */
export function buildRewardRows(items: readonly MissionRewardItem[], db: ItemDatabase): RewardRow[] {
  return items
    .map((item) => {
      const entry = db[item.uniqueName];
      const ducats = typeof entry?.ducats === "number" && entry.ducats > 0 ? entry.ducats * item.count : null;
      return {
        uniqueName: item.uniqueName,
        name: sanitizeDisplayName(entry?.displayName || entry?.name) || fallbackNameFromUniqueName(item.uniqueName),
        imageUrl: entry?.imageUrl ?? null,
        count: item.count,
        vaulted: entry?.vaulted === true,
        ducats,
      };
    })
    .sort((a, b) => (b.ducats ?? -1) - (a.ducats ?? -1) || a.name.localeCompare(b.name));
}

export const rewardRowDucats = (rows: readonly RewardRow[]): number => rows.reduce((sum, r) => sum + (r.ducats ?? 0), 0);

/** Recorded item types whose shown or English name contains the search text. */
export function matchRewardItemTypes(itemTypes: readonly string[], search: string, db: ItemDatabase): string[] {
  const needle = search.trim().toLocaleLowerCase();
  if (!needle) return [];
  return itemTypes.filter((uniqueName) => {
    const entry = db[uniqueName];
    const names = [entry?.displayName, entry?.name, entry ? undefined : fallbackNameFromUniqueName(uniqueName)];
    return names.some((name) => name?.toLocaleLowerCase().includes(needle));
  });
}

const FAILURE_DETAIL_KEYS: Partial<Record<NonNullable<MissionRewardsStatus["lastFailure"]>, MessageKey>> = {
  "game-not-running": "titlebar.tooltip.gameNotRunning",
  "access-denied": "titlebar.tooltip.accessDenied",
  "no-fresh-copy": "dashboard.lastMission.noFreshCopy",
};

/** Tracking off, then a read in progress, then the last failed read with its cause. */
export function missionStatusText(status: MissionRewardsStatus | null, t: Translator): string | null {
  if (!status) return null;
  if (status.blocked === "tracking-off") return t("dashboard.lastMission.trackingOff");
  if (status.phase !== "idle") return t("dashboard.lastMission.reading");
  if (!status.lastFailure) return null;
  const detail = FAILURE_DETAIL_KEYS[status.lastFailure];
  return detail ? `${t("dashboard.lastMission.readFailed")} ${t(detail)}` : t("dashboard.lastMission.readFailed");
}

export const MISSION_PERIODS = ["today", "7d", "30d", "all"] as const;
export type MissionPeriod = (typeof MISSION_PERIODS)[number];

const DAY_MS = 86_400_000;

export function localMidnight(now: number): number {
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);
  return midnight.getTime();
}

/** Earliest endedAt a period covers, in local time; null for all time. */
export function missionPeriodStart(period: MissionPeriod, now: number): number | null {
  if (period === "all") return null;
  if (period === "today") return localMidnight(now);
  return now - (period === "7d" ? 7 : 30) * DAY_MS;
}

export function endedAtLabel(endedAt: number, code: string): string {
  return new Date(endedAt).toLocaleString(code, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

/** A later page after the loaded rows; a mission recorded since the last request shifts the offsets,
 *  so the page can repeat rows already loaded. */
export function appendPage<T extends { id: string }>(loaded: readonly T[], next: readonly T[]): T[] {
  const ids = new Set(loaded.map((entry) => entry.id));
  return [...loaded, ...next.filter((entry) => !ids.has(entry.id))];
}

/** Lays a fresh first page over a longer loaded list, keeping every row already loaded; starts over
 *  only when the two no longer overlap. */
export function mergeFirstPage<T extends { id: string }>(loaded: readonly T[], first: readonly T[], matched: number): T[] {
  const firstIds = new Set(first.map((entry) => entry.id));
  const overlaps = first.length >= matched || loaded.some((entry) => firstIds.has(entry.id));
  if (!overlaps) return [...first];
  return [...first, ...loaded.filter((entry) => !firstIds.has(entry.id))].slice(0, matched);
}

export type PageLoadMode = "replace" | "append" | "merge";

interface LoadedPage<T> {
  summaries: readonly T[];
  matched: number;
}

interface PageLoaderTarget<T, P> {
  rows: () => readonly T[];
  show: (rows: T[], page: P | null) => void;
  /** `rowsOutdated`: the rows shown were loaded under an earlier filter. */
  fail: (error: unknown, rowsOutdated: boolean) => void;
}

/** Orders a paged list's loads: a filter change drops every older answer, a first page drops those
 *  older than the one shown, and rows loaded under an earlier filter are replaced, never merged into
 *  or appended to. */
export function createPageLoader<T extends { id: string }, P extends LoadedPage<T>>(
  target: PageLoaderTarget<T, P>,
): (mode: PageLoadMode, load: () => Promise<P | null>) => Promise<void> {
  let filterSeq = 0;
  let firstSent = 0;
  let firstShown = 0;
  let rowsFilter = 0;
  return async (mode, load) => {
    if (mode === "replace") filterSeq += 1;
    const filters = filterSeq;
    const first = mode === "append" ? firstSent : ++firstSent;
    const base = rowsFilter;
    const stale = (): boolean => filters !== filterSeq || (mode === "append" ? base !== filters : first <= firstShown);
    try {
      const next = await load();
      if (stale()) return;
      if (!next) throw new Error("mission page query rejected");
      let rows: T[];
      if (mode === "append") {
        rows = appendPage(target.rows(), next.summaries);
      } else {
        rows = mode === "merge" && rowsFilter === filters ? mergeFirstPage(target.rows(), next.summaries, next.matched) : [...next.summaries];
        firstShown = first;
        rowsFilter = filters;
      }
      target.show(rows, first >= firstShown ? next : null);
    } catch (error: unknown) {
      if (stale()) return;
      target.fail(error, rowsFilter !== filters);
    }
  };
}
