import { Check, Info } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import ConfigureStep from "../components/setup/ConfigureStep";
import OverlayPlacement from "../components/setup/OverlayPlacement";
import MarketStep from "../components/setup/MarketStep";
import DataSourceMode from "../components/settings/DataSourceMode";
import AppDisclaimer from "../components/settings/AppDisclaimer";
import { Button, Card, CardHeader, PageHeader } from "../components/ui";
import type { MessageKey, Translator } from "../lib/i18n";
import { clearInventory, getOnboarding, setSetupDone, type Settings, type WfmSession } from "../lib/ipc";
import { isMemoryMode } from "../stores/dataSource";
import { useInventory } from "../stores/inventory";
import { startTour } from "../stores/onboarding";

type Phase = "inventory" | "market" | "configure" | "overlay" | "finish";
export type SetupTarget = "inventory" | "world";

interface Props {
  tr: Translator;
  onComplete: (next: SetupTarget) => void;
  /** Without these the matching steps are left out (isolated tests). */
  settings?: Settings;
  onChange?: (patch: Partial<Settings>) => Promise<string | null> | void;
  market?: {
    session: WfmSession | null;
    signIn: (email: string, password: string) => Promise<void>;
    signInWebview?: () => Promise<void>;
  };
}

const TITLES: Record<Phase, { label: MessageKey; title: MessageKey; desc: MessageKey }> = {
  inventory: { label: "common.inventory", title: "setup.inventory.title", desc: "setup.inventory.desc" },
  market: { label: "setup.step.market", title: "setup.market.title", desc: "setup.market.desc" },
  configure: { label: "setup.step.configure", title: "setup.configure.title", desc: "setup.configure.desc" },
  overlay: { label: "setup.step.overlay", title: "setup.overlay.title", desc: "setup.overlay.desc" },
  finish: { label: "setup.step.finish", title: "setup.finish.title", desc: "setup.finish.desc" },
};

