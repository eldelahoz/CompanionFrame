import type { MessageKey } from "../i18n";
import type { WfmPresenceState, WsStatus } from "../ipc";
import type { PillTone } from "../inventoryFreshness";

export interface PresencePill {
  label: MessageKey;
  tone: PillTone;
}

/**
 * The titlebar's warframe.market pill. The socket state wins while it is down; "stopped" is not an error
 * (the socket idles between sessions and outside Tauri), so the presence status shows then.
 */
export function presencePill(loggedIn: boolean, ws: WsStatus, presence: WfmPresenceState | null): PresencePill {
  if (!loggedIn) return { label: "market.pill.signedOut", tone: "muted" };
  if (ws === "connecting" || ws === "reconnecting") return { label: "market.pill.connecting", tone: "warning" };
  if (ws === "gave_up") return { label: "market.pill.disconnected", tone: "danger" };
  switch (presence?.status) {
    case "ingame":
      return { label: "market.pill.ingame", tone: "success" };
    case "online":
      return { label: "market.pill.online", tone: "warning" };
    case "invisible":
      return { label: "market.pill.invisible", tone: "muted" };
    default:
      return { label: "market.pill.connected", tone: "muted" };
  }
}
