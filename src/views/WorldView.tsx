import { useMemo, type ReactNode } from "react";
import type { WorldState } from "../lib/ipc";
import type { Translator } from "../lib/i18n";
import { activeWindow, parseIsoDate, timeTo } from "../lib/format";
import { itemLabel } from "../lib/itemLabel";
import {
  COARSE_CLOCK_MS,
  FISSURE_MODE_OPTIONS,
  buildBountyGroups,
  buildBountyTimers,
  buildCycleRows,
  buildFissureRows,
  buildResetUrgency,
  buildWorldTimes,
} from "../lib/world/view";
import { Badge, Card, CardBody, EmptyState, PageHeader, Readout, Segmented, Skeleton, Tabs } from "../components/ui";
import { RelicIcon, type RelicTier } from "../components/icons";
import EditLayoutBar from "../components/layout/EditLayoutBar";
import LayoutGrid from "../components/layout/LayoutGrid";
import OpenInWindowButton from "../components/layout/OpenInWindowButton";
import WorldNotifySettings from "../components/settings/WorldNotifySettings";
import NightwaveShop from "../components/world/NightwaveShop";
import VendorRotations from "../components/world/VendorRotations";
import WorldWeekOverview from "../components/world/WorldWeekOverview";
import { registerSections } from "../lib/layout/registry";
import { isPopoutWindow } from "../stores/popout";
import ArbiSchedule from "../components/world/ArbiSchedule";
import BaroPlanner from "../components/world/BaroPlanner";
import { usePersisted } from "../lib/usePersisted";
import { useItemDatabase } from "../stores/items";
import { useWorldNotify } from "../stores/worldNotify";
import { useClock, useCollapsedSections, usePersistedFissureMode, useWorldState } from "../stores/world";
import { ChevronDown } from "lucide-react";
import WorldCard from "../components/world/WorldCard";
import IconButtonCard from "../components/world/IconButtonCard";
import { openItemBy } from "../stores/modals";
import { openDashboardSection } from "../stores/nav";
import { titleCase } from "../../config/shared/textNormalize";
import BaroInventoryCard from "../components/world/BaroInventoryCard";
import CycleRow, { TIMER_CLASS, timerColor } from "../components/world/CycleRow";
import InvasionItem from "../components/world/InvasionItem";
const TIER_BADGE: Record<string, string> = {
  lith: "world-badge-lith",
  meso: "world-badge-meso",
  neo: "world-badge-neo",
  axi: "world-badge-axi",
  requiem: "world-badge-requiem",
  omnia: "world-badge-omnia",
};
const RELIC_TIERS: ReadonlySet<string> = new Set(["lith", "meso", "neo", "axi", "requiem"]);

const WORLD_MAIN_SECTIONS = [
  "world.weekOverview", "world.cycles", "world.timers", "world.resurgence", "world.circuit", "world.steelPath", "world.fissures",
  "world.fissureAlerts", "world.invasions", "world.darvo", "world.vendors", "world.nightwaveShop", "world.baro", "world.bounties",
];
const WORLD_ARBI_SECTIONS = ["world.arbiSchedule"];

// Order is the default column split: the first half fills the wide left column.
registerSections("world", [
  { id: "world.weekOverview", view: "world", labelKey: "world.thisWeek", defaultSpan: "full" },
  { id: "world.cycles", view: "world", labelKey: "world.planetCycles", defaultSpan: 1, canPopout: true },
  { id: "world.timers", view: "world", labelKey: "world.resetTimers", defaultSpan: 1 },
  { id: "world.resurgence", view: "world", labelKey: "world.primeResurgence", defaultSpan: 1 },
  { id: "world.circuit", view: "world", labelKey: "world.theCircuit", defaultSpan: 1 },
  { id: "world.steelPath", view: "world", labelKey: "world.steelPathHonorsReset", defaultSpan: 1 },
  { id: "world.fissures", view: "world", labelKey: "world.voidFissures", defaultSpan: 1, canPopout: true },
  { id: "world.fissureAlerts", view: "world", labelKey: "common.alerts", defaultSpan: 1, canPopout: true },
  { id: "world.invasions", view: "world", labelKey: "world.invasions", defaultSpan: 1, canPopout: true },
  { id: "world.darvo", view: "world", labelKey: "world.darvosDeal", defaultSpan: 1 },
  { id: "world.vendors", view: "world", labelKey: "world.vendorRotations", defaultSpan: 1 },
  { id: "world.nightwaveShop", view: "world", labelKey: "world.nightwaveShop", defaultSpan: 1 },
  { id: "world.baro", view: "world", labelKey: "world.baroKiteer", defaultSpan: "full", canPopout: true },
  { id: "world.bounties", view: "world", labelKey: "world.bounties", defaultSpan: "full" },
  { id: "world.arbiSchedule", view: "world", labelKey: "common.arbitrations", defaultSpan: "full", minSpan: "full", canHide: false, canPopout: true },
]);

