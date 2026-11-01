import { describe, expect, it } from "vitest";
import { VIEWS } from "../lib/viewRegistry";
import { LAYOUT_PRESETS, presetById } from "./layoutPresets";

const LAYOUT_VIEWS = VIEWS.map(([name]) => name as string).filter((name) => name !== "settings");

describe("layout presets", () => {
  it("only name arrangeable views and ids that belong to them", () => {
    for (const preset of LAYOUT_PRESETS) {
      for (const [view, sections] of Object.entries(preset.views)) {
        expect(LAYOUT_VIEWS, `${preset.id}.${view}`).toContain(view);
        for (const section of sections ?? []) expect(section.id.startsWith(`${view}.`), section.id).toBe(true);
      }
    }
  });

  it("list each section once per view", () => {
    for (const preset of LAYOUT_PRESETS) {
      for (const [view, sections] of Object.entries(preset.views)) {
        const ids = (sections ?? []).map((s) => s.id);
        expect(new Set(ids).size, `${preset.id}.${view}`).toBe(ids.length);
      }
    }
  });

  it("looks presets up by id", () => {
    expect(presetById("relicFarmer")?.id).toBe("relicFarmer");
    expect(presetById("nope")).toBeNull();
  });
});

describe("dashboard presets", () => {
  it("place every dashboard section", async () => {
    const { DASHBOARD_WIDGETS, sectionIdFor } = await import("../lib/widgets/registry");
    const all = [...DASHBOARD_WIDGETS.map((w) => sectionIdFor(w.id)), "dashboard.inventory", "dashboard.resources"].sort();
    for (const preset of LAYOUT_PRESETS) {
      expect((preset.views.dashboard ?? []).map((s) => s.id).sort(), preset.id).toEqual(all);
    }
  });
});
