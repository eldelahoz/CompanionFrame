import { useCallback, useEffect, useState } from "react";
import { normalizeWfmAwayIdleMinutes, normalizeWfmHoldMinutes } from "../../../config/shared/wfm";
import { onWfmPresence, wfmPresenceGet, wfmPresenceSet, wfmPresenceSetOptions, type PresenceOptions, type WfmPresenceState, type WfmStatus } from "../ipc";

const KEY = "wfm_presence_options";
export const DEFAULT_PRESENCE_OPTIONS: PresenceOptions = {
  auto_ingame_enabled: false,
  hold_minutes: 0,
  away_idle_enabled: false,
  away_idle_minutes: 10,
  away_when_closed_enabled: false,
};

/** Coerces whatever was stored: a hand-edited or older value falls back to the default per field. */
export function parsePresenceOptions(raw: unknown): PresenceOptions {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    auto_ingame_enabled: o.auto_ingame_enabled === true,
    hold_minutes: normalizeWfmHoldMinutes(o.hold_minutes),
    away_idle_enabled: o.away_idle_enabled === true,
    away_idle_minutes: normalizeWfmAwayIdleMinutes(o.away_idle_minutes),
    away_when_closed_enabled: o.away_when_closed_enabled === true,
  };
}

// shortcut: options live in localStorage (the backend only holds them in memory); move to Settings if the backend needs them at boot.
function loadPresenceOptions(): PresenceOptions {
  try {
    return parsePresenceOptions(JSON.parse(localStorage.getItem(KEY) ?? "null"));
  } catch {
    return DEFAULT_PRESENCE_OPTIONS;
  }
}

function savePresenceOptions(o: PresenceOptions): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(o));
  } catch {
    // the choice just does not survive a restart
  }
}

const EMPTY: WfmPresenceState = { status: null, expires_at: null, auto_active: false, away_active: false };

/** Live presence plus the persisted options; `enabled` is false while signed out (nothing to fetch or push). */
export function usePresence(enabled: boolean) {
  const [state, setState] = useState<WfmPresenceState>(EMPTY);
  const [options, setOptions] = useState<PresenceOptions>(loadPresenceOptions);

  useEffect(() => {
    if (!enabled) return setState(EMPTY);
    let live = true;
    void wfmPresenceSetOptions(options).catch(() => {});
    void wfmPresenceGet().then(
      (s) => live && setState(s),
      () => {},
    );
    const off = onWfmPresence((s) => live && setState(s));
    return () => {
      live = false;
      void off.then((f) => f());
    };
    // options are pushed by saveOptions; only a session change re-subscribes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  const setStatus = useCallback(async (s: WfmStatus) => setState(await wfmPresenceSet(s)), []);
  const saveOptions = useCallback(
    (patch: Partial<PresenceOptions>) => {
      const next = parsePresenceOptions({ ...options, ...patch });
      setOptions(next);
      savePresenceOptions(next);
      void wfmPresenceSetOptions(next).catch(() => {});
    },
    [options],
  );

  return { state, options, setStatus, saveOptions };
}
