import type { Translator } from "../../lib/i18n";
import type { WfmPresenceState } from "../../lib/ipc";
import { presencePill } from "../../lib/wfm/presencePill";
import { openView } from "../../stores/nav";
import { useWfmPresence } from "../../stores/wfmPresence";

const DOT = { success: "bg-success", warning: "bg-warning", danger: "bg-danger", muted: "bg-text-muted" } as const;

interface Props {
  loggedIn: boolean;
  /** Live presence from usePresence (the wfm-presence event). */
  state: WfmPresenceState;
  tr: Translator;
}

/** Titlebar pill: warframe.market session, socket and presence; opens the Market tab. Status changes live there. */
export default function WfmStatusPill({ loggedIn, state, tr }: Props) {
  const { ws } = useWfmPresence();
  const pill = presencePill(loggedIn, ws, state);
  const note = loggedIn && state.auto_active ? tr("market.followingGame") : loggedIn && state.away_active ? tr("market.presenceAway") : "";
  const label = tr(pill.label);
  const tip = `${tr("market.wfmTitle")}: ${label}${note ? `, ${note}` : ""}. ${tr("market.pill.openMarket")}`;
  return (
    <button type="button" className="titlebar-pill" data-wfm-status-pill data-wfm-status-current={state.status ?? ""} data-tip={tip} aria-label={tip} onClick={() => openView("market")}>
      <span className={`status-dot ${DOT[pill.tone]}`} />
      <span className="titlebar-pill-text">{label}</span>
    </button>
  );
}
