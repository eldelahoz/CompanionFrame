// The pure row builders of the world view.
import type { CambionCycle, DayCycle, Fissure, SyndicateBounty, VallisCycle, WorldState } from "../../bindings";
import { cycleTimeDisplay, nextDailyResetUtc, nextWeeklyResetUtc, parseIsoDate, timeTo, timeToStrict } from "../format";
import type { MessageKey } from "../i18n";

export const COARSE_CLOCK_MS = 5_000;
const URGENCY_RATIO = 0.2;
const FISSURE_EXPIRY_GUARD_MS = 1_500;
const MS_24H = 86_400_000;
const MS_7D = 604_800_000;
const FISSURE_TIER_ORDER: Record<string, number> = { lith: 0, meso: 1, neo: 2, axi: 3, requiem: 4, omnia: 5 };
const BOUNTY_ORDER: Record<string, number> = {
  CetusSyndicate: 0,
  Ostrons: 0,
  SolarisSyndicate: 1,
  "Solaris United": 1,
  EntratiSyndicate: 2,
  Entrati: 2,
  ZarimanSyndicate: 3,
  "The Holdfasts": 3,
  EntratiLabSyndicate: 4,
  Cavia: 4,
  HexSyndicate: 5,
  "The Hex": 5,
};

export type FissureMode = "all" | "normal" | "steel" | "railjack";
export const FISSURE_MODES: readonly FissureMode[] = ["all", "normal", "steel", "railjack"];
export const FISSURE_MODE_OPTIONS: ReadonlyArray<{ value: FissureMode; labelKey: MessageKey }> = [
  { value: "all", labelKey: "common.all" },
  { value: "normal", labelKey: "common.normal" },
  { value: "steel", labelKey: "common.steelPath" },
  { value: "railjack", labelKey: "world.railjack" },
];

export const fissureTierClass = (tier = ""): string => {
  const t = tier.toLowerCase();
  return ["lith", "meso", "neo", "axi", "requiem", "omnia"].find((k) => t.includes(k)) ?? "default";
};

/** Active window shorter than 20% of its length reads as urgent. */
export function isUrgent(
  expiryIso: string | null | undefined,
  activationIso: string | null | undefined,
  fallbackTotalMs?: number,
  clock: number = Date.now(),
): boolean {
  const exp = parseIsoDate(expiryIso);
  if (!exp) return false;
  const remainMs = exp.getTime() - clock;
  if (remainMs <= 0) return false;
  const act = parseIsoDate(activationIso);
  const totalMs = act ? exp.getTime() - act.getTime() : (fallbackTotalMs ?? 0);
  if (totalMs <= 0) return false;
  return remainMs / totalMs < URGENCY_RATIO;
}

export function buildWorldTimes(
  wd: WorldState,
  baroActive: boolean,
  varziaActive: boolean,
  nowMs: number,
) {
  const { voidTrader: baro, vaultTrader: varzia, sortie, steelPath, duviriCycle: duviri } = wd;
  const cycle = (c?: { timeLeft?: string | null; expiry?: string | null } | null) => cycleTimeDisplay(c?.timeLeft, c?.expiry, nowMs);
  return {
    baro: timeTo(parseIsoDate(baroActive ? baro?.expiry : baro?.activation), nowMs),
    varzia: timeTo(parseIsoDate(varziaActive ? varzia?.expiry : varzia?.activation), nowMs),
    daily: timeTo(nextDailyResetUtc(), nowMs),
    weekly: timeTo(nextWeeklyResetUtc(), nowMs),
    sortie: timeTo(parseIsoDate(sortie?.expiry) || nextDailyResetUtc(), nowMs),
    steelPath: timeTo(parseIsoDate(steelPath?.expiry) || nextWeeklyResetUtc(), nowMs),
    duviri: timeToStrict(parseIsoDate(duviri?.expiry), nowMs),
    earth: cycle(wd.earthCycle),
    cetus: cycle(wd.cetusCycle),
    vallis: cycle(wd.vallisCycle),
    cambion: cycle(wd.cambionCycle),
  };
}

export type FissureRow = Fissure & { timeStr: string; tierCls: string; sourceMode: Exclude<FissureMode, "all"> };

export function buildFissureRows(
  fissures: Fissure[] | undefined,
  mode: FissureMode,
  nowMs: number,
  nowCoarseMs: number,
): FissureRow[] {
  return (fissures ?? [])
    .filter((f) => {
      if (f.expired) return false;
      if ((parseIsoDate(f.expiry)?.getTime() || 0) <= nowCoarseMs + FISSURE_EXPIRY_GUARD_MS) return false;
      // Railjack mode = all Void Storm fissures; the others show standard fissures split by isHard.
      if (mode === "railjack") return f.isStorm;
      if (f.isStorm) return false;
      if (mode === "all") return true;
      return mode === "steel" ? f.isHard : !f.isHard;
    })
    .sort((a, b) => {
      const oa = FISSURE_TIER_ORDER[a.tier.toLowerCase()] ?? 99;
      const ob = FISSURE_TIER_ORDER[b.tier.toLowerCase()] ?? 99;
      if (oa !== ob) return oa - ob;
      return (parseIsoDate(a.expiry)?.getTime() || 0) - (parseIsoDate(b.expiry)?.getTime() || 0);
    })
    .map((f) => ({
      ...f,
      timeStr: timeToStrict(parseIsoDate(f.expiry), nowMs),
      tierCls: fissureTierClass(f.tier),
      sourceMode: f.isStorm ? "railjack" : f.isHard ? "steel" : "normal",
    }));
}

