import { TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { commands } from "../../bindings";
import { inTauri } from "../../lib/ipc";
import { itemsByName } from "../../lib/world/itemsByName";
import { useItemDatabase } from "../../stores/items";
import type { RivenOverlayView_Serialize as RivenOverlayView } from "../../bindings";
import type { OverlayContext } from "../common/OverlayShell";
import { SAMPLE_RIVEN_COMPARE, SAMPLE_RIVEN_EMPTY, SAMPLE_RIVEN_PARTIAL, SAMPLE_RIVEN_SINGLE } from "../common/moveSamples";

const SAMPLES: Record<string, RivenOverlayView> = { single: SAMPLE_RIVEN_SINGLE, partial: SAMPLE_RIVEN_PARTIAL, empty: SAMPLE_RIVEN_EMPTY };
import { CardHead, CardStats, Estimate, Footer, Listings, Loading, MarketLink, NoListingsNote, VerdictBanner } from "./RivenParts";

export const RIVEN_OVERLAY_EVENT = "riven-overlay";

/**
 * Riven overlay: one card (inspect, after a pick) or the compare screen's Current beside the New roll
 * with a keep / take verdict. The backend owns the scans, the market analysis and the auto-close; a
 * rescan, compare reads included, keeps the previous view under a "scanning" chip; the skeleton is only for a compare read with nothing shown yet.
 */
export function RivenOverlay({ tr, ipc, moving }: OverlayContext) {
  // `?mode=preview[&sample=single|partial|empty|loading]` shows a sample, for browser previews and screenshots.
  const preview = new URLSearchParams(window.location.search);
  const [live, setView] = useState<RivenOverlayView | null>(() => (preview.get("mode") === "preview" ? (SAMPLES[preview.get("sample") ?? ""] ?? SAMPLE_RIVEN_COMPARE) : null));
  const [scanning, setScanning] = useState<RivenOverlayView | null>(() => (preview.get("mode") === "preview" && preview.get("sample") === "loading" ? SAMPLE_RIVEN_COMPARE : null));
  const art = itemsByName(useItemDatabase());
  useEffect(
    () =>
      ipc.on<RivenOverlayView>(RIVEN_OVERLAY_EVENT, (v) => {
        setScanning(v.status === "scanning" ? v : null);
        if (v.status !== "scanning") setView(v);
      }),
    [ipc],
  );

  // Move mode shows a sample of the layout last drawn (the compare one first), so the frame matches.
  const view = moving && (!live || live.status !== "ready") ? (live?.layout === "single" ? SAMPLE_RIVEN_SINGLE : SAMPLE_RIVEN_COMPARE) : live;
  if (scanning?.layout === "compare" && live?.status !== "ready") return <Loading tr={tr} />;
  const chip = scanning && <span className="overlay-panel riven-chip">{tr("overlay.riven.scanning")}</span>;
  if (!view || view.status === "closed") return <>{chip}</>;
  if (view.status !== "ready") {
    const text = view.status === "unavailable" ? tr("overlay.riven.unavailable") : tr("overlay.riven.readFailed");
    return (
      <>
        {chip}
        <div className="overlay-panel riven-card rv-one" role="alert">
          <p className="riven-note">{text}</p>
        </div>
      </>
    );
  }

  const { card, current } = view;
  if (view.layout === "compare" && current) {
    return (
      <>
        {chip}
        <div className="overlay-panel riven-card rv-cmp">
          <VerdictBanner v={card ? view.verdict : null} tr={tr} />
          <div className="rv-cols">
            <div className="rv-col">
              <div className="rv-col-lab">
                <span>{tr("overlay.riven.current")}</span>
                <span className="rv-tag">{tr("overlay.riven.tagKept")}</span>
              </div>
              <CardHead card={current} art={art} tr={tr} />
              <CardStats card={current} tr={tr} />
              <Estimate card={current} tr={tr} />
            </div>
            <hr />
            <div className="rv-col">
              <div className="rv-col-lab">
                <span>{tr("overlay.riven.newRoll")}</span>
                <span className="rv-tag" data-new>
                  {tr("overlay.riven.tagOnScreen")}
                </span>
              </div>
              {card ? (
                <>
                  <CardHead card={card} art={art} tr={tr} />
                  <CardStats card={card} tr={tr} />
                  <Estimate card={card} tr={tr} />
                </>
              ) : (
                <div className="rv-unread">
                  <TriangleAlert className="ov-icon" aria-hidden />
                  <strong>{tr("overlay.riven.unreadTitle")}</strong>
                  {tr("overlay.riven.unreadText")}
                  <button type="button" onClick={() => inTauri() && void commands.rivenOverlayScan("roll")}>
                    {tr("overlay.riven.readAgain")}
                  </button>
                </div>
              )}
            </div>
          </div>
          <hr className="rv-sep" />
          <Footer view={view} card={card} tr={tr} />
        </div>
      </>
    );
  }
  if (!card) return <>{chip}</>;
  return (
    <>
      {chip}
      <div className="overlay-panel riven-card rv-one">
        <CardHead card={card} art={art} tr={tr} />
        <CardStats card={card} tr={tr} best />
        <Estimate card={card} tr={tr} />
        <NoListingsNote card={card} tr={tr} />
        <Listings card={card} tr={tr} />
        <MarketLink card={card} tr={tr} />
        <hr className="rv-sep" />
        <Footer view={view} card={card} tr={tr} />
      </div>
    </>
  );
}
