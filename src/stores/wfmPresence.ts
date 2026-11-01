import { useEffect, useState } from "react";
import {
  onWfmPresence,
  onWfmWsStatus,
  wfmPresenceGet,
  wfmPresenceSet,
  wfmWsStatus,
  type WfmPresenceState,
  type WfmStatus,
  type WsStatus,
} from "../lib/ipc";

/** Presence and socket state for the market header (WfmPresenceBar / WfmStatusPill mount it). */
export function useWfmPresence(): {
  presence: WfmPresenceState | null;
  ws: WsStatus;
  setStatus: (s: WfmStatus) => Promise<void>;
} {
  const [presence, setPresence] = useState<WfmPresenceState | null>(null);
  const [ws, setWs] = useState<WsStatus>("stopped");
  useEffect(() => {
    let live = true;
    void wfmPresenceGet().then((s) => live && setPresence(s)).catch(() => {});
    void wfmWsStatus().then((s) => live && setWs(s));
    const offs = [onWfmPresence((s) => live && setPresence(s)), onWfmWsStatus((s) => live && setWs(s))];
    return () => {
      live = false;
      offs.forEach((off) => void off.then((f) => f()));
    };
  }, []);
  return { presence, ws, setStatus: async (s) => void setPresence(await wfmPresenceSet(s)) };
}
