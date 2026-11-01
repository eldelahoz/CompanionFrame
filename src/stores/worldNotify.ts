import { useCallback, useEffect, useState } from "react";
import { worldNotifyGet, worldNotifySet, type WorldNotifyPrefs } from "../lib/ipc";

export type WorldCycleKey = keyof WorldNotifyPrefs["cycleAlerts"];

/** World-notification prefs with optimistic saves; `prefs` is null until the first load. */
export function useWorldNotify() {
  const [prefs, setPrefs] = useState<WorldNotifyPrefs | null>(null);
  useEffect(() => {
    let live = true;
    void worldNotifyGet().then((p) => live && setPrefs(p)).catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  const save = useCallback(async (next: WorldNotifyPrefs) => {
    setPrefs(next);
    try {
      setPrefs(await worldNotifySet(next));
    } catch {
      setPrefs(await worldNotifyGet().catch(() => next));
    }
  }, []);
  const toggleCycle = useCallback(
    (key: WorldCycleKey) => prefs && save({ ...prefs, cycleAlerts: { ...prefs.cycleAlerts, [key]: !prefs.cycleAlerts[key] } }),
    [prefs, save],
  );
  return { prefs, save, toggleCycle };
}
