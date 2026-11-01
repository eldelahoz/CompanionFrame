import { describe, expect, it } from "vitest";

import { autoTrackerState, nightwaveSeasonStanding } from "../../lib/world/dailiesAuto";
import type { RawInventoryData } from "../../types/inventory";
import type { WorldState } from "../ipc";

const NOW = Date.parse("2026-08-24T12:00:00Z");
const FUTURE = Date.parse("2026-08-31T00:00:00Z");
const PAST = Date.parse("2026-08-17T00:00:00Z");
/** Inventory mtimes either side of the 2026-08-24T00:00Z daily reset. */
const FRESH = Date.parse("2026-08-24T06:00:00Z");
const STALE = Date.parse("2026-08-23T23:00:00Z");

function deDate(ms: number): { $date: { $numberLong: string } } {
  return { $date: { $numberLong: String(ms) } };
}

function inv(overrides: Record<string, unknown>): RawInventoryData {
  return overrides as RawInventoryData;
}

function world(overrides: Record<string, unknown>): WorldState {
  return overrides as unknown as WorldState;
}

/** An accepted task is a copy of the offer plus Scans. */
function libraryTask(
  enemy: string,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    EnemyTypes: [`/Lotus/Types/Enemies/${enemy}Avatar`],
    EnemyLocTag: `/Lotus/Language/Game/${enemy}`,
    ScansRequired: 4,
    RewardStoreItem: "/Lotus/StoreItems/Upgrades/Mods/FusionBundles/UncommonFusionBundle",
    RewardQuantity: 10,
    RewardStanding: 10000,
    ...overrides,
  };
}

