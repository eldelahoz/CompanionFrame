import { useSyncExternalStore } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { inTauri } from "../lib/ipc";

export const RELEASES_URL = "https://github.com/eldelahoz/CompanionFrame/releases";
export const CHECK_DELAY_MS = 8_000;
export const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export type UpdaterStatus = "idle" | "checking" | "uptodate" | "available" | "downloading" | "installing" | "error";
export interface UpdaterState {
  status: UpdaterStatus;
  version: string | null;
  /** Download progress 0-100, or null while the size is unknown. */
  progress: number | null;
  error: string | null;
  /** The user hit Later for this version: no banner until the next session. */
  dismissed: boolean;
}

// The plugin handle of the update found; not part of the state because it is not serializable.
type Pending = { version: string; downloadAndInstall: (onEvent: (e: { event: string; data?: { contentLength?: number; chunkLength?: number } }) => void) => Promise<void> };
let pending: Pending | null = null;
let state: UpdaterState = { status: "idle", version: null, progress: null, error: null, dismissed: false };
const dismissedVersions = new Set<string>();
const listeners = new Set<() => void>();
const set = (patch: Partial<UpdaterState>) => {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};
export const useUpdater = (): UpdaterState => useSyncExternalStore(subscribe, () => state);
export const getUpdaterState = () => state;
export const resetUpdater = () => {
  pending = null;
  dismissedVersions.clear();
  set({ status: "idle", version: null, progress: null, error: null, dismissed: false });
};

const busy = () => state.status === "checking" || state.status === "downloading" || state.status === "installing";

/** Asks github.com for latest.json. `manual` surfaces errors and shows "up to date"; an automatic check fails silently. */
export async function checkForUpdate(manual: boolean): Promise<void> {
  if (busy()) return;
  if (!inTauri()) {
    if (manual) set({ status: "uptodate", error: null });
    return;
  }
  const had = state.status === "available";
  if (manual || !had) set({ status: "checking", error: null });
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      pending = null;
      set({ status: "uptodate", version: null });
      return;
    }
    pending = update;
    set({ status: "available", version: update.version, dismissed: dismissedVersions.has(update.version) && !manual });
  } catch (e) {
    // Keep a known update on screen when a later automatic check fails.
    if (manual) set({ status: "error", error: e instanceof Error ? e.message : String(e) });
    else set({ status: had ? "available" : "idle" });
  }
}

/** Later: hides the banner for this version until the app restarts. */
export function dismissUpdate(): void {
  if (state.version) dismissedVersions.add(state.version);
  set({ dismissed: true });
}

/** NSIS passive mode closes the app, installs and starts it again; relaunch covers the other platforms. */
export async function installUpdate(): Promise<void> {
  if (!pending || busy()) return;
  let total = 0;
  let done = 0;
  set({ status: "downloading", progress: null, error: null });
  try {
    await pending.downloadAndInstall((e) => {
      if (e.event === "Started") total = e.data?.contentLength ?? 0;
      else if (e.event === "Progress") {
        done += e.data?.chunkLength ?? 0;
        set({ progress: total > 0 ? Math.min(100, (done / total) * 100) : null });
      } else if (e.event === "Finished") set({ status: "installing", progress: 100 });
    });
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch (e) {
    set({ status: "error", error: e instanceof Error ? e.message : String(e), dismissed: false });
  }
}

export const openReleaseNotes = (): void => {
  const url = state.version ? `${RELEASES_URL}/tag/v${state.version}` : RELEASES_URL;
  void openUrl(url);
};

/** First check after a short delay, then every 6 h. Returns the stopper. */
export function startAutoCheck(): () => void {
  const first = setTimeout(() => void checkForUpdate(false), CHECK_DELAY_MS);
  const every = setInterval(() => void checkForUpdate(false), CHECK_INTERVAL_MS);
  return () => {
    clearTimeout(first);
    clearInterval(every);
  };
}
