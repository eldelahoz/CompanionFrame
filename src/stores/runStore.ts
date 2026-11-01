import { useSyncExternalStore } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface RunsPayload<T> {
  runs: T[];
  diskUsageBytes: number;
}

/** The IPC calls one run feature owns; passing thunks keeps each typed against its own command. */
interface RunStoreSpec<T> {
  fetch: () => Promise<RunsPayload<T>>;
  subscribeSaved: (onRun: (run: T) => void) => Promise<UnlistenFn>;
  removeRun: (id: string) => Promise<boolean>;
  /** Drops the raw capture; resolves to the updated record, null when the id is unknown. */
  removeRunLog: (id: string) => Promise<T | null>;
}

export interface RunsState<T> {
  runs: T[];
  diskUsageBytes: number;
  loaded: boolean;
}

/** The list, disk-usage and push handling every EE.log run feature needs, as a module-level store so
 * runs saved while their tab is closed still land (the push is bound for the app's lifetime). */
export function createRunStore<T extends { id: string }>(spec: RunStoreSpec<T>) {
  let state: RunsState<T> = { runs: [], diskUsageBytes: 0, loaded: false };
  const listeners = new Set<() => void>();
  const set = (patch: Partial<RunsState<T>>) => {
    state = { ...state, ...patch };
    listeners.forEach((l) => l());
  };

  function upsert(run: T): void {
    const i = state.runs.findIndex((r) => r.id === run.id);
    set({ runs: i >= 0 ? state.runs.map((r, k) => (k === i ? run : r)) : [run, ...state.runs] });
  }

  async function load(): Promise<void> {
    const p = await spec.fetch();
    set({ runs: p.runs, diskUsageBytes: p.diskUsageBytes, loaded: true });
  }

  async function refreshDiskUsage(): Promise<void> {
    try {
      set({ diskUsageBytes: (await spec.fetch()).diskUsageBytes });
    } catch {
      // the usage label is best-effort
    }
  }

  return {
    getState: () => state,
    subscribe: (l: () => void) => {
      listeners.add(l);
      return () => void listeners.delete(l);
    },
    load,
    subscribeSaved: () => spec.subscribeSaved(upsert),
    /** Applies the record a mutation handed back; null means the id was unknown. */
    patch: (updated: T | null) => {
      if (updated) upsert(updated);
    },
    remove: async (id: string) => {
      if (await spec.removeRun(id)) set({ runs: state.runs.filter((r) => r.id !== id) });
      await refreshDiskUsage();
    },
    removeLog: async (id: string) => {
      const updated = await spec.removeRunLog(id);
      if (updated) upsert(updated);
      await refreshDiskUsage();
    },
    /** Test hook: back to the empty, unloaded state. */
    reset: () => set({ runs: [], diskUsageBytes: 0, loaded: false }),
  };
}

export type RunStore<T extends { id: string }> = ReturnType<typeof createRunStore<T>>;

export const useRunStore = <T extends { id: string }>(store: RunStore<T>): RunsState<T> =>
  useSyncExternalStore(store.subscribe, store.getState);
