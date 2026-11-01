import { X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { commands, type RelicPlannerEvent, type RewardOverlayView } from "../../bindings";
import { normalizeRewardOverlayLayout } from "../../../config/shared/rewardOverlayLayout";
import type { OverlayLayout } from "../../../config/shared/overlayLayout";
import { applyInventoryPick, getInventoryStatus, inTauri } from "../../lib/ipc";
import { SAMPLE_PLANNER_EVENT } from "../common/moveSamples";
import { MovablePanel } from "../common/MovablePanel";
import { OvReadout } from "../common/OvReadout";
import type { OverlayContext } from "../common/OverlayShell";
import { Planner, PLANNER_EVENT } from "./Planner";
import { RewardOverlayCanvas } from "./RewardOverlayCanvas";
import { PREVIEW_VIEW, rewardCardViews } from "./rewardCards";
import type { CardView } from "./RewardOverlayCanvas";

/** Pushed by the reward scan (`services/overlay/reward.rs`). */
export const REWARD_EVENT = "reward-overlay";


/** "I took this": the reward the player chose, added to the imported inventory as an estimate. */
function PickRow({ slots, tr, token }: { slots: NonNullable<RewardOverlayView["presentation"]>["slots"]; tr: OverlayContext["tr"]; token: number }) {
  const [picked, setPicked] = useState<string | null>(null);
  useEffect(() => setPicked(null), [token]);
  return (
    <div className="reward-pick" data-reward-pick>
      {slots.flatMap((slot) => (slot ? [slot.item.name] : [])).map((name) => (
        <button key={name} type="button" disabled={picked !== null} aria-pressed={picked === name} onClick={() => void applyInventoryPick(name).then(() => setPicked(name))}>
          {picked === name ? tr("overlay.reward.pickAdded") : tr("overlay.reward.pick", { name })}
        </button>
      ))}
    </div>
  );
}

/** The card to take, under the row; closing needs the interactive mode (the overlay is click-through otherwise). */
function RewardBar({ best, tr, interactive, onClose }: OverlayContext & { best: CardView | undefined; onClose: () => void }) {
  const canClose = interactive && inTauri();
  if (!best && !canClose) return null;
  return (
    <div className="overlay-panel reward-bar">
      {best && (
        <span className="reward-bar-sum">
          {tr("overlay.reward.pickSummary")}
          <strong>{best.name}</strong>
          {best.platinum != null && <OvReadout currency="platinum" value={best.platinum} />}
        </span>
      )}
      {canClose && (
        <button type="button" className="ov-btn ov-btn-icon reward-close" aria-label={tr("common.close")} onClick={onClose}>
          <X className="ov-icon" aria-hidden />
        </button>
      )}
    </div>
  );
}

/** The reward screen: the card row over the game's, and the scan state (reading, failed). */
export function RewardOverlay(ctx: OverlayContext) {
  const [live, setView] = useState<RewardOverlayView | null>(() => (new URLSearchParams(window.location.search).get("mode") === "preview" ? PREVIEW_VIEW : null));
  const [layout, setLayout] = useState<OverlayLayout>(() => normalizeRewardOverlayLayout(null));
  // A late event of an older round must not replace the current one.
  useEffect(() => ctx.ipc.on<RewardOverlayView>(REWARD_EVENT, (v) => setView((old) => (old && (v.token ?? 0) < (old.token ?? 0) ? old : v))), [ctx.ipc]);
  // Re-read per round: a saved layout may have changed since.
  useEffect(() => {
    let live = true;
    ctx.ipc.layout
      .get("reward")
      .catch(() => null)
      .then((json) => {
        try {
          if (live) setLayout(normalizeRewardOverlayLayout(JSON.parse(json ?? "null")));
        } catch {}
      });
    return () => void (live = false);
  }, [ctx.ipc, live?.token]);
  // Move mode shows the preview round when nothing live is on screen.
  const view = ctx.moving && live?.phase !== "ready" ? PREVIEW_VIEW : live;
  // Owned counts mean nothing without an inventory: re-checked per round, it may have been imported since.
  const [hasInventory, setHasInventory] = useState(true);
  useEffect(() => void getInventoryStatus().then((s) => setHasInventory(s.found), () => {}), [view?.token]);
  const cards = useMemo(() => (view?.presentation ? rewardCardViews(view.presentation, ctx.tr, hasInventory) : []), [view?.presentation, ctx.tr, hasInventory]);

  // `?mode=planner` previews the planner in a browser, with the mock inventory.
  const [planner, setPlanner] = useState<RelicPlannerEvent | null>(() =>
    new URLSearchParams(window.location.search).get("mode") === "planner" ? { token: 0, phase: "ready", era: "meso", confidence: 1, ocrUnavailable: false } : null,
  );
  useEffect(() => ctx.ipc.on<RelicPlannerEvent>(PLANNER_EVENT, (e) => setPlanner(e.phase === "closed" ? null : e)), [ctx.ipc]);
  // Move mode shows the planner next to the sample row, each with its own position.
  const plannerPanel = (event: RelicPlannerEvent) => (
    <MovablePanel id="planner" kind="reward" ctx={ctx}>
      <Planner {...ctx} event={event} />
    </MovablePanel>
  );
  if (planner && !ctx.moving) return plannerPanel(planner);
  if (!view) return null;
  return (
    <>
    {ctx.moving && plannerPanel(planner ?? SAMPLE_PLANNER_EVENT)}
    <div className="reward-live" data-interactive={ctx.interactive}>
      {view.phase === "scanning" && <p className="overlay-panel reward-status">{ctx.tr("overlay.reward.scanning")}</p>}
      {view.phase === "failed" && <p className="overlay-panel reward-status">{ctx.tr(view.failure === "screenClosed" ? "overlay.reward.screenClosed" : view.failure === "notRewardScreen" ? "overlay.reward.notRewardScreen" : "overlay.reward.ocrFailedShort")}</p>}
      {view.phase === "ready" && view.presentation && <RewardOverlayCanvas layout={layout} cards={cards} tr={ctx.tr} />}
      <RewardBar best={view.phase === "ready" ? cards.find((c) => c.best) : undefined} {...ctx} onClose={() => void commands.rewardOverlayClose(view.token)} />
      {ctx.interactive && inTauri() && view.phase === "ready" && view.presentation && (
        <PickRow slots={view.presentation.slots} tr={ctx.tr} token={view.token ?? 0} />
      )}
    </div>
    </>
  );
}
