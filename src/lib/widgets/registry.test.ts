// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { expect, test } from "vitest";
import { mergeViewLayout } from "../layout/plan";
import { usePersisted } from "../usePersisted";
import { WORLD_TABS } from "../../views/WorldView";
import { normalizeDashboard } from "../../stores/dashboard";
import { SPAN_ORDER } from "../layout/plan";
import { DASHBOARD_WIDGETS, dashboardSectionDescriptors, sectionIdFor, withRowKeys } from "./registry";

test("allowedSpans are a contiguous tail of SPAN_ORDER", () => {
  for (const w of DASHBOARD_WIDGETS) {
    const start = SPAN_ORDER.indexOf(w.allowedSpans[0]!);
    expect(w.allowedSpans).toEqual(SPAN_ORDER.slice(start));
  }
});

test("section ids map from widget ids", () => {
  expect(sectionIdFor("widget.recentRuns")).toBe("dashboard.recentRuns");
  expect(dashboardSectionDescriptors().map((d) => d.id)).toHaveLength(DASHBOARD_WIDGETS.length);
  expect(dashboardSectionDescriptors(new Set(["widget.baro"]))).toHaveLength(1);
});

test("repeated rows get numbered keys", () => {
  expect(withRowKeys([{}, {}, {}], () => "k").map((r) => r.rowKey)).toEqual(["k", "k#2", "k#3"]);
});

test("normalizeDashboard drops junk and clamps limits", () => {
  const out = normalizeDashboard({ version: 1, widgets: [{ id: "widget.fissures", settings: { limit: 999, lith: "no" } }, { id: "nope", settings: {} }] });
  expect(out["widget.fissures"]).toMatchObject({ limit: 20, lith: true });
  expect(out["nope"]).toBeUndefined();
  expect(out["widget.recentRuns"]).toEqual({ limit: 5 });
});

test("dailies is a full-width dashboard widget, right after the fissures", () => {
  const ids = dashboardSectionDescriptors().map((d) => d.id);
  const merged = mergeViewLayout(null, dashboardSectionDescriptors());
  expect(merged.sections.find((s) => s.id === "dashboard.dailies")).toMatchObject({ span: "full", hidden: false });
  expect(ids.indexOf("dashboard.dailies")).toBe(ids.indexOf("dashboard.fissures") + 1);
});

test("a saved dashboard layout without dailies gets it visible after the fissures", () => {
  const saved = { version: 1 as const, sections: ["dashboard.cycles", "dashboard.fissures", "dashboard.baro"].map((id) => ({ id, span: 1 as const, hidden: false, collapsed: false })) };
  const merged = mergeViewLayout(saved, dashboardSectionDescriptors()).sections.map((s) => s.id);
  expect(merged.indexOf("dashboard.dailies")).toBe(merged.indexOf("dashboard.fissures") + 1);
  expect(mergeViewLayout(saved, dashboardSectionDescriptors()).sections.find((s) => s.id === "dashboard.dailies")?.hidden).toBe(false);
});

test("the World tabs no longer include dailies and a stored dailies tab falls back", () => {
  expect(WORLD_TABS).not.toContain("dailies");
  localStorage.setItem("world-tab", "dailies");
  expect(renderHook(() => usePersisted("world-tab", WORLD_TABS, "world")).result.current[0]).toBe("world");
});
