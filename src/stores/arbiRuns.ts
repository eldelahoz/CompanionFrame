import {
  arbiRunGet,
  arbiRunSetVitus,
  arbiRunsGet,
  onArbiRunSaved,
  runDelete,
  runDeleteLog,
  runSetNotes,
  runSetTags,
  type ArbiRun,
} from "../lib/ipc";
import { createRunStore, useRunStore } from "./runStore";

export const arbiRuns = createRunStore<ArbiRun>({
  // The bindings type every f64 as number | null (NaN serializes to null).
  fetch: async () => {
    const p = await arbiRunsGet();
    return { runs: p.runs as unknown as ArbiRun[], diskUsageBytes: p.diskUsageBytes ?? 0 };
  },
  subscribeSaved: (cb) => onArbiRunSaved((run) => cb(run as unknown as ArbiRun)),
  removeRun: runDelete,
  removeRunLog: async (id) => ((await runDeleteLog(id)) ? ((await arbiRunGet(id)) as unknown as ArbiRun | null) : null),
});

export const useArbiRuns = () => useRunStore(arbiRuns);

const patch = (id: string, change: Partial<ArbiRun>) => {
  const run = arbiRuns.getState().runs.find((r) => r.id === id);
  if (run) arbiRuns.patch({ ...run, ...change });
};

export const updateArbiVitus = async (id: string, vitus: number | null) =>
  arbiRuns.patch((await arbiRunSetVitus(id, vitus)) as unknown as ArbiRun | null);

export async function updateArbiTags(id: string, tags: string[]) {
  const stored = await runSetTags(id, tags);
  if (stored) patch(id, { tags: stored });
}

export async function updateArbiNotes(id: string, notes: string) {
  const stored = await runSetNotes(id, notes);
  if (stored !== null) patch(id, { notes: stored || null });
}
