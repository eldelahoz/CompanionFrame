import { describe, expect, it } from "vitest";
import type { HelperStatus } from "./ipc";
import { helperPill, helperView } from "./helperStatus";

const base: HelperStatus = { exeFound: true, running: false, lastRunAt: null, lastRunOk: null, lastRunReason: null, inventoryLastModified: null, installerAutoInstallHelper: null };
const NOW = new Date(2026, 5, 10, 15, 0).getTime();
const view = (s: Partial<HelperStatus> | null, none = false, linux = false) => helperView(s && { ...base, ...s }, none, linux, NOW, "en");

describe("helperView", () => {
  it("is muted without a status or without an inventory source", () => {
    expect(view(null).status.key).toBe("titlebar.status.unknown");
    expect(view({}, true)).toMatchObject({ status: { key: "settings.inventorySourceNone" }, dot: "bg-text-muted", pulse: false });
  });

  it("shows fresh data as ok with a clock time and old data as a warning", () => {
    const fresh = view({ inventoryLastModified: new Date(2026, 5, 10, 14, 30).getTime() });
    expect(fresh).toMatchObject({ status: { key: "titlebar.status.ok", params: { time: "14:30" } }, dot: "bg-success" });
    expect(view({ inventoryLastModified: new Date(2026, 5, 10, 12, 5).getTime() })).toMatchObject({ status: { key: "titlebar.status.old" }, dot: "bg-warning-dim" });
  });

  it("pulses while refreshing", () => {
    expect(view({ running: true })).toMatchObject({ status: { key: "titlebar.status.refreshing" }, dot: "bg-warning", pulse: true });
  });

  it("explains why there is no data, with the Linux wording for a denied read", () => {
    expect(view({ exeFound: false }).status.key).toBe("titlebar.status.helperNotFound");
    expect(view({ lastRunReason: "access-denied" }).status.key).toBe("titlebar.status.accessDenied");
    expect(view({ lastRunReason: "access-denied" }, false, true)).toMatchObject({ status: { key: "titlebar.status.memoryBlocked" }, tooltip: "titlebar.tooltip.memoryBlocked" });
    expect(view({ lastRunReason: "not-logged-in" }).tooltip).toBe("titlebar.tooltip.notLoggedIn");
    expect(view({}).status.key).toBe("titlebar.status.dataMissing");
  });
});

describe("helperPill", () => {
  const pill = (s: Partial<HelperStatus> | null, none = false) => helperPill(s && { ...base, ...s }, none, NOW);
  it("maps the helper state to connected / waiting / not configured / error", () => {
    expect(pill({}, true).label).toBe("titlebar.pill.notConfigured");
    expect(pill(null).label).toBe("titlebar.pill.checking");
    expect(pill({ running: true })).toMatchObject({ label: "titlebar.pill.syncing", pulse: true });
    expect(pill({ inventoryLastModified: NOW - 60_000 })).toMatchObject({ label: "titlebar.pill.connected", tone: "success" });
    expect(pill({ inventoryLastModified: NOW - 5 * 3_600_000 }).tone).toBe("warning");
    expect(pill({ exeFound: false }).label).toBe("titlebar.pill.notConfigured");
    expect(pill({ lastRunReason: "game-not-running" }).label).toBe("titlebar.pill.waiting");
    expect(pill({ lastRunReason: "access-denied" })).toMatchObject({ label: "titlebar.pill.error", tone: "danger" });
  });
});