describe("autoTrackerState", () => {
  it("returns nothing without an inventory", () => {
    expect(autoTrackerState(null, world({}), NOW, FRESH)).toEqual({});
  });

  it("returns nothing for an inventory without any tracked fields", () => {
    expect(autoTrackerState(inv({ Suits: [] }), null, NOW, FRESH)).toEqual({});
  });

  it("marks the sortie done only when the reward id matches the active sortie", () => {
    const payload = inv({
      LastSortieReward: [{ SortieId: { $oid: "abc123" }, StoreItem: "x" }],
    });
    expect(
      autoTrackerState(payload, world({ sortie: { id: "abc123" } }), NOW, FRESH).sortie,
    ).toEqual({
      count: 1,
    });
    expect(
      autoTrackerState(payload, world({ sortie: { id: "other" } }), NOW, FRESH).sortie,
    ).toBeUndefined();
    expect(autoTrackerState(payload, null, NOW, FRESH).sortie).toBeUndefined();
  });

  it("marks the archon hunt done from LastLiteSortieReward", () => {
    const payload = inv({
      LastLiteSortieReward: [{ SortieId: { $oid: "lite1" } }],
    });
    const wd = world({
      archonHunt: { id: "lite1", activation: null, expiry: null, boss: "Nira", missions: [] },
    });
    expect(autoTrackerState(payload, wd, NOW, FRESH).archonHunt).toEqual({ count: 1 });
  });

  it("counts netracell runs only while their reset date is ahead", () => {
    const current = inv({
      EntratiVaultCountLastPeriod: 3,
      EntratiVaultCountResetDate: deDate(FUTURE),
    });
    expect(autoTrackerState(current, null, NOW, FRESH).netracells).toEqual({ count: 3 });

    const stale = inv({
      EntratiVaultCountLastPeriod: 3,
      EntratiVaultCountResetDate: deDate(PAST),
    });
    expect(autoTrackerState(stale, null, NOW, FRESH).netracells).toEqual({ count: 0 });

    // Without the reset date the count's week is unknown, so no signal at all.
    expect(
      autoTrackerState(inv({ EntratiVaultCountLastPeriod: 3 }), null, NOW, FRESH).netracells,
    ).toBeUndefined();
  });

  // Never auto-ticked. An accepted task has no date and survives the daily
  // reset until it is handed in, so a full count could belong to any past day.
  it("reports the Simaris scan count without ever calling the daily done", () => {
    const offer = libraryTask("OrokinHealingAncient");

    const full = inv({
      LibraryActiveDailyTaskInfo: libraryTask("OrokinHealingAncient", { Scans: 4 }),
      LibraryAvailableDailyTaskInfo: offer,
    });
    expect(autoTrackerState(full, null, NOW, FRESH).simaris).toEqual({
      count: 0,
      detail: { key: "dailies.simarisScans", params: { scans: "4", required: "4" } },
    });

    const partial = inv({
      LibraryActiveDailyTaskInfo: libraryTask("OrokinHealingAncient", { Scans: 2 }),
      LibraryAvailableDailyTaskInfo: offer,
    });
    expect(autoTrackerState(partial, null, NOW, FRESH).simaris).toEqual({
      count: 0,
      detail: { key: "dailies.simarisScans", params: { scans: "2", required: "4" } },
    });
  });

  it("keeps a finished target the Sanctuary no longer offers out of the done count", () => {
    const stale = inv({
      LibraryActiveDailyTaskInfo: libraryTask("OrokinBladeSawman", { Scans: 4 }),
      LibraryAvailableDailyTaskInfo: libraryTask("OrokinHealingAncient"),
    });
    expect(autoTrackerState(stale, null, NOW, FRESH).simaris?.count).toBe(0);
  });

  it("caps the reported scans at the requirement", () => {
    const over = inv({
      LibraryActiveDailyTaskInfo: libraryTask("OrokinBladeSawman", { Scans: 9 }),
    });
    expect(autoTrackerState(over, null, NOW, FRESH).simaris?.detail?.params).toEqual({
      scans: "4",
      required: "4",
    });
  });

  it("ignores a task record with no scan count", () => {
    const noScans = inv({ LibraryActiveDailyTaskInfo: { ScansRequired: 4 } });
    expect(autoTrackerState(noScans, null, NOW, FRESH).simaris).toBeUndefined();
  });

  it("derives standing and focus caps from the remaining pools", () => {
    const capped = autoTrackerState(inv({ DailyAffiliation: 0, DailyFocus: 0 }), null, NOW, FRESH);
    expect(capped.syndicateStanding).toEqual({ count: 1 });
    expect(capped.dailyFocus).toEqual({ count: 1 });

    const open = autoTrackerState(
      inv({ DailyAffiliation: 27000, DailyFocus: 250000 }),
      null,
      NOW,
      FRESH,
    );
    expect(open.syndicateStanding).toEqual({
      count: 0,
      detail: { key: "dailies.standingLeft", params: { amount: (27000).toLocaleString() } },
    });
    expect(open.dailyFocus?.count).toBe(0);
    expect(open.dailyFocus?.detail?.key).toBe("dailies.focusLeft");
  });

  it("drops the undated daily signals when the inventory file predates the reset", () => {
    const payload = inv({
      DailyAffiliation: 0,
      DailyFocus: 0,
      LibraryActiveDailyTaskInfo: { ScansRequired: 4, Scans: 4 },
    });

    const stale = autoTrackerState(payload, null, NOW, STALE);
    expect(stale.syndicateStanding).toBeUndefined();
    expect(stale.dailyFocus).toBeUndefined();
    expect(stale.simaris).toBeUndefined();
  });

  it("drops the undated daily signals when the inventory has no mtime at all", () => {
    const payload = inv({
      DailyAffiliation: 27000,
      DailyFocus: 250000,
      LibraryActiveDailyTaskInfo: { ScansRequired: 4, Scans: 1 },
    });

    const undated = autoTrackerState(payload, null, NOW, null);
    expect(undated.syndicateStanding).toBeUndefined();
    expect(undated.dailyFocus).toBeUndefined();
    expect(undated.simaris).toBeUndefined();
  });

  it("keeps the dated signals regardless of the inventory mtime", () => {
    const payload = inv({
      EntratiVaultCountLastPeriod: 3,
      EntratiVaultCountResetDate: deDate(FUTURE),
      PeriodicMissionCompletions: [
        { date: deDate(Date.parse("2026-08-24T09:00:00Z")), tag: "HardDaily3" },
      ],
    });

    const stale = autoTrackerState(payload, null, NOW, STALE);
    expect(stale.netracells).toEqual({ count: 3 });
    expect(stale.spIncursions).toEqual({ count: 1 });
  });

  it("unwraps a $numberDouble-boxed reset date", () => {
    const payload = inv({
      EntratiVaultCountLastPeriod: 2,
      EntratiVaultCountResetDate: { $date: { $numberDouble: String(FUTURE) } },
    });

    expect(autoTrackerState(payload, null, NOW, FRESH).netracells).toEqual({ count: 2 });
  });

  it("reads weekly Clem and Ayatan completion from periodic mission records", () => {
    // NOW is Monday 2026-08-24 12:00 UTC, so the week started at 00:00 today.
    const thisWeek = Date.parse("2026-08-24T00:00:00Z");
    const payload = inv({
      PeriodicMissionCompletions: [
        { date: deDate(thisWeek), tag: "GetClem" },
        { date: deDate(PAST), tag: "TreasureHuntD" },
        { date: deDate(thisWeek), tag: "SomethingElse" },
      ],
    });
    const state = autoTrackerState(payload, null, NOW, FRESH);
    expect(state.clem).toEqual({ count: 1 });
    // A date before the current week start belongs to an earlier week.
    expect(state.ayatanHunt).toBeUndefined();
  });

  it("matches every rotating TreasureHunt variant tag", () => {
    const thisWeek = Date.parse("2026-08-24T00:00:00Z");
    const payload = inv({
      PeriodicMissionCompletions: [{ date: deDate(thisWeek), tag: "TreasureHuntB" }],
    });
    expect(autoTrackerState(payload, null, NOW, FRESH).ayatanHunt).toEqual({ count: 1 });
  });

  it("counts today's Steel Path incursions from HardDaily tags", () => {
    const today = Date.parse("2026-08-24T09:00:00Z");
    const yesterday = Date.parse("2026-08-23T09:00:00Z");
    const payload = inv({
      PeriodicMissionCompletions: [
        { date: deDate(today), tag: "HardDaily3" },
        { date: deDate(today), tag: "HardDaily7" },
        { date: deDate(yesterday), tag: "HardDaily12" },
      ],
    });
    expect(autoTrackerState(payload, null, NOW, FRESH).spIncursions).toEqual({ count: 2 });
  });

  it("marks Kahl done only for a completed current-week entry", () => {
    // Weeks count from Monday 2014-02-10 00:00 UTC.
    const currentWeek = Math.floor((NOW - Date.UTC(2014, 1, 10)) / (7 * 86_400_000));
    const done = inv({
      Affiliations: [
        {
          Tag: "KahlSyndicate",
          WeeklyMissions: [{ WeekCount: currentWeek, CompletedMission: true }],
        },
      ],
    });
    expect(autoTrackerState(done, null, NOW, FRESH).kahl).toEqual({ count: 1 });

    const stale = inv({
      Affiliations: [
        {
          Tag: "KahlSyndicate",
          WeeklyMissions: [
            { WeekCount: currentWeek - 43, CompletedMission: true },
            { WeekCount: currentWeek, CompletedMission: false },
          ],
        },
      ],
    });
    expect(autoTrackerState(stale, null, NOW, FRESH).kahl).toBeUndefined();
  });

  it("surfaces the weekly Archimedea research score without ticking the task", () => {
    const payload = inv({
      EntratiLabConquestCacheScoreMission: 12,
      EchoesHexConquestCacheScoreMission: 0,
      EntratiVaultCountResetDate: deDate(FUTURE),
    });
    const state = autoTrackerState(payload, null, NOW, FRESH);
    expect(state.deepArchimedea).toEqual({
      count: 0,
      detail: { key: "dailies.conquestScore", params: { score: "12" } },
    });
    expect(state.temporalArchimedea).toBeUndefined();

    // A passed reset date means the score belongs to an earlier week.
    const stale = inv({
      EntratiLabConquestCacheScoreMission: 12,
      EntratiVaultCountResetDate: deDate(PAST),
    });
    expect(autoTrackerState(stale, null, NOW, FRESH).deepArchimedea).toBeUndefined();
  });

  it("never ticks a nightwave act from the season challenge history", () => {
    // DE logs instantiated acts in the history, not completions: 25 entries can
    // sit against one act's worth of standing, so a hit must not read as done.
    const wd = world({
      nightwave: {
        activation: null,
        expiry: null,
        season: 18,
        phase: 0,
        challenges: [
          {
            id: "act1",
            name: "SeasonDailyAimGlide",
            title: "Glider",
            description: "",
            standing: 1000,
            requiredCount: 15,
            isDaily: true,
            isElite: false,
            activation: null,
            expiry: null,
          },
          {
            id: "act2",
            name: "SeasonWeeklyKillEximus",
            title: "Eximus Eliminator",
            description: "",
            standing: 4500,
            requiredCount: 30,
            isDaily: false,
            isElite: false,
            activation: null,
            expiry: null,
          },
        ],
      },
    });
    const payload = inv({
      SeasonChallengeHistory: [{ challenge: "SeasonDailyAimGlide", id: "act1" }],
      ChallengeProgress: [
        { Name: "SeasonDailyAimGlide", Progress: 0 },
        { Name: "SeasonWeeklyKillEximus", Progress: 7 },
      ],
    });

    const state = autoTrackerState(payload, wd, NOW, FRESH);
    expect(state["nw:act1"]).toEqual({ count: 0 });
    expect(state["nw:act2"]).toEqual({ count: 0, progress: { current: 7, required: 30 } });
  });

  it("reads season standing from the matching syndicate entry", () => {
    const payload = inv({
      Affiliations: [
        { Tag: "RadioLegionIntermission15Syndicate", Standing: 58000 },
        { Tag: "RadioLegionIntermission16Syndicate", Standing: 1000 },
      ],
    });
    expect(nightwaveSeasonStanding(payload, "RadioLegionIntermission16Syndicate")).toBe(1000);
    expect(nightwaveSeasonStanding(payload, "RadioLegionIntermission99Syndicate")).toBeNull();
    expect(nightwaveSeasonStanding(payload, undefined)).toBeNull();
    expect(nightwaveSeasonStanding(null, "RadioLegionIntermission16Syndicate")).toBeNull();
  });

  it("hides challenge progress left over from an earlier appearance of the act", () => {
    const wd = world({
      nightwave: {
        activation: null,
        expiry: null,
        season: 18,
        phase: 0,
        challenges: [
          {
            id: "act3",
            name: "SeasonDailyPickUpMods",
            title: "Collector",
            description: "",
            standing: 1000,
            requiredCount: 8,
            isDaily: true,
            isElite: false,
            activation: null,
            expiry: null,
          },
        ],
      },
    });
    // Progress meets the requirement but the act is absent from the history:
    // the count is from a previous season, so no completion and no progress.
    const payload = inv({
      SeasonChallengeHistory: [],
      ChallengeProgress: [{ Name: "SeasonDailyPickUpMods", Progress: 8 }],
    });

    expect(autoTrackerState(payload, wd, NOW, FRESH)["nw:act3"]).toEqual({ count: 0 });
  });

  it("survives malformed field shapes without throwing", () => {
    const payload = inv({
      LastSortieReward: "garbage",
      PeriodicMissionCompletions: [null, 5, { tag: "GetClem", date: "not-a-date" }],
      EntratiVaultCountLastPeriod: "three",
      SeasonChallengeHistory: { not: "an array" },
      ChallengeProgress: [{ Name: 42, Progress: "x" }],
      DailyAffiliation: null,
      LibraryActiveDailyTaskInfo: "nope",
    });
    expect(autoTrackerState(payload, world({ sortie: { id: "abc" } }), NOW, FRESH)).toEqual({});
  });
});
