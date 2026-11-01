import { describe, expect, it } from "vitest";
import { cycleTimeDisplay, nextWeeklyResetUtc, timeTo, timeToStrict } from "../format";
import { translator } from "../i18n";
import { mockWorldState } from "./fixture";
import {
  buildBountyGroups,
  buildCycleRows,
  buildFissureRows,
  buildResetUrgency,
  buildWorldTimes,
  fissureTierClass,
  isUrgent,
} from "./view";

const NOW = Date.UTC(2026, 9, 25, 12, 0, 0);
const wd = mockWorldState(NOW);
const tr = translator("en");

describe("buildFissureRows", () => {
  const rows = (mode: Parameters<typeof buildFissureRows>[1]) => buildFissureRows(wd.fissures, mode, NOW, NOW);

  it("splits normal, steel path and railjack, tiers first then soonest expiry", () => {
    expect(rows("normal").map((f) => f.tier)).toEqual(["Lith", "Lith", "Meso", "Neo", "Axi", "Requiem"]);
    expect(rows("normal").slice(0, 2).map((f) => f.node)).toEqual(["Cinxia, Ceres", "Hydron, Sedna"]);
    expect(rows("steel").every((f) => f.isHard && !f.isStorm)).toBe(true);
    expect(rows("railjack").map((f) => f.sourceMode)).toEqual(["railjack"]);
  });

  it("all keeps void storms in their own tab", () => {
    expect(rows("all")).toHaveLength(8);
    expect(rows("all").some((f) => f.isStorm)).toBe(false);
  });

  it("drops expired fissures and those inside the expiry guard", () => {
    const soon = [{ ...wd.fissures[0], expiry: new Date(NOW + 1000).toISOString() }, { ...wd.fissures[1], expired: true }];
    expect(buildFissureRows(soon, "all", NOW, NOW)).toEqual([]);
  });

  it("maps tiers to badge classes", () => {
    expect(fissureTierClass("Requiem")).toBe("requiem");
    expect(fissureTierClass("???")).toBe("default");
  });
});

describe("cycles and timers", () => {
  const times = buildWorldTimes(wd, true, true, NOW);

  it("builds one row per known cycle with the next state", () => {
    const rows = buildCycleRows(wd, times, NOW, tr);
    expect(rows.map((r) => r.key)).toEqual(["earth", "cetus", "vallis", "cambion", "duviri"]);
    expect(rows[0]).toMatchObject({ stateLabel: "Day", nextLabel: "Night", stateClass: "day" });
    expect(rows[2]).toMatchObject({ stateClass: "cold", nextLabel: "Warm" });
    expect(rows[3]).toMatchObject({ stateClass: "fass", nextLabel: "Vome" });
    expect(rows[4]).toMatchObject({ stateLabel: "Joy", nextLabel: "Anger" });
  });

  it("omits cycles the backend could not resolve", () => {
    const rows = buildCycleRows({ ...wd, cetusCycle: null, cambionCycle: null, duviriCycle: null }, times, NOW, tr);
    expect(rows.map((r) => r.key)).toEqual(["earth", "vallis"]);
  });

  it("counts down to Baro's departure while he is here", () => {
    expect(times.baro).toBe("1d 4h");
    // Not active: the countdown targets the arrival, which already passed in the fixture.
    expect(buildWorldTimes(wd, false, true, NOW).baro).toBe("Refreshing...");
  });

  it("flags the last fifth of a window as urgent", () => {
    const start = new Date(NOW - 80 * 60_000).toISOString();
    expect(isUrgent(new Date(NOW + 10 * 60_000).toISOString(), start, undefined, NOW)).toBe(true);
    expect(isUrgent(new Date(NOW + 60 * 60_000).toISOString(), start, undefined, NOW)).toBe(false);
    expect(isUrgent(null, null, 1000, NOW)).toBe(false);
  });

  it("marks a weekly reset within a day of Monday as urgent", () => {
    // The reset instants come from the real clock; only the comparison clock is injected.
    const sunday = nextWeeklyResetUtc().getTime() - 3_600_000;
    expect(buildResetUrgency(wd, sunday).weekly).toBe(true);
    expect(buildResetUrgency(wd, sunday - 6 * 86_400_000).weekly).toBe(false);
  });
});

describe("bounty groups", () => {
  it("orders syndicates like the in-game list and drops empty ones", () => {
    const [a, b] = wd.bounties;
    const empty = { ...a, jobs: [] };
    expect(buildBountyGroups([b, a, empty]).map((g) => g.syndicate)).toEqual(["Ostrons", "The Holdfasts"]);
    expect(buildBountyGroups(undefined)).toEqual([]);
  });
});

describe("format", () => {
  it("shows days past 24h and seconds below", () => {
    expect(timeTo(new Date(NOW + 30 * 3_600_000), NOW)).toBe("1d 6h");
    expect(timeTo(new Date(NOW + 3_723_000), NOW)).toBe("1h 2m 3s");
    expect(timeToStrict(new Date(NOW + 61_000), NOW)).toBe("1m 1s");
    expect(timeTo(new Date(NOW - 1), NOW)).toBe("Refreshing...");
    expect(timeTo(null, NOW)).toBe("N/A");
  });

  it("prefers the expiry over the API's own countdown text", () => {
    expect(cycleTimeDisplay("5m 3s", new Date(NOW + 61_000).toISOString(), NOW)).toBe("1m 1s");
    expect(cycleTimeDisplay("5m 3s", null, NOW)).toBe("5m 3s");
    expect(cycleTimeDisplay("0m 0s", null, NOW)).toBe("N/A");
  });
});
