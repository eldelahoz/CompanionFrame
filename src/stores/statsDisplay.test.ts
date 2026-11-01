import { afterEach, describe, expect, it, vi } from "vitest";
import { stubStorage as sharedStub } from "../test/storage";
import { DEFAULT_STAT_RESOURCE_IDS, STAT_RESOURCES, statResourceLabelKey } from "../lib/stats/resources";

const STORAGE_KEY = "wf_stats_chart_resources";

function stubStorage(initial?: string): Map<string, string> {
  return sharedStub(initial === undefined ? {} : { [STORAGE_KEY]: initial });
}

async function importStore() {
  vi.resetModules();
  return import("./statsDisplay");
}

afterEach(() => vi.unstubAllGlobals());

describe("stats chart resource prefs", () => {
  it("defaults to the pre-map six plus Kuva", async () => {
    stubStorage();
    expect((await importStore()).getChartResources()).toEqual([...DEFAULT_STAT_RESOURCE_IDS]);
  });

  it("drops stored ids this build does not record", async () => {
    stubStorage(JSON.stringify(["plat", "unobtanium", "kuva", "ayaDelta"]));
    expect((await importStore()).getChartResources()).toEqual(["plat", "kuva"]);
  });

  it("falls back to the defaults on corrupt or non-array prefs", async () => {
    stubStorage("{not json");
    expect((await importStore()).getChartResources()).toEqual([...DEFAULT_STAT_RESOURCE_IDS]);
    stubStorage(JSON.stringify({ plat: true }));
    expect((await importStore()).getChartResources()).toEqual([...DEFAULT_STAT_RESOURCE_IDS]);
  });

  it("keeps an empty selection instead of resurrecting the defaults", async () => {
    const mem = stubStorage(JSON.stringify([]));
    const store = await importStore();
    expect(store.getChartResources()).toEqual([]);

    store.toggleChartResource("kuva");
    expect(store.getChartResources()).toEqual(["kuva"]);
    expect(JSON.parse(mem.get(STORAGE_KEY) as string)).toEqual(["kuva"]);

    store.toggleChartResource("kuva");
    expect(store.getChartResources()).toEqual([]);
  });

  it("renders in catalog order however the prefs were written", async () => {
    stubStorage(JSON.stringify(["kuva", "plat", "credits"]));
    expect((await importStore()).getChartResources()).toEqual(["plat", "credits", "kuva"]);
  });

  it("resets back to the defaults", async () => {
    stubStorage(JSON.stringify(["kuva"]));
    const store = await importStore();
    store.resetChartResources();
    expect(store.getChartResources()).toEqual([...DEFAULT_STAT_RESOURCE_IDS]);
  });

  // The label keys are spelled out, so a resource added to the catalog alone would chart as "Unknown".
  it("names a label key for every catalog resource", () => {
    expect(STAT_RESOURCES.filter((r) => statResourceLabelKey(r.id) === "common.unknown")).toEqual([]);
  });
});
