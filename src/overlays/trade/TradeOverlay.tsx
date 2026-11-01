import { Check } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import ItemImage from "../../components/ItemImage";
import { inTauri, wfmSendPlusRep, wfmSessionStatus } from "../../lib/ipc";
import { useMarketArt } from "../../lib/useMarketArt";
import { OvReadout } from "../common/OvReadout";
import { commands, type SendRepResult, type TradeItem, type TradeOverlayView_Serialize as TradeOverlayView } from "../../bindings";
import type { OverlayContext } from "../common/OverlayShell";
import { SAMPLE_TRADE } from "../common/moveSamples";

export const TRADE_OVERLAY_EVENT = "trade-overlay";

/** What the screen asks of the backend; injectable so tests and the mock page need no Tauri. */
export interface TradeApi {
  dismiss(token: number | null): Promise<unknown>;
  closeMatch(token: number | null, orderId: string): Promise<boolean>;
  /** Clickable area in physical screen pixels; empty = click-through again. */
  setRegions(rects: { x: number; y: number; w: number; h: number }[]): Promise<unknown>;
  /** Whether a warframe.market session is open (the +rep button needs one). */
  signedIn(): Promise<boolean>;
  plusRep(username: string): Promise<SendRepResult>;
}

// Outside Tauri (browser mock, tests) the calls are inert, like tauriOverlayIpc.
const tauriTradeApi: TradeApi = {
  dismiss: async (t) => inTauri() && commands.tradeNotificationDismiss(t),
  closeMatch: async (t, id) => {
    if (!inTauri()) return false;
    const r = await commands.tradeNotificationCloseMatch(t, id);
    return r.status === "ok" && r.data;
  },
  setRegions: async (rects) => inTauri() && commands.overlaySetRegions("trade", rects),
  signedIn: async () => inTauri() && (await wfmSessionStatus()).logged_in,
  plusRep: wfmSendPlusRep,
};

type RepState = "idle" | "sending" | SendRepResult;
const REP_TEXT = {
  sending: "overlay.trade.plusRepSending",
  sent: "overlay.trade.plusRepSent",
  "already-exists": "overlay.trade.plusRepAlready",
  "user-not-found": "overlay.trade.plusRepNotFound",
  "profile-unresolved": "overlay.trade.plusRepFailed",
  failed: "overlay.trade.plusRepFailed",
} as const;

const line = (i: TradeItem) => `${i.direction === "given" ? "−" : "+"} ${i.count > 1 ? `${i.count}x ` : ""}${i.displayName}`;

/**
 * Trade popup: the trade, the listings it would close (one click each) and a dismiss button.
 * The backend owns the queue and the auto-close timer; this only draws `current` and makes its own
 * box clickable. shortcut: the box's screen position is window.screenX*dpr, fine for a window
 * placed at the monitor's origin, which is how the manager places overlays.
 */
export function TradeOverlay({ tr, ipc, moving, api = tauriTradeApi }: OverlayContext & { api?: TradeApi }) {
  // `?mode=preview` shows the move-mode sample, for browser previews and screenshots.
  const [live, setView] = useState<TradeOverlayView | null>(() => (new URLSearchParams(window.location.search).get("mode") === "preview" ? SAMPLE_TRADE : null));
  const view = moving && !live?.current ? SAMPLE_TRADE : live;
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [signedIn, setSignedIn] = useState(false);
  const [rep, setRep] = useState<RepState>("idle");
  const box = useRef<HTMLDivElement>(null);
  const art = useMarketArt();
  useEffect(() => ipc.on<TradeOverlayView>(TRADE_OVERLAY_EVENT, setView), [ipc]);

  const token = view?.token ?? null;
  const shown = !!view?.current;
  // A new trade clears the last one's error.
  useEffect(() => {
    setFailed(null);
    setRep("idle");
  }, [token]);
  useEffect(() => {
    void api.signedIn().then(setSignedIn, () => setSignedIn(false));
  }, [api, token]);
  useLayoutEffect(() => {
    // Move mode reports the shell's region; afterwards the real box is sent again.
    if (moving) return;
    const r = shown ? box.current?.getBoundingClientRect() : null;
    const d = window.devicePixelRatio || 1;
    void api.setRegions(
      r ? [{ x: Math.round((window.screenX + r.left) * d), y: Math.round((window.screenY + r.top) * d), w: Math.round(r.width * d), h: Math.round(r.height * d) }] : [],
    );
  }, [api, shown, token, view?.queued, failed, moving]);

  const t = view?.current;
  if (!t) return null;
  const { trade, matches } = t;
  // Only a known partner can be thanked; a failed send may be retried.
  const repOpen = signedIn && !!trade.partner && (rep === "idle" || rep === "failed" || rep === "profile-unresolved");
  const sendRep = async (partner: string | null | undefined) => {
    setRep("sending");
    setRep(await api.plusRep(partner ?? "").catch((): SendRepResult => "failed"));
  };
  const close = async (orderId: string) => {
    setBusy(orderId);
    const ok = await api.closeMatch(token, orderId).catch(() => false);
    setBusy(null);
    if (!ok) setFailed(orderId);
  };
  return (
    <div ref={box} className="overlay-panel trade-notice" data-interactive>
      <div className="tr-head">
        <span className="tr-ok">
          <Check className="ov-icon" aria-hidden />
        </span>
        <div className="tr-title">
          <strong>{tr("overlay.trade.tradeFinished")}</strong>
          <span className="tr-who">
            {trade.partner && <span>{tr("overlay.trade.with", { partner: trade.partner })}</span>}
            {trade.type !== "trade" && trade.platChange != null && <OvReadout currency="platinum" value={trade.platChange} />}
            {repOpen && (
              <button type="button" className="ov-btn" onClick={() => void sendRep(trade.partner)}>
                {tr("overlay.trade.plusRep")}
              </button>
            )}
            {signedIn && rep !== "idle" && <span role="status">{tr(REP_TEXT[rep as keyof typeof REP_TEXT])}</span>}
          </span>
        </div>
      </div>
      <ul className="tr-items">
        {trade.items.map((i, n) => (
          <li key={n}>{line(i)}</li>
        ))}
      </ul>
      {matches.length === 0 && <p className="trade-none">{tr("overlay.trade.noListingMatched")}</p>}
      {matches.map((m) => (
        <div key={m.orderId} className="tr-match">
          <span className="tr-art">
            <ItemImage slug={m.itemUrlName} src={art(m.itemUrlName)} cls="ov-art" tr={tr} />
          </span>
          <div className="tr-match-text">
            <div className="tr-match-name">{m.itemName}</div>
            <div className="tr-match-q">
              <span>{tr("overlay.trade.yourListing")}</span>
              <span>{`×${m.quantity}`}</span>
            </div>
          </div>
          <OvReadout currency="platinum" value={m.platinum} size="lg" />
          <button type="button" className="ov-btn ov-btn-primary tr-close" disabled={busy === m.orderId} onClick={() => void close(m.orderId)}>
            {tr("overlay.trade.closeListing")}
          </button>
        </div>
      ))}
      {failed && <p role="alert">{tr("overlay.trade.closingFailed")}</p>}
      <div className="tr-actions">
        <button type="button" className="ov-btn" onClick={() => void api.dismiss(token)}>
          {tr("overlay.trade.dismiss")}
        </button>
        {view.queued > 0 && <span className="trade-queued">{tr("overlay.trade.queued", { count: view.queued })}</span>}
      </div>
      <i key={token} className="trade-timer" style={{ animationDuration: `${view.autoCloseMs}ms` }} />
    </div>
  );
}