interface Props {
  tr: Translator;
  locale: string;
}

const Row = ({ label, value, urgent }: { label: string; value: string; urgent: boolean }) => (
  <div className="flex items-center justify-between border-b border-border py-2 last:border-b-0">
    <span className="text-sm text-text-secondary">{label}</span>
    <span className={`${TIMER_CLASS} ${timerColor(urgent)}`}>{value}</span>
  </div>
);
const Muted = ({ children }: { children: ReactNode }) => <span className="text-sm text-text-muted">{children}</span>;
const Strip = ({ children }: { children: ReactNode }) => <div className="-m-1 flex gap-3 overflow-x-auto p-1">{children}</div>;
const Eyebrow = ({ children, warn = false }: { children: ReactNode; warn?: boolean }) => <div className={`mb-1.5 text-xs font-semibold ${warn ? "text-warning" : "text-text-muted"}`}>{children}</div>;

function WorldMain({ tr, locale, onOpenDailies }: Props & { onOpenDailies: () => void }) {
  const { data: wd, loading, error } = useWorldState();
  const db = useItemDatabase();
  const nowMs = useClock(1000);
  const nowCoarseMs = useClock(COARSE_CLOCK_MS);
  const [fissureMode, setFissureMode] = usePersistedFissureMode();
  const [collapsed, toggle] = useCollapsedSections();
  const notify = useWorldNotify();

  // Circuit picks and the resurgence rotation arrive as display names / paths; join the item DB for art.
  const byName = useMemo(() => new Map(Object.entries(db).map(([uniqueName, e]) => [e.name, { ...e, uniqueName }])), [db]);

  if (!wd) {
    return loading ? (
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2" data-world-state="loading" aria-label={tr("world.loading")}>
        <Skeleton height="14rem" />
        <Skeleton height="14rem" />
      </div>
    ) : (
      <div data-world-state="unavailable">
        <EmptyState title={tr("world.unavailable")} />
      </div>
    );
  }

  return <Loaded {...{ wd, tr, locale, db, byName, nowMs, nowCoarseMs, fissureMode, setFissureMode, collapsed, toggle, error, notify, onOpenDailies }} />;
}

interface LoadedProps extends Props {
  onOpenDailies: () => void;
  wd: WorldState;
  notify: ReturnType<typeof useWorldNotify>;
  db: ReturnType<typeof useItemDatabase>;
  byName: Map<string, ReturnType<typeof useItemDatabase>[string] & { uniqueName: string }>;
  nowMs: number;
  nowCoarseMs: number;
  fissureMode: ReturnType<typeof usePersistedFissureMode>[0];
  setFissureMode: ReturnType<typeof usePersistedFissureMode>[1];
  collapsed: Record<string, boolean>;
  toggle: (key: string) => void;
  error: string | null;
}

