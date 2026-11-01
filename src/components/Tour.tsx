import { useEffect, useLayoutEffect, useState } from "react";
import type { MessageKey, Translator } from "../lib/i18n";
import { getOnboarding, setTourSeen } from "../lib/ipc";
import { VIEWS, type ViewName } from "../lib/viewRegistry";
import { openView } from "../stores/nav";
import { tourRequested } from "../stores/onboarding";
import { Button } from "./ui";

interface Step {
  view: ViewName;
  body: MessageKey;
  /** Spotlight target; the content area when absent or not on screen. */
  target?: string;
}

// shortcut: steps that need a tab switched first (WFHelper 2, 5, 6, 9) are left out; add a prepare hook when needed.
const STEPS: readonly Step[] = [
  { view: "inventory", body: "tour.step1.body" },
  { view: "foundry", body: "tour.step3.body" },
  { view: "mastery", body: "tour.step4.body", target: '[data-tour="mastery-view-tabs"]' },
  { view: "stats", body: "tour.step7.body" },
  { view: "world", body: "tour.step8.body" },
  { view: "relics", body: "tour.step10.body", target: '[data-tour="relic-filters"]' },
  { view: "market", body: "tour.step11.body" },
  { view: "rivens", body: "tour.step13.body" },
  { view: "arbi", body: "tour.step14.body" },
  { view: "wiki", body: "tour.step15.body" },
  { view: "settings", body: "tour.step16.body" },
];

const PAD = 6;
const viewLabel = (view: ViewName) => VIEWS.find(([name]) => name === view)![1];

/** First-run feature tour: walks the main views with a spotlight. Runs once after setup; Settings restarts it. */
export default function Tour({ tr, enabled }: { tr: Translator; enabled: boolean }) {
  const requested = tourRequested.use();
  const [index, setIndex] = useState<number | null>(null);
  const [box, setBox] = useState<DOMRect | null>(null);

  // First launch: the backend remembers whether the tour was seen.
  useEffect(() => {
    if (enabled) void getOnboarding().then((s) => s.setupDone && !s.tourSeen && setIndex(0), () => {});
  }, [enabled]);
  useEffect(() => {
    if (!requested) return;
    tourRequested.set(false);
    setIndex(0);
  }, [requested]);

  const step = index === null ? null : STEPS[index];
  useEffect(() => {
    if (step) openView(step.view);
  }, [step]);

  // The view mounts after the navigation request, so look for the target on the next frames.
  useLayoutEffect(() => {
    if (!step) return;
    let frame = 0;
    let tries = 0;
    const measure = () => {
      const el = (step.target && document.querySelector(step.target)) || document.getElementById("content");
      if (el) setBox(el.getBoundingClientRect());
      if (++tries < 20) frame = requestAnimationFrame(measure);
    };
    measure();
    window.addEventListener("resize", measure);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", measure);
    };
  }, [step]);

  if (index === null || !step) return null;
  const last = index === STEPS.length - 1;
  const finish = () => {
    setIndex(null);
    void setTourSeen(true).catch(() => {});
  };

  return (
    <div className="pointer-events-none fixed inset-0 z-[1100]" data-tour-overlay role="dialog" aria-label={tr("tour.featureTour")}>
      {box && (
        <div
          className="absolute rounded-[var(--radius-md)] shadow-[0_0_0_100vmax_var(--scrim)] outline outline-2 outline-accent transition-all duration-200"
          style={{ left: box.left - PAD, top: box.top - PAD, width: box.width + 2 * PAD, height: box.height + 2 * PAD }}
        />
      )}
      <div className="pointer-events-auto absolute bottom-6 right-6 flex w-[min(24rem,calc(100vw-3rem))] flex-col gap-3 rounded-[var(--radius-lg)] border border-border-strong bg-bg-surface p-4 shadow-[var(--shadow-3)]">
        <div className="flex items-center justify-between text-xs text-text-muted">
          <span className="font-semibold text-accent">{tr("tour.featureTour")}</span>
          <span>
            {index + 1} / {STEPS.length}
          </span>
        </div>
        <h2 className="m-0 text-md font-semibold text-text-heading">{tr(viewLabel(step.view))}</h2>
        <p className="m-0 text-sm text-text-secondary">{tr(step.body)}</p>
        <div className="flex items-center justify-between gap-2">
          <Button variant="ghost" size="sm" onClick={finish}>
            {tr("tour.skip")}
          </Button>
          <div className="flex gap-2">
            {index > 0 && (
              <Button size="sm" onClick={() => setIndex(index - 1)}>
                {tr("common.back")}
              </Button>
            )}
            <Button variant="primary" size="sm" onClick={() => (last ? finish() : setIndex(index + 1))} autoFocus>
              {last ? tr("tour.done") : tr("common.next")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
