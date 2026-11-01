import { beforeEach, expect, it, vi } from "vitest";
import type { PtRunRecord } from "../lib/ipc";

const h = vi.hoisted(() => ({
  ptRunsGet: vi.fn(),
  ptRunGet: vi.fn(),
  onPtRunSaved: vi.fn(),
  runDelete: vi.fn(),
  runDeleteLog: vi.fn(),
  runSetNotes: vi.fn(),
  runSetTags: vi.fn(),
}));
vi.mock("../lib/ipc", () => h);

import { ptRuns, updatePtNotes, updatePtTags } from "./ptRuns";

const run = (id: string, extra: Partial<PtRunRecord> = {}) => ({ id, tags: [], notes: null, hasLog: true, ...extra }) as unknown as PtRunRecord;
const ids = () => ptRuns.getState().runs.map((r) => r.id);

beforeEach(() => {
  Object.values(h).forEach((f) => f.mockReset());
  ptRuns.reset();
});

it("loads, takes pushed runs on the pt event and patches annotations", async () => {
  h.ptRunsGet.mockResolvedValue({ runs: [run("a")], diskUsageBytes: 11 });
  await ptRuns.load();
  expect(ptRuns.getState()).toMatchObject({ loaded: true, diskUsageBytes: 11 });

  ptRuns.subscribeSaved();
  (h.onPtRunSaved.mock.calls[0][0] as (r: PtRunRecord) => void)(run("b"));
  expect(ids()).toEqual(["b", "a"]);

  h.runSetTags.mockResolvedValue(["fast"]);
  await updatePtTags("a", [" Fast "]);
  h.runSetNotes.mockResolvedValue("");
  await updatePtNotes("a", " ");
  expect(ptRuns.getState().runs[1]).toMatchObject({ tags: ["fast"], notes: null });
});

it("drops a deleted run and keeps the record after a log delete", async () => {
  h.ptRunsGet.mockResolvedValue({ runs: [run("a"), run("b")], diskUsageBytes: 5 });
  await ptRuns.load();
  h.runDelete.mockResolvedValue(true);
  await ptRuns.remove("a");
  expect(ids()).toEqual(["b"]);
  h.runDeleteLog.mockResolvedValue(true);
  h.ptRunGet.mockResolvedValue(run("b", { hasLog: false }));
  await ptRuns.removeLog("b");
  expect(ptRuns.getState().runs[0].hasLog).toBe(false);
});