function Loaded({ wd, tr, locale, db, byName, nowMs, nowCoarseMs, fissureMode, setFissureMode, collapsed, toggle, error, notify, onOpenDailies }: LoadedProps) {
  const { voidTrader: baro, vaultTrader: varzia, steelPath, duviriCycle: duviri } = wd;
  const baroActive = activeWindow(baro?.activation, baro?.expiry, nowCoarseMs);
  const baroAct = parseIsoDate(baro?.activation);
  const varziaActive = activeWindow(varzia?.activation, varzia?.expiry, nowCoarseMs);
  const times = buildWorldTimes(wd, baroActive, varziaActive, nowMs);
  const urgency = buildResetUrgency(wd, nowCoarseMs);
  const cycleRows = buildCycleRows(wd, times, nowCoarseMs, tr);
  const fissures = buildFissureRows(wd.fissures, fissureMode, nowMs, nowCoarseMs);
  const invasions = wd.invasions.filter((inv) => !inv.completed);
  const bounties = buildBountyGroups(wd.bounties);
  const bountyTimers = buildBountyTimers(bounties, nowMs, nowCoarseMs);
  const bountyColumns = [bounties.filter((_, i) => i % 2 === 0), bounties.filter((_, i) => i % 2 === 1)];
  const modeLabel = FISSURE_MODE_OPTIONS.find((o) => o.value === fissureMode);
  const baroLocation = baro?.location || null;

  const daysUntil = (iso: string) => {
    const ms = Date.parse(iso) - nowCoarseMs;
    if (!Number.isFinite(ms)) return tr("world.soon");
    const days = Math.max(1, Math.ceil(ms / 86_400_000));
    return days === 1 ? tr("world.inDaySingular", { days }) : tr("world.inDaysPlural", { days });
  };

  const circuit = (category: "normal" | "hard") => duviri?.choices.find((c) => c.category === category)?.choices ?? [];
  const itemStrip = (names: string[]) =>
    names.length === 0 ? (
      <Muted>{tr("world.noData")}</Muted>
    ) : (
      <Strip>
        {names.map((name) => {
          const entry = byName.get(name);
          return <IconButtonCard key={name} name={itemLabel(name, entry)} imageUrl={entry?.imageUrl ?? null} onClick={() => entry && openItemBy({ uniqueName: entry.uniqueName })} tr={tr} />;
        })}
      </Strip>
    );
  const circuitRotation = (label: string, names: string[], steel: boolean) => (
    <div key={label} className="mb-3 last:mb-0">
      <Eyebrow warn={steel}>{label}</Eyebrow>
      {itemStrip(names)}
    </div>
  );

  const card = (key: string, title: string, children: ReactNode, tools?: ReactNode, flush = false) => (
    <WorldCard title={title} collapsed={!!collapsed[key]} onToggle={() => toggle(key)} tools={tools} flush={flush}>
      {children}
    </WorldCard>
  );

  const known: Record<string, ReactNode> = {
    cycles: card(
      "cycles",
      tr("world.planetCycles"),
      cycleRows.length > 0 ? (
        <div className="grid grid-cols-1 gap-x-6 @3xl:grid-cols-2">
          {cycleRows.map((row) => (
            <CycleRow
              key={row.key}
              name={titleCase(row.key)}
              planet={row.key}
              stateLabel={row.stateLabel}
              stateClass={row.stateClass}
              nextLabel={row.nextLabel}
              time={row.time}
              urgent={row.urgent}
              tr={tr}
              bell={notify.prefs ? { on: notify.prefs.cycleAlerts[row.key], title: tr("world.notifyBeforeCycleChange"), onToggle: () => void notify.toggleCycle(row.key) } : undefined}
            />
          ))}
        </div>
      ) : (
        <Muted>{tr("world.cycleDataUnavailable")}</Muted>
      ),
    ),
    timers: card(
      "timers",
      tr("world.resetTimers"),
      <>
        <Row label={tr("world.dailySortie")} value={times.sortie} urgent={urgency.sortie} />
        <Row label={tr("world.dailyReset")} value={times.daily} urgent={urgency.daily} />
        <Row label={tr("world.weeklyResets")} value={times.weekly} urgent={urgency.weekly} />
        <Row label={tr("world.steelPathHonorsReset")} value={times.steelPath} urgent={urgency.steelPath} />
      </>,
    ),
    resurgence: card(
      "resurgence",
      tr("world.primeResurgence"),
      <>
        <div className="mb-3 text-sm text-text-secondary">
          {tr("world.rotationEndsIn")} <strong className="tabular-nums text-text-primary">{times.varzia}</strong>
        </div>
        {varzia && varzia.inventory.length > 0 ? (
          <Strip>
            {varzia.inventory.map((i) => (
              <IconButtonCard key={i.uniqueName} name={itemLabel(i.uniqueName, db[i.uniqueName])} imageUrl={db[i.uniqueName]?.imageUrl ?? null} onClick={() => db[i.uniqueName] && openItemBy({ uniqueName: i.uniqueName })} tr={tr} />
            ))}
          </Strip>
        ) : (
          <Muted>{tr("world.noFeaturedPrimes")}</Muted>
        )}
      </>,
    ),
    circuit: card("circuit", tr("world.theCircuit"), <>{circuitRotation(tr("world.normalRotation"), circuit("normal"), false)}{circuitRotation(tr("world.steelPathRotation"), circuit("hard"), true)}</>),
    steel: steelPath
      ? card(
          "steelpath",
          tr("world.steelPathHonorsReset"),
          <>
            <div className="flex items-center gap-2 pb-2">
              <span className="shrink-0 text-xs text-text-muted">{tr("world.thisWeek")}</span>
              <span className="min-w-0 flex-1 text-sm font-semibold text-warning">{steelPath.currentReward.name}</span>
              <Badge>{tr("world.steelEssenceCost", { cost: steelPath.currentReward.cost })}</Badge>
            </div>
            {steelPath.upcoming.map((u) => (
              <div key={u.weekOffset} className="flex items-center gap-2 border-t border-border py-1.5">
                <span className="shrink-0 text-sm text-text-muted">{daysUntil(u.activation)}</span>
                <span className="min-w-0 flex-1 text-sm text-text-primary">{u.name}</span>
                <span className="shrink-0 whitespace-nowrap text-xs text-text-secondary">{tr("world.steelEssenceCost", { cost: u.cost })}</span>
              </div>
            ))}
          </>,
          <span className={`${TIMER_CLASS} text-text-primary`}>{times.steelPath}</span>,
        )
      : null,
    fissures: card(
      "fissures",
      tr("world.voidFissures"),
      <div className="flex flex-col">
        {fissures.length === 0 ? (
          <Muted>{fissureMode === "all" ? tr("world.noFissuresAny") : tr("world.noActiveFissures", { mode: modeLabel ? tr(modeLabel.labelKey) : "" })}</Muted>
        ) : (
          fissures.map((f) => (
            <div key={`${f.nodeId}-${f.tier}-${f.expiry}`} className="flex items-center gap-3 border-b border-border py-2 last:border-b-0" data-fissure={f.tierCls}>
              <span className="inline-flex w-24 shrink-0 items-center gap-1.5">
                {RELIC_TIERS.has(f.tierCls) || f.tierCls === "omnia" ? (
                  <RelicIcon tier={(f.tierCls === "omnia" ? "requiem" : f.tierCls) as RelicTier} label={f.tier} />
                ) : null}
                <span className={`rounded-[var(--radius-sm)] px-1.5 py-0.5 text-xs font-semibold ${TIER_BADGE[f.tierCls] ?? ""}`}>{f.tier}</span>
              </span>
              <span className="min-w-0 flex-1 text-sm">
                <strong className="font-medium text-text-primary">{f.missionType || tr("common.mission")}</strong>
                {f.isHard && (f.sourceMode === "railjack" || fissureMode === "all") && <Badge tone="warning" className="ml-1.5">{tr("world.spBadge")}</Badge>}
                <span className="ml-1.5 text-xs text-text-muted">{f.node || tr("common.unknown")}</span>
              </span>
              <span className={`${TIMER_CLASS} shrink-0 text-text-primary`}>{f.timeStr}</span>
            </div>
          ))
        )}
      </div>,
      <Segmented label={tr("world.voidFissures")} active={fissureMode} onSelect={setFissureMode} items={FISSURE_MODE_OPTIONS.map((o) => ({ key: o.value, label: tr(o.labelKey) }))} />,
    ),
    invasions:
      invasions.length > 0
        ? card("invasions", tr("world.invasions"), <div className="flex flex-col">{invasions.map((inv) => <InvasionItem key={inv.id} inv={inv} tr={tr} locale={locale} />)}</div>)
        : null,
    darvo:
      wd.dailyDeals.length > 0
        ? card(
            "darvo",
            tr("world.darvosDeal"),
            wd.dailyDeals.map((deal) => {
              const img = db[deal.uniqueName]?.imageUrl || deal.imageOverride;
              const name = deal.item || tr("common.unknown");
              return (
                <div key={deal.uniqueName} className="flex items-center gap-3 py-1" data-darvo-deal={deal.uniqueName}>
                  <span className="ui-thumb !h-16 !w-16 !rounded-[var(--radius-lg)]" title={name}>
                    {img ? <img src={img} alt={name} loading="lazy" /> : name[0]}
                  </span>
                  <div className="flex min-w-0 flex-col gap-1">
                    <span className="truncate text-sm font-semibold text-text-primary">{name}</span>
                    <span className="flex flex-wrap items-center gap-2">
                      <Readout currency="platinum" chip value={deal.salePrice} />
                      <span className="text-xs tabular-nums text-text-muted line-through">{deal.originalPrice}</span>
                      <Badge tone="success">-{deal.discount}%</Badge>
                    </span>
                    <span className="text-xs text-text-muted">
                      {tr("world.soldOfTotal", { sold: deal.sold, total: deal.total })} {deal.expiry && tr("world.endsIn", { time: timeTo(parseIsoDate(deal.expiry), nowCoarseMs) })}
                    </span>
                  </div>
                </div>
              );
            }),
          )
        : null,
  };

  const baroPill = (baroActive || baroAct) && (
    <Badge tone="warning" className="whitespace-nowrap" icon={undefined}>
      <span data-world-baro-pill>
        {tr(baroActive ? "world.baroLeavesIn" : "world.baroArrivesIn", { baro: times.baro })}
        {baroLocation ? ` - ${baroLocation}` : ""}
      </span>
    </Badge>
  );

  const baroPresent = baroActive ? !!baro && baro.inventory.length > 0 : !!baro && !!baroAct;
  const parts: Record<string, ReactNode> = {
    "world.weekOverview": <WorldWeekOverview wd={wd} nowMs={nowMs} tr={tr} onOpenDailies={onOpenDailies} />,
    "world.cycles": known.cycles,
    "world.timers": known.timers,
    "world.resurgence": known.resurgence,
    "world.circuit": known.circuit,
    "world.steelPath": known.steel,
    "world.fissures": known.fissures,
    "world.fissureAlerts": (
      <Card>
        <CardBody>
          <WorldNotifySettings tr={tr} />
        </CardBody>
      </Card>
    ),
    "world.invasions": known.invasions,
    "world.darvo": known.darvo,
    "world.vendors": card("vendors", tr("world.vendorRotations"), <VendorRotations vendors={["coda", "tenet"]} variant="section" tr={tr} />),
    "world.nightwaveShop": card("nightwaveShop", tr("world.nightwaveShop"), <NightwaveShop wd={wd} tr={tr} locale={locale} />),
    "world.baro":
      baro && !baroActive && baroAct ? (
        <WorldCard title={tr("world.baroKiteer")} tools={<Badge>{tr("world.inactive")}</Badge>}>
          <span className="text-sm tabular-nums text-text-secondary">
            {times.baro}
            {baroLocation ? ` - ${baroLocation}` : ""}
          </span>
        </WorldCard>
      ) : baro && baroActive && baro.inventory.length > 0 ? (
        card(
          "baro",
          tr("world.baroKiteer"),
          <>
            <div className="flex items-center justify-between pb-3 text-sm text-text-secondary">
              <span>{baroLocation}</span>
              <span className="text-xs">
                {tr("world.leavesIn")} <strong className="tabular-nums text-text-primary">{times.baro}</strong>
              </span>
            </div>
            <div className="flex flex-wrap gap-3">
              {baro.inventory.map((entry) => (
                <BaroInventoryCard key={entry.uniqueName} entry={entry} dbEntry={db[entry.uniqueName]} tr={tr} locale={locale} />
              ))}
            </div>
          </>,
        )
      ) : null,
    "world.bounties":
      bounties.length > 0
        ? card(
            "bounties",
            tr("world.bounties"),
            <div className="grid grid-cols-1 gap-x-6 gap-y-1 @3xl:grid-cols-2">
              {bountyColumns.map((column, ci) => (
                <div key={ci} className="flex min-w-0 flex-col">
                  {column.map((group) => {
                    const key = `bounty-${group.syndicateKey}`;
                    const timer = bountyTimers[group.syndicateKey];
                    return (
                      <div key={key} className="border-b border-border py-1 last:border-b-0" data-bounty-group={group.syndicateKey}>
                        <button type="button" className="flex w-full cursor-pointer items-center gap-2 border-0 bg-transparent py-1.5 text-left text-inherit" onClick={() => toggle(key)} aria-expanded={!collapsed[key]}>
                          <ChevronDown className={`h-4 w-4 shrink-0 text-text-muted transition-transform ${collapsed[key] ? "-rotate-90" : ""}`} aria-hidden="true" />
                          <span className="text-sm font-semibold text-text-primary">{group.syndicate}</span>
                          {timer?.timeStr && <span className={`${TIMER_CLASS} ${timerColor(timer.urgent)}`}>{timer.timeStr}</span>}
                          <span className="ml-auto text-xs text-text-muted">{tr("world.bountiesCount", { count: group.jobs.length })}</span>
                        </button>
                        {!collapsed[key] && (
                          <div className="flex flex-col pb-1 pl-6">
                            {group.jobs.map((job, ji) => (
                              <div key={ji} className="flex items-center gap-2 py-1 text-sm">
                                <span className="min-w-0 flex-1 truncate text-text-primary">
                                  {titleCase(job.type)}
                                  {job.challengeDesc && <span className="text-text-muted"> - {job.challengeDesc}</span>}
                                </span>
                                <Badge>{job.enemyLevels[0]}-{job.enemyLevels[1]}</Badge>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              ))}
            </div>,
          )
        : null,
  };
  const available = [
    "world.weekOverview",
    "world.nightwaveShop",
    "world.cycles",
    "world.timers",
    "world.resurgence",
    "world.circuit",
    ...(steelPath ? ["world.steelPath"] : []),
    "world.fissures",
    "world.fissureAlerts",
    ...(invasions.length > 0 ? ["world.invasions"] : []),
    ...(wd.dailyDeals.length > 0 ? ["world.darvo"] : []),
    "world.vendors",
    ...(baroPresent ? ["world.baro"] : []),
    ...(bounties.length > 0 ? ["world.bounties"] : []),
  ];

  return (
    <div className="@container" data-world-state="ready">
      {(error || baroPill) && (
        <div className="mb-3 flex items-center justify-between gap-3">
          {error ? <span className="text-xs text-warning" data-world-error>{tr("world.fetchFailed", { error })}</span> : <span />}
          {baroPill}
        </div>
      )}
      <LayoutGrid view="world" tr={tr} only={WORLD_MAIN_SECTIONS} available={available} wideTemplate="minmax(0, 1.2fr) minmax(0, 1fr)" className="world-layout" gapClass="gap-4" columnGapClass="gap-4">
        {(id) => parts[id] ?? null}
      </LayoutGrid>
    </div>
  );
}

export const WORLD_TABS = ["world", "arbis", "baro"] as const;

export default function WorldView({ tr, locale }: Props) {
  const [tab, setTab] = usePersisted("world-tab", WORLD_TABS, "world");
  const scope = tab === "arbis" ? WORLD_ARBI_SECTIONS : WORLD_MAIN_SECTIONS;
  return (
    <div className="w-full">
      <PageHeader
        title={tr("common.world")}
        actions={
          <>
            {!isPopoutWindow && <OpenInWindowButton target={{ kind: "view", view: "world" }} tr={tr} data-layout-popout="world" className="ui-icon-btn" />}
            {tab !== "baro" && <EditLayoutBar view="world" tr={tr} only={scope} />}
          </>
        }
      />
      <Tabs
        label={tr("common.world")}
        className="mb-4"
        active={tab}
        onSelect={setTab}
        items={[
          { key: "world", label: tr("common.world") },
          { key: "arbis", label: tr("common.arbitrations") },
          { key: "baro", label: tr("marketAlerts.kindBaro") },
        ]}
      />
      {tab === "baro" ? (
        <BaroPlanner tr={tr} locale={locale} />
      ) : tab === "arbis" ? (
        <LayoutGrid view="world" tr={tr} only={WORLD_ARBI_SECTIONS} gapClass="gap-0">
          {(id) => (id === "world.arbiSchedule" ? <ArbiSchedule tr={tr} locale={locale} /> : null)}
        </LayoutGrid>
      ) : (
        <WorldMain tr={tr} locale={locale} onOpenDailies={() => openDashboardSection("dashboard.dailies")} />
      )}
    </div>
  );
}
