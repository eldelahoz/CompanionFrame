import { describe, expect, it } from "vitest";
import { buildNodeCatalog, buildSearchState, factionBadgeKey, filterScheduleEntries, formatScheduleCountdown, groupEntriesByDay } from "./arbiScheduleData";

const e = (epochMs: number, nodeId: string, node: string, faction = "Grineer") => ({ epochMs, nodeId, node, mission: "Defense", faction });
const entries = [e(2_000, "b", "Hydron (Sedna)"), e(1_000, "a", "Casta (Ceres)", "Corpus"), e(3_000, "a", "Casta (Ceres)", "Corpus")];

describe("arbi schedule data", () => {
  it("lists each node once, sorted by name", () => {
    expect(buildNodeCatalog(entries).map((n) => n.id)).toEqual(["a", "b"]);
  });
  it("filters by selection and search, oldest first", () => {
    const catalog = buildNodeCatalog(entries);
    const s = buildSearchState("casta", catalog);
    expect(filterScheduleEntries(entries, new Set(), s, 30, 0).map((x) => x.epochMs)).toEqual([1_000, 3_000]);
    expect(filterScheduleEntries(entries, new Set(["b"]), buildSearchState("", catalog), 30, 0).map((x) => x.nodeId)).toEqual(["b"]);
  });
  it("names search tokens that match no node", () => {
    expect(buildSearchState("casta zzz", buildNodeCatalog(entries)).unmatchedTokens).toEqual(["zzz"]);
  });
  it("counts down and says NOW once started", () => {
    expect(formatScheduleCountdown(90_000, 0)).toBe("0h 01m 30s");
    expect(formatScheduleCountdown(0, 5)).toBe("NOW");
  });
  it("groups by local day and maps factions", () => {
    expect(groupEntriesByDay(entries, "en")).toHaveLength(1);
    expect(factionBadgeKey("Orokin")).toBe("corrupted");
  });
});
