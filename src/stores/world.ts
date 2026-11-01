import { useEffect, useState } from "react";
import { getWorldState, onWorldStateFetchError, type WorldState } from "../lib/ipc";
import { FISSURE_MODES, type FissureMode } from "../lib/world/view";

const POLL_MS = 30_000;

/** Polls the backend (which caches for a minute) and surfaces the last fetch error. */
export function useWorldState(): { data: WorldState | null; loading: boolean; error: string | null } {
  const [data, setData] = useState<WorldState | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const load = () =>
      void getWorldState()
        .then((next) => {
          if (!live) return;
          setData(next);
          setError(null);
        })
        .catch((e) => live && setError(String(e)))
        .finally(() => live && setLoading(false));
    load();
    const timer = setInterval(load, POLL_MS);
    const off = onWorldStateFetchError((m) => live && setError(m));
    return () => {
      live = false;
      clearInterval(timer);
      void off.then((f) => f());
    };
  }, []);
  return { data, loading, error };
}

/** Re-renders on an interval; the value is the current time in ms. */
export function useClock(intervalMs: number): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(timer);
  }, [intervalMs]);
  return now;
}

// localStorage can throw (private windows, blocked site data); the view works without it.
const read = (key: string): string | null => {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
};
const write = (key: string, value: string): void => {
  try {
    localStorage.setItem(key, value);
  } catch {
    // preference is best-effort
  }
};

export function usePersistedFissureMode(): [FissureMode, (m: FissureMode) => void] {
  const [mode, setMode] = useState<FissureMode>(() => {
    const saved = read("wf_fissure_mode");
    return FISSURE_MODES.find((m) => m === saved) ?? "normal";
  });
  return [
    mode,
    (m) => {
      setMode(m);
      write("wf_fissure_mode", m);
    },
  ];
}

const COLLAPSE_KEY = "world-collapsed-sections";

export function useCollapsedSections(): [Record<string, boolean>, (key: string) => void] {
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>(() => {
    try {
      const parsed: unknown = JSON.parse(read(COLLAPSE_KEY) ?? "{}");
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
      return Object.fromEntries(Object.entries(parsed).filter(([, v]) => typeof v === "boolean"));
    } catch {
      return {};
    }
  });
  return [
    collapsed,
    (key) =>
      setCollapsed((prev) => {
        const next = { ...prev, [key]: !prev[key] };
        // Per-job bounty rows are transient, so only section state persists.
        const saved = Object.entries(next).filter(([k]) => !/^bounty-.+-\d+$/.test(k));
        write(COLLAPSE_KEY, JSON.stringify(Object.fromEntries(saved)));
        return next;
      }),
  ];
}