export default function SetupView({ tr, onComplete, settings, onChange, market }: Props) {
  const phases: Phase[] = [
    "inventory",
    ...(market ? (["market"] as const) : []),
    ...(settings && onChange ? (["configure"] as const) : []),
    "overlay",
    "finish",
  ];
  const [phase, setPhase] = useState<Phase>("inventory");
  const [target, setTarget] = useState<SetupTarget>("inventory");
  const { status } = useInventory();
  const [errorMessage, setErrorMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);

  const move = (by: number) => setPhase((cur) => phases[Math.max(0, Math.min(phases.length - 1, phases.indexOf(cur) + by))]);

  useEffect(() => {
    alive.current = true;
    return () => void (alive.current = false);
  }, []);

  /** Runs one import attempt; moves on when it worked, otherwise shows why. */
  const attempt = async (run: () => Promise<void>, next: SetupTarget, failure: (e: Error) => string) => {
    setBusy(true);
    setErrorMessage("");
    try {
      await run();
      if (!alive.current) return;
      setTarget(next);
      move(1);
    } catch (e) {
      if (alive.current) setErrorMessage(failure(e as Error));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  /** Moves on with whatever the data-source step set up; nothing set up means the world view at the end. */
  const next = () => {
    setTarget(status?.found || (settings && isMemoryMode(settings)) ? "inventory" : "world");
    move(1);
  };

  const finish = async () => {
    setBusy(true);
    try {
      await setSetupDone(true);
      if (!(await getOnboarding()).tourSeen) startTour();
    } catch {
      // the wizard just shows again next launch
    }
    onComplete(target);
  };

  const inInventory = phase === "inventory";
  const [title, desc] = [TITLES[phase].title, TITLES[phase].desc];

  let body: ReactNode = null;
  if (phase === "market" && market) body = <MarketStep tr={tr} {...market} />;
  if (phase === "configure" && settings && onChange) body = <ConfigureStep tr={tr} settings={settings} onChange={onChange} />;
  if (phase === "overlay") body = <OverlayPlacement tr={tr} />;
  if (phase === "finish") {
    body = (
      <ul className="m-0 flex list-none flex-col gap-2 p-0 text-sm text-text-secondary">
        {(["setup.finish.tour", "setup.finish.settings"] as const).map((k) => (
          <li key={k} className="flex items-start gap-2">
            <Check size={16} className="mt-0.5 shrink-0 text-accent" aria-hidden />
            {tr(k)}
          </li>
        ))}
      </ul>
    );
  }
  if (inInventory) {
    body = (
      <>
        <p className="m-0 flex items-start gap-2 rounded-[var(--radius-md)] border border-border bg-bg-raised p-3 text-sm text-text-secondary" role="note" data-app-disclaimer>
          <Info size={16} className="mt-0.5 shrink-0 text-info" aria-hidden />
          <AppDisclaimer tr={tr} />
        </p>
        {settings && onChange && (
          <section className="flex flex-col gap-2" data-setup-data-source>
            <h3 className="m-0 text-sm font-semibold text-text-primary">{tr("dataSource.title")}</h3>
            <DataSourceMode tr={tr} settings={settings} onChange={onChange} locale={settings.language} />
          </section>
        )}
        {errorMessage && (
          <p className="m-0 text-xs text-danger" role="alert">
            {errorMessage}
          </p>
        )}
      </>
    );
  }

  const index = phases.indexOf(phase);

  return (
    <div className="mx-auto flex w-full max-w-[56rem] flex-col gap-4 @container" data-setup-step={phase}>
      <PageHeader title={tr("setup.welcomeTitle")} sub={tr("setup.welcomeSub")} />
      <ol className="m-0 flex list-none items-center gap-2 p-0" aria-label={tr("setup.stepsLabel")}>
        {phases.map((p, i) => {
          const state = i < index ? "done" : i === index ? "active" : "todo";
          const danger = p === "inventory" && !!errorMessage && state === "active";
          return (
            <li key={p} className="flex min-w-0 flex-1 items-center gap-2" aria-current={state === "active" ? "step" : undefined}>
              <span
                className={`grid h-6 w-6 shrink-0 place-items-center rounded-full border text-xs font-semibold ${
                  danger
                    ? "border-danger text-danger"
                    : state === "active"
                      ? "border-accent bg-accent text-text-on-accent"
                      : state === "done"
                        ? "border-success text-success"
                        : "border-border text-text-muted"
                }`}
              >
                {state === "done" ? <Check size={14} aria-hidden /> : i + 1}
              </span>
              <span className={`hidden truncate text-xs @xl:block ${state === "active" ? "font-semibold text-text-primary" : "text-text-muted"}`}>{tr(TITLES[p].label)}</span>
              {i < phases.length - 1 && <span className="h-px min-w-2 flex-1 bg-border" aria-hidden />}
            </li>
          );
        })}
      </ol>

      <Card>
        <CardHeader title={tr(title)} />
        <div className="flex flex-col gap-4 p-4">
          {desc && <p className="m-0 text-sm text-text-secondary">{tr(desc)}</p>}
          {body}
        </div>
        <footer className="flex flex-wrap items-center justify-between gap-2 border-t border-border p-3">
          <div>
            {index > 0 && (
              <Button variant="ghost" onClick={() => move(-1)}>
                {tr("common.back")}
              </Button>
            )}
          </div>
          <div className="flex flex-wrap justify-end gap-2">
            {inInventory && (
              <Button disabled={busy} data-setup-without-inventory onClick={() => void attempt(clearInventory, "world", () => tr("settings.inventorySourceChangeFailed"))}>
                {tr("setup.withoutInventory")}
              </Button>
            )}
            {phase === "market" && (
              <Button variant="ghost" onClick={() => move(1)}>
                {tr("setup.skip")}
              </Button>
            )}
            {inInventory && (
              <Button variant="primary" disabled={busy} data-setup-next onClick={next}>
                {tr("common.next")}
              </Button>
            )}
            {!inInventory && phase !== "finish" && (
              <Button variant="primary" onClick={() => move(1)}>
                {tr("common.next")}
              </Button>
            )}
            {phase === "finish" && (
              <Button variant="primary" disabled={busy} data-setup-finish onClick={() => void finish()}>
                {tr("setup.finish")}
              </Button>
            )}
          </div>
        </footer>
      </Card>
    </div>
  );
}
