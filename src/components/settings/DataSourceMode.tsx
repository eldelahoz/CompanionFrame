import { useEffect, useState, type ReactNode } from "react";
import { PackageCheck, Shield, ShieldAlert, Wrench } from "lucide-react";
import InventorySourceBar from "../inventory/InventorySourceBar";
import ProfileIdRow from "./ProfileIdRow";
import { helperView } from "../../lib/helperStatus";
import type { MessageKey, Translator } from "../../lib/i18n";
import {
  downloadHelper,
  findAlecaFrameCache,
  getHelperStatus,
  importInventoryPath,
  onHelperDownloadProgress,
  openAlecaFrameInventoryFile,
  openInventoryFile,
  runHelperNow,
  type HelperDownloadProgress,
  type HelperStatus,
  type Settings,
} from "../../lib/ipc";
import { isMemoryMode } from "../../stores/dataSource";
import { useInventory } from "../../stores/inventory";
import { Badge, Button, Checkbox, Progress, SettingRow, Switch } from "../ui";

type Mode = Settings["data_source_mode"];

interface Props {
  tr: Translator;
  settings: Settings;
  /** Resolves to the backend's refusal, if any. */
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
  locale?: string;
}

const cardClass = (selected: boolean) =>
  `flex w-full cursor-pointer flex-col gap-3 rounded-[var(--radius-lg)] border p-4 text-left transition-colors duration-150 ${
    selected ? "border-accent bg-accent/10" : "border-border bg-bg-raised hover:border-border-strong"
  }`;

/** What each mode gives, needs and risks, shown on its card. */
const FACTS: Record<Mode, Array<{ label: MessageKey; text: MessageKey; icon: ReactNode }>> = {
  passive: [
    { label: "dataSource.gets", text: "dataSource.passive.gets", icon: <PackageCheck size={14} /> },
    { label: "dataSource.needs", text: "dataSource.passive.needs", icon: <Wrench size={14} /> },
    { label: "dataSource.risk", text: "dataSource.passive.risk", icon: <Shield size={14} /> },
  ],
  memory: [
    { label: "dataSource.gets", text: "dataSource.memory.gets", icon: <PackageCheck size={14} /> },
    { label: "dataSource.needs", text: "dataSource.memory.needs", icon: <Wrench size={14} /> },
    { label: "dataSource.risk", text: "dataSource.memory.risk", icon: <ShieldAlert size={14} className="text-warning" /> },
  ],
};

