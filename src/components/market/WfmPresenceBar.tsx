import { useEffect, useState } from "react";
import { WFM_STATUS_HOLD_MINUTES, normalizeWfmAwayIdleMinutes } from "../../../config/shared/wfm";
import type { Translator } from "../../lib/i18n";
import type { PresenceOptions, WfmPresenceState, WfmStatus } from "../../lib/ipc";

interface Props {
  state: WfmPresenceState;
  options: PresenceOptions;
  setStatus: (s: WfmStatus) => void;
  saveOptions: (patch: Partial<PresenceOptions>) => void;
  tr: Translator;
}

const STATUSES: Array<[WfmStatus, "common.online" | "common.inGame" | "common.invisible"]> = [
  ["online", "common.online"],
  ["ingame", "common.inGame"],
  ["invisible", "common.invisible"],
];
// The dot carries the status colour; the active segment is the raised one.
export const DOT: Record<WfmStatus, string> = { online: "bg-success", ingame: "bg-info", invisible: "bg-text-muted" };
const ACTIVE: Record<WfmStatus, string> = { online: "text-success", ingame: "text-info", invisible: "text-text-primary" };
const CHIP = "ui-chip disabled:cursor-default disabled:opacity-50";

export function formatHoldRemaining(expiresAt: number | null, now: number): string {
  if (!expiresAt) return "";
  const total = Math.max(0, Math.round((expiresAt - now) / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  if (h) return `${h}h ${String(m).padStart(2, "0")}m`;
  return m ? `${m}m` : `${total}s`;
}

// Shared by the Market header and the titlebar popover.
export default function WfmPresenceBar({ state, options, setStatus, saveOptions, tr }: Props) {
  const [now, setNow] = useState(() => Date.now());
  const deadline = state.expires_at ?? null;
  useEffect(() => {
    if (deadline === null) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [deadline]);

  const on = (b: boolean) => tr(b ? "market.stateOn" : "market.stateOff");
  const idle = !state.status || state.status === "invisible"; // a hidden status cannot expire
  const remaining = formatHoldRemaining(deadline, now);
  const minutes = normalizeWfmAwayIdleMinutes(options.away_idle_minutes);

  return (
    <div className="flex flex-wrap items-center gap-2" data-wfm-presence-bar>
      <div className="ui-seg" role="group" aria-label={tr("market.wfmTitle")}>
        {STATUSES.map(([key, label]) => (
          <button key={key} type="button" data-wfm-status-option={key} aria-pressed={state.status === key} className={state.status === key ? ACTIVE[key] : ""} onClick={() => setStatus(key)}>
            <span className={`status-dot ${DOT[key]}`} />
            {tr(label)}
          </button>
        ))}
      </div>
      <button type="button" aria-pressed={options.auto_ingame_enabled} className={CHIP} title={tr("market.autoIngameTitle")} onClick={() => saveOptions({ auto_ingame_enabled: !options.auto_ingame_enabled })}>
        {tr("market.autoInGame")}
        {on(options.auto_ingame_enabled)}
      </button>
      <button type="button" aria-pressed={options.away_idle_enabled} className={CHIP} title={tr("market.awayIdleTitle")} onClick={() => saveOptions({ away_idle_enabled: !options.away_idle_enabled })}>
        {tr("market.awayIdle", { minutes })}
        {on(options.away_idle_enabled)}
      </button>
      <input
        key={minutes}
        type="number"
        min={1}
        max={60}
        defaultValue={minutes}
        disabled={!options.away_idle_enabled}
        title={tr("market.awayIdleTitle")}
        aria-label={tr("market.awayIdle", { minutes })}
        className="ui-input h-7 w-[3.6rem] text-right tabular-nums"
        // an emptied input must not clamp up to the floor, so normalize against the current value
        onBlur={(e) => saveOptions({ away_idle_minutes: normalizeWfmAwayIdleMinutes(e.currentTarget.value, minutes) })}
      />
      <button type="button" aria-pressed={options.away_when_closed_enabled} className={CHIP} title={tr("market.awayClosedTitle")} onClick={() => saveOptions({ away_when_closed_enabled: !options.away_when_closed_enabled })}>
        {tr("market.awayClosed")}
        {on(options.away_when_closed_enabled)}
      </button>
      <div className={`flex flex-wrap items-center gap-1.5 ${idle ? "opacity-40" : ""}`}>
        <span className="ml-1 text-xs text-text-muted">{tr("market.keepStatusFor")}</span>
        {WFM_STATUS_HOLD_MINUTES.map((m) => (
          <button key={m} type="button" aria-pressed={options.hold_minutes === m && !idle} className={CHIP} disabled={idle} onClick={() => saveOptions({ hold_minutes: m })}>
            {!m ? tr("market.holdAlways") : m < 60 ? `${m}m` : `${m / 60}h`}
          </button>
        ))}
      </div>
      {remaining && <span className="text-xs text-text-secondary">{tr("market.holdLeft", { time: remaining })}</span>}
      {state.auto_active ? (
        <span className="text-xs text-text-muted">{tr("market.followingGame")}</span>
      ) : state.away_active ? (
        <span className="text-xs text-text-muted">{tr("market.presenceAway")}</span>
      ) : null}
    </div>
  );
}