type CycleKey = "earth" | "cetus" | "vallis" | "cambion" | "duviri";
export interface CycleRowData {
  key: CycleKey;
  time: string;
  stateLabel: string;
  stateClass: string;
  nextLabel: string;
  urgent: boolean;
}

export function buildCycleRows(
  wd: WorldState,
  times: ReturnType<typeof buildWorldTimes>,
  nowCoarseMs: number,
  t: (key: MessageKey) => string,
): CycleRowData[] {
  const day = (key: "earth" | "cetus", c?: DayCycle | null): CycleRowData[] =>
    c
      ? [
          {
            key,
            time: times[key],
            stateLabel: t(c.isDay ? "world.cycle.day" : "world.cycle.night"),
            stateClass: c.isDay ? "day" : "night",
            nextLabel: t(c.isDay ? "world.cycle.night" : "world.cycle.day"),
            urgent: isUrgent(c.expiry, null, undefined, nowCoarseMs),
          },
        ]
      : [];
  const vallis = (c?: VallisCycle | null): CycleRowData[] =>
    c
      ? [
          {
            key: "vallis",
            time: times.vallis,
            stateLabel: t(c.isWarm ? "world.cycle.warm" : "world.cycle.cold"),
            stateClass: c.isWarm ? "warm" : "cold",
            nextLabel: t(c.isWarm ? "world.cycle.cold" : "world.cycle.warm"),
            urgent: isUrgent(c.expiry, null, undefined, nowCoarseMs),
          },
        ]
      : [];
  const cambion = (c?: CambionCycle | null): CycleRowData[] => {
    if (!c) return [];
    const state = c.active.toLowerCase();
    // DE only ever reports fass or vome here; anything else is shown as sent.
    const label =
      state === "fass" ? t("world.cycle.fass") : state === "vome" ? t("world.cycle.vome") : state ? state[0].toUpperCase() + state.slice(1) : t("common.unknown");
    return [
      {
        key: "cambion",
        time: times.cambion,
        stateLabel: label,
        stateClass: state || "fass",
        nextLabel: t(state === "fass" ? "world.cycle.vome" : "world.cycle.fass"),
        urgent: isUrgent(c.expiry, null, undefined, nowCoarseMs),
      },
    ];
  };
  const duviri = wd.duviriCycle;
  const duviriRow: CycleRowData[] = duviri?.expiry
    ? [
        {
          key: "duviri",
          time: times.duviri,
          stateLabel: duviri.state ?? "unknown",
          stateClass: (duviri.state ?? "unknown").toLowerCase(),
          nextLabel: duviri.nextState ?? t("common.unknown"),
          urgent: isUrgent(duviri.expiry, null, undefined, nowCoarseMs),
        },
      ]
    : [];
  return [...day("earth", wd.earthCycle), ...day("cetus", wd.cetusCycle), ...vallis(wd.vallisCycle), ...cambion(wd.cambionCycle), ...duviriRow];
}

export const buildBountyGroups = (bounties: SyndicateBounty[] | undefined): SyndicateBounty[] =>
  (bounties ?? [])
    .filter((b) => b.jobs.length > 0)
    .sort(
      (a, b) =>
        (BOUNTY_ORDER[a.syndicateKey] ?? BOUNTY_ORDER[a.syndicate] ?? 99) -
        (BOUNTY_ORDER[b.syndicateKey] ?? BOUNTY_ORDER[b.syndicate] ?? 99),
    );

export function buildResetUrgency(wd: WorldState, nowCoarseMs: number) {
  const dailyRemaining = nextDailyResetUtc().getTime() - nowCoarseMs;
  const weeklyRemaining = nextWeeklyResetUtc().getTime() - nowCoarseMs;
  return {
    sortie: isUrgent(wd.sortie?.expiry, null, MS_24H, nowCoarseMs),
    daily: dailyRemaining > 0 && dailyRemaining / MS_24H < URGENCY_RATIO,
    weekly: weeklyRemaining > 0 && weeklyRemaining / MS_7D < URGENCY_RATIO,
    steelPath: isUrgent(wd.steelPath?.expiry, null, MS_7D, nowCoarseMs),
  };
}

export const buildBountyTimers = (
  bounties: SyndicateBounty[],
  nowMs: number,
  nowCoarseMs: number,
): Record<string, { timeStr: string; urgent: boolean }> =>
  Object.fromEntries(
    bounties.map((b) => {
      const exp = parseIsoDate(b.expiry);
      return [b.syndicateKey, { timeStr: exp ? timeTo(exp, nowMs) : "", urgent: isUrgent(b.expiry, null, 9_000_000, nowCoarseMs) }];
    }),
  );
