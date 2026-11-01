import { useEffect, useState } from "react";
import type { ArbiLive } from "../../bindings";
import type { Translator } from "../../lib/i18n";
import type { OverlayContext } from "../common/OverlayShell";
import { SAMPLE_ARBI } from "../common/moveSamples";

export const ARBI_LIVE_EVENT = "arbi-live";

export function formatClock(sec: number): string {
  const s = Math.max(0, Math.floor(sec || 0));
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return s >= 3600 ? `${Math.floor(s / 3600)}:${mm}:${ss}` : `${Math.floor(s / 60)}:${ss}`;
}

function missionLabel(run: ArbiLive, tr: Translator): string {
  const known = { defense: "arbi.type.defense", interception: "arbi.type.interception", disruption: "arbi.type.disruption" } as const;
  if (run.missionType !== "other") return tr(known[run.missionType]);
  // The game's own MT_ enum only exists in English.
  const raw = run.missionTypeRaw?.replace(/^MT_/, "").toLowerCase().replace(/(^|_)\w/g, (c: string) => c.replace("_", " ").toUpperCase());
  return raw || tr("overlay.arbi.missionFallback");
}

/** Live counter of the open arbitration; blank between runs (the backend sends null when one closes). */
export function ArbiOverlay({ tr, ipc, interactive, moving }: OverlayContext) {
  // `?mode=preview` shows the move-mode sample, for browser previews and screenshots.
  const [live, setRun] = useState<ArbiLive | null>(() => (new URLSearchParams(window.location.search).get("mode") === "preview" ? SAMPLE_ARBI : null));
  useEffect(() => ipc.on<ArbiLive | null>(ARBI_LIVE_EVENT, setRun), [ipc]);
  const run = live ?? (moving ? SAMPLE_ARBI : null);
  if (!run) return null;

  const num = (n: number, digits = 0) => n.toLocaleString(undefined, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  const rows: [string, string][] = [
    [tr("overlay.arbi.dpm"), num(run.dpm ?? 0, 1)],
    [tr("overlay.arbi.droneKills"), num(run.drones)],
    [tr("overlay.arbi.totalKills"), num(run.enemies)],
  ];
  const sat = Math.max(0, Math.min(100, run.saturationPct ?? 0));
  return (
    <div className="overlay-panel arbi-live" data-interactive={interactive}>
      <div className="ab-head">
        <strong>{run.node || tr("overlay.arbi.unknownNode")}</strong>
        <span className="ab-sub">{missionLabel(run, tr)}</span>
      </div>
      <div className="ab-time">
        {formatClock(run.elapsedSec ?? 0)}
        <small>{tr("overlay.arbi.rotations", { count: run.rotations })}</small>
      </div>
      <dl className="ab-grid">
        {rows.map(([k, v]) => (
          <div key={k}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </div>
        ))}
        <div className="ab-sat">
          <dt>
            {tr("overlay.arbi.saturation")}
            <b>{`${num(run.saturationPct ?? 0)}%`}</b>
          </dt>
          <dd className="ov-meter" aria-hidden>
            <i style={{ width: `${sat}%` }} />
          </dd>
        </div>
      </dl>
    </div>
  );
}
