import { beforeEach, expect, it, vi } from "vitest";
import type { ArbiRunRecord } from "../lib/ipc";

const h = vi.hoisted(() => ({
  arbiRunsGet: vi.fn(),
  arbiRunGet: vi.fn(),
  arbiRunSetVitus: vi.fn(),
  onArbiRunSaved: vi.fn(),
  runDelete: vi.fn(),
  runDeleteLog: vi.fn(),
  runSetNotes: vi.fn(),
  runSetTags: vi.fn(),
}));
vi.mock("../lib/ipc", () => h);

import { arbiRuns, updateArbiNotes, updateArbiTags, updateArbiVitus } from "./arbiRuns";

const run = (id: string, extra: Partial<ArbiRunRecord> = {}): ArbiRunRecord => ({
  id,
  startedAt: 1,
  endedAt: 2,
  missionName: "Arbitration: Casta Defense (Ceres)",
  node: "Casta Defense (Ceres)",
  missionType: "defense",
  missionTypeRaw: null,
  solNode: null,
  durationSec: 600,
  rotations: 2,
  drones: 1,
  totalEnemies: 40,
  vitusActual: null,
  hasLog: true,
  logSizeBytes: 10,
  endReason: "mission-end",
  source: "live",
  stats: null,
  tags: [],
  players: [],
  notes: null,
  duplicateOf: null,
  ...extra,
});
const ids = () => arbiRuns.getState().runs.map((r) => r.id);

beforeEach(() => {
  Object.values(h).forEach((f) => f.mockReset());
  arbiRuns.reset();
});

it("loads runs, disk usage and the loaded flag", async () => {
  h.arbiRunsGet.mockResolvedValue({ runs: [run("a")], diskUsageBytes: 42 });
  expect(arbiRuns.getState().loaded).toBe(false);
  await arbiRuns.load();
  expect(arbiRuns.getState()).toMatchObject({ loaded: true, diskUsageBytes: 42 });
  expect(ids()).toEqual(["a"]);
});

it("prepends a pushed run and replaces one it already holds", async () => {
  h.arbiRunsGet.mockResolvedValue({ runs: [run("old")], diskUsageBytes: 0 });
  await arbiRuns.load();
  arbiRuns.subscribeSaved();
  const push = h.onArbiRunSaved.mock.calls[0][0] as (r: ArbiRunRecord) => void;

  push(run("new"));
  expect(ids()).toEqual(["new", "old"]);
  push(run("old", { vitusActual: 90 }));
  expect(ids()).toEqual(["new", "old"]);
  expect(arbiRuns.getState().runs[1].vitusActual).toBe(90);
});

it("drops a deleted run and re-reads the disk usage", async () => {
  h.arbiRunsGet.mockResolvedValueOnce({ runs: [run("a"), run("b")], diskUsageBytes: 100 });
  await arbiRuns.load();
  h.arbiRunsGet.mockResolvedValue({ runs: [run("b")], diskUsageBytes: 30 });
  h.runDelete.mockResolvedValue(true);
  await arbiRuns.remove("a");
  expect(ids()).toEqual(["b"]);
  expect(arbiRuns.getState().diskUsageBytes).toBe(30);
});

it("keeps the run when the delete failed", async () => {
  h.arbiRunsGet.mockResolvedValue({ runs: [run("a")], diskUsageBytes: 5 });
  await arbiRuns.load();
  h.runDelete.mockResolvedValue(false);
  await arbiRuns.remove("a");
  expect(ids()).toEqual(["a"]);
});

it("patches the record a mutation returns and ignores a null one", async () => {
  h.arbiRunsGet.mockResolvedValue({ runs: [run("a")], diskUsageBytes: 0 });
  await arbiRuns.load();
  h.arbiRunSetVitus.mockResolvedValue(run("a", { vitusActual: 120 }));
  await updateArbiVitus("a", 120);
  expect(arbiRuns.getState().runs[0].vitusActual).toBe(120);

  h.arbiRunSetVitus.mockResolvedValue(null);
  await updateArbiVitus("gone", 1);
  expect(ids()).toEqual(["a"]);
});

it("applies the tags and notes the backend normalized, and clears an empty note", async () => {
  h.arbiRunsGet.mockResolvedValue({ runs: [run("a", { notes: "old" })], diskUsageBytes: 0 });
  await arbiRuns.load();
  h.runSetTags.mockResolvedValue(["boar"]);
  await updateArbiTags("a", ["  Boar "]);
  expect(arbiRuns.getState().runs[0].tags).toEqual(["boar"]);

  h.runSetNotes.mockResolvedValue("");
  await updateArbiNotes("a", "  ");
  expect(arbiRuns.getState().runs[0].notes).toBeNull();

  h.runSetNotes.mockResolvedValue(null);
  await updateArbiNotes("gone", "x");
  expect(ids()).toEqual(["a"]);
});

it("marks the capture gone after a log delete and survives a failing usage refresh", async () => {
  h.arbiRunsGet.mockResolvedValueOnce({ runs: [run("a")], diskUsageBytes: 9 });
  await arbiRuns.load();
  h.runDeleteLog.mockResolvedValue(true);
  h.arbiRunGet.mockResolvedValue(run("a", { hasLog: false, logSizeBytes: 0 }));
  h.arbiRunsGet.mockRejectedValue(new Error("offline"));
  await expect(arbiRuns.removeLog("a")).resolves.toBeUndefined();
  expect(arbiRuns.getState().runs[0].hasLog).toBe(false);
  expect(arbiRuns.getState().diskUsageBytes).toBe(9);
});