/** Passive (default) vs. game-memory mode; the picked mode's setup shows under the cards. Memory mode needs the disclaimer's checkbox ticked first. */
export default function DataSourceMode({ tr, settings, onChange, locale = "en" }: Props) {
  const active = isMemoryMode(settings);
  const [picked, setChoice] = useState<Mode>("passive");
  const choice: Mode = active ? "memory" : picked;
  const [consent, setConsent] = useState(false);
  const [error, setError] = useState("");

  const save = async (patch: Partial<Settings>) => setError((await onChange(patch)) ?? "");
  const pick = (mode: Mode) => {
    setChoice(mode);
    setConsent(false);
    if (mode === "passive" && settings.data_source_mode !== "passive") void save({ data_source_mode: "passive" });
  };

  return (
    <div className="flex flex-col gap-3 @container" data-data-source={active ? "memory" : "passive"}>
      <div className="grid gap-3 @[50rem]:grid-cols-2" role="radiogroup" aria-label={tr("dataSource.title")}>
        {(["passive", "memory"] as const).map((value) => (
          <button key={value} type="button" role="radio" aria-checked={choice === value} className={cardClass(choice === value)} data-data-source-option={value} onClick={() => pick(value)}>
            <span className="flex w-full items-center gap-2">
              <span className={`grid h-4 w-4 shrink-0 place-items-center rounded-full border ${choice === value ? "border-accent" : "border-border-strong"}`} aria-hidden>
                {choice === value && <span className="h-2 w-2 rounded-full bg-accent" />}
              </span>
              <span className="min-w-0 flex-1 text-md font-semibold text-text-primary">{tr(`dataSource.${value}.name`)}</span>
              {value === "passive" ? <Badge tone="success">{tr("common.recommended")}</Badge> : <Badge tone="warning">{tr("dataSource.memory.badge")}</Badge>}
            </span>
            <dl className="m-0 grid grid-cols-[auto_1fr] items-start gap-x-3 gap-y-1.5 text-sm">
              {FACTS[value].map((f) => (
                <div key={f.label} className="contents">
                  <dt className="flex h-[1.5em] items-center gap-1.5 whitespace-nowrap text-text-muted">
                    <span aria-hidden className="flex">
                      {f.icon}
                    </span>
                    {tr(f.label)}
                  </dt>
                  <dd className="m-0 text-text-secondary">{tr(f.text)}</dd>
                </div>
              ))}
            </dl>
          </button>
        ))}
      </div>
      <section className="flex flex-col gap-3 rounded-[var(--radius-lg)] border border-border p-4" aria-label={tr(`dataSource.${choice}.name`)} data-data-source-panel={choice}>
        {choice === "passive" ? (
          <PassiveSources tr={tr} />
        ) : (
          <>
            <div className="flex flex-col gap-2 rounded-[var(--radius-md)] border border-warning bg-warning/10 p-3">
              <p className="m-0 text-sm text-text-primary" role="note" data-memory-disclaimer>
                {tr("dataSource.disclaimer")}
              </p>
              {!active && (
                <>
                  <Checkbox label={tr("dataSource.consent")} checked={consent} data-memory-consent onChange={(e) => setConsent(e.target.checked)} />
                  <div>
                    <Button variant="primary" size="sm" disabled={!consent} data-memory-enable onClick={() => void save({ data_source_mode: "memory", memory_consent_at: Date.now() })}>
                      {tr("dataSource.enable")}
                    </Button>
                  </div>
                </>
              )}
            </div>
            {active ? <HelperPanel tr={tr} locale={locale} settings={settings} onChange={save} /> : <p className="m-0 text-sm text-text-secondary">{tr("dataSource.memory.next")}</p>}
          </>
        )}
      </section>
      {error && (
        <p className="m-0 text-xs text-danger" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/** The passive sources, each usable right here: AlecaFrame's cache, an inventory file and the profile ID. */
function PassiveSources({ tr }: { tr: Translator }) {
  const { status } = useInventory();
  const [aleca, setAleca] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useEffect(() => void findAlecaFrameCache().then(setAleca, () => {}), []);

  const load = async (job: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await job();
    } finally {
      setBusy(false);
    }
  };
  const inUse = (source: string) => !!status?.found && status.source === source;
  const alecaState = inUse("aleca") ? "inUse" : aleca ? "found" : "missing";

  return (
    <>
      <div className="flex flex-col gap-1">
        <h4 className="m-0 text-md font-semibold text-text-primary">{tr("dataSource.sources.title")}</h4>
        <p className="m-0 text-sm text-text-secondary">{tr("dataSource.sources.desc")}</p>
      </div>
      <InventorySourceBar status={status} tr={tr} imports={false} />
      <div className="flex flex-col gap-4 border-t border-border pt-4" data-passive-sources data-aleca-state={alecaState}>
        <SettingRow
          label={
            <span className="flex flex-wrap items-center gap-2">
              {tr("dataSource.aleca.name")}
              <Badge tone={alecaState === "missing" ? "neutral" : alecaState === "found" ? "success" : "accent"}>{tr(`dataSource.badge.${alecaState}`)}</Badge>
            </span>
          }
          description={tr(`dataSource.aleca.${alecaState}`)}
          control={
            aleca && alecaState === "found" ? (
              <Button size="sm" variant="primary" disabled={busy} data-aleca-use onClick={() => void load(() => importInventoryPath(aleca))}>
                {tr("dataSource.aleca.use")}
              </Button>
            ) : (
              <Button size="sm" disabled={busy} data-aleca-browse onClick={() => void load(openAlecaFrameInventoryFile)}>
                {tr("dataSource.browse")}
              </Button>
            )
          }
        />
        <SettingRow
          label={
            <span className="flex flex-wrap items-center gap-2">
              {tr("dataSource.json.name")}
              {inUse("manual") && <Badge tone="accent">{tr("dataSource.badge.inUse")}</Badge>}
            </span>
          }
          description={tr("dataSource.json.desc")}
          control={
            <Button size="sm" disabled={busy} data-json-choose onClick={() => void load(openInventoryFile)}>
              {tr("dataSource.json.choose")}
            </Button>
          }
        />
        <ProfileIdRow tr={tr} />
      </div>
    </>
  );
}

const loadStatus = (set: (s: HelperStatus) => void) => getHelperStatus().then(set, () => {});
const LINUX = typeof navigator !== "undefined" && /linux/i.test(navigator.userAgent);

/** Helper status, install, run now and the background sync switch (memory mode only). */
function HelperPanel({ tr, locale, settings, onChange }: { tr: Translator; locale: string; settings: Settings; onChange: (patch: Partial<Settings>) => Promise<void> }) {
  const [status, setStatus] = useState<HelperStatus | null>(null);
  const [progress, setProgress] = useState<HelperDownloadProgress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const inventory = useInventory().status;
  const refresh = () => void loadStatus(setStatus);

  useEffect(() => {
    void loadStatus(setStatus);
    const off = onHelperDownloadProgress(setProgress);
    return () => void off.then((f) => f());
  }, []);

  const run = async (job: () => Promise<unknown>) => {
    setBusy(true);
    setError("");
    try {
      await job();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      refresh();
    }
  };
  const install = () =>
    run(async () => {
      const r = await downloadHelper();
      if (!r.ok) throw new Error(r.error ?? tr("setup.downloadFailedConnection"));
    });

  const v = helperView(status, false, LINUX, Date.now(), locale);
  const downloading = busy && progress && progress.stage !== "done" && progress.stage !== "error";
  return (
    <div className="flex flex-col gap-2" data-helper-panel>
      <p className="m-0 text-xs text-text-secondary" data-helper-panel-status>
        {tr(v.status.key, v.status.params)}, {tr(v.tooltip)}
      </p>
      {downloading && <Progress value={progress.percent ?? 0} label={tr("setup.downloading.title")} />}
      <div className="flex flex-wrap gap-2">
        {status && !status.exeFound && (
          <Button size="sm" variant="primary" disabled={busy} data-helper-install onClick={() => void install()}>
            {tr("setup.installHelper")}
          </Button>
        )}
        <Button size="sm" disabled={busy || !status?.exeFound || status.running} data-helper-run onClick={() => void run(runHelperNow)}>
          {tr("setup.runHelperNow")}
        </Button>
      </div>
      <InventorySourceBar status={inventory} tr={tr} imports={false} />
      <SettingRow
        label={tr("settings.autoInventorySync")}
        control={<Switch checked={settings.auto_inventory_sync} aria-label={tr("settings.autoInventorySync")} onChange={(e) => void onChange({ auto_inventory_sync: e.target.checked })} />}
      />
      {error && (
        <p className="m-0 text-xs text-danger" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
