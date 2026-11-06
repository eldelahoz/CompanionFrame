import { jobsList, onJobUpdated, type Job, type JobUpdate } from "../lib/ipc";
import { createSharedStore } from "./shared";

export interface JobsState {
  jobs: Job[];
}

/** How long a clean finish stays on screen. */
export const OK_VISIBLE_MS = 5_000;

/** The price refresh: progress only, nothing to act on. */
export const isBackground = (job: Job) => job.kind.type === "pricesLoad";

/** Folds one backend update into the list. */
export function applyUpdate(jobs: Job[], u: JobUpdate): Job[] {
  if (u.job) return [...jobs.filter((j) => j.id !== u.id), u.job];
  return jobs.map((j) => {
    if (j.id !== u.id) return j;
    const { item } = u;
    const items = item ? j.items.map((i) => (i.id === item.id ? { ...i, status: item.status, reason: item.reason } : i)) : j.items;
    return { ...j, state: u.state, total: u.total, done: u.done, failed: u.failed, items };
  });
}

const cleanFinish = (j: Job) => j.state === "done" && j.failed === 0;

/** Running jobs, those that need the user (failures, interrupted) and, for a few seconds, a clean finish. */
const store = createSharedStore<JobsState>({ jobs: [] }, (patch) => {
  let dead = false;
  let jobs: Job[] = [];
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  const set = (next: Job[]) => {
    jobs = next;
    patch({ jobs });
  };
  const forget = (id: string) => {
    timers.delete(id);
    set(jobs.filter((j) => j.id !== id));
  };
  // The snapshot fills in what ran before this view looked; events that already landed win.
  let fetching = false;
  const refetch = () => {
    if (fetching) return;
    fetching = true;
    void jobsList().then(
      (list) => {
        fetching = false;
        if (!dead) set([...list.filter((j) => !jobs.some((x) => x.id === j.id)), ...jobs]);
      },
      () => {
        fetching = false;
      },
    );
  };
  const off = onJobUpdated((u) => {
    if (dead) return;
    // A partial update for a job we never saw (its first, full update came before we listened): ask the backend for it.
    if (!u.job && !jobs.some((j) => j.id === u.id)) return refetch();
    set(applyUpdate(jobs, u));
    const job = jobs.find((j) => j.id === u.id);
    if (job && cleanFinish(job) && !timers.has(job.id)) timers.set(job.id, setTimeout(() => forget(job.id), isBackground(job) ? 0 : OK_VISIBLE_MS));
  });
  refetch();
  return () => {
    dead = true;
    timers.forEach(clearTimeout);
    void off.then((f) => f());
  };
});

export const useJobs = store.use;
export const resetJobs = store.reset;
