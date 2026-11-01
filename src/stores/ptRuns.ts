import {
  onPtRunSaved,
  ptRunGet,
  ptRunsGet,
  runDelete,
  runDeleteLog,
  runSetNotes,
  runSetTags,
  type PtRunRecord,
} from "../lib/ipc";
import { createRunStore, useRunStore } from "./runStore";

export const ptRuns = createRunStore<PtRunRecord>({
  fetch: async () => {
    const p = await ptRunsGet();
    return { runs: p.runs, diskUsageBytes: p.diskUsageBytes };
  },
  subscribeSaved: onPtRunSaved,
  removeRun: runDelete,
  removeRunLog: async (id) => ((await runDeleteLog(id)) ? ptRunGet(id) : null),
});

export const usePtRuns = () => useRunStore(ptRuns);

const patch = (id: string, change: Partial<PtRunRecord>) => {
  const run = ptRuns.getState().runs.find((r) => r.id === id);
  if (run) ptRuns.patch({ ...run, ...change });
};

export async function updatePtTags(id: string, tags: string[]) {
  const stored = await runSetTags(id, tags);
  if (stored) patch(id, { tags: stored });
}

export async function updatePtNotes(id: string, notes: string) {
  const stored = await runSetNotes(id, notes);
  if (stored !== null) patch(id, { notes: stored || null });
}
