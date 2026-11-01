import { beforeEach, describe, expect, it, vi } from "vitest";
import { stubStorage } from "../test/storage";
import { LAYOUT_STORAGE_KEY } from "../lib/layout/types";

let stored: Map<string, string>;

/** The store reads storage once at import, so every case needs a fresh module. */
async function load(raw?: string) {
  vi.resetModules();
  stored = stubStorage(raw === undefined ? {} : { [LAYOUT_STORAGE_KEY]: raw });
  const registry = await import("../lib/layout/registry");
  registry.registerSections("world", [
    { id: "world.cycles", view: "world", labelKey: "world.planetCycles", defaultSpan: 1 },
    { id: "world.timers", view: "world", labelKey: "world.resetTimers", defaultSpan: 1 },
    { id: "world.bounties", view: "world", labelKey: "world.bounties", defaultSpan: "full", canHide: false },
  ]);
  return import("./layout");
}

const ids = (layout: Awaited<ReturnType<typeof load>>, view = "world" as const) =>
  layout.sectionsOf(layout.layoutState.get(), view, "wide").map((s) => s.id);

describe("layout store", () => {
  beforeEach(() => vi.unstubAllGlobals());

  it("starts from registry defaults and survives unreadable storage", async () => {
    expect(ids(await load("{not json"))).toEqual(["world.cycles", "world.timers", "world.bounties"]);
  });

  it("persists a move and restores it on the next load", async () => {
    const layout = await load();
    layout.moveSection("world", "wide", "world.timers", "up");
    expect(ids(layout)).toEqual(["world.timers", "world.cycles", "world.bounties"]);
    const next = await load(stored.get(LAYOUT_STORAGE_KEY));
    expect(ids(next)).toEqual(["world.timers", "world.cycles", "world.bounties"]);
  });

  it("hides, restores and refuses to hide load-bearing sections", async () => {
    const layout = await load();
    layout.setHidden("world", "wide", "world.cycles", true);
    layout.setHidden("world", "wide", "world.bounties", true);
    const hidden = layout.sectionsOf(layout.layoutState.get(), "world", "wide").filter((s) => s.hidden);
    expect(hidden.map((s) => s.id)).toEqual(["world.cycles"]);
  });

  it("undoes the last change and resets a view", async () => {
    const layout = await load();
    layout.setSpan("world", "wide", "world.cycles", 2);
    layout.undo();
    expect(layout.sectionsOf(layout.layoutState.get(), "world", "wide")[0]?.span).toBe(1);
    layout.setHidden("world", "wide", "world.cycles", true);
    layout.resetView("world");
    expect(layout.sectionsOf(layout.layoutState.get(), "world", "wide")[0]?.hidden).toBe(false);
  });

  it("collapses a drag gesture into one undo entry", async () => {
    const layout = await load();
    layout.beginUndoGroup("world", "wide", "world.timers");
    layout.moveSection("world", "wide", "world.timers", "up");
    layout.moveSection("world", "wide", "world.timers", "down");
    layout.moveSection("world", "wide", "world.timers", "up");
    layout.endUndoGroup();
    layout.undo();
    expect(ids(layout)).toEqual(["world.cycles", "world.timers", "world.bounties"]);
  });

  it("applies a preset to listed views only", async () => {
    const layout = await load();
    layout.applyPreset("dailyChecklist", ["world"]);
    expect(layout.sectionsOf(layout.layoutState.get(), "world", "narrow").length).toBeGreaterThan(0);
    layout.applyPreset("missing");
    expect(layout.layoutState.get().views.stats).toBeUndefined();
  });
});
