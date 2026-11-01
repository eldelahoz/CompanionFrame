import { useEffect } from "react";
import ItemImage from "../components/ItemImage";
import { Card, CardHeader, EmptyState, PageHeader, StatStrip } from "../components/ui";
import EditLayoutBar from "../components/layout/EditLayoutBar";
import LayoutGrid from "../components/layout/LayoutGrid";
import { formatNumber } from "../lib/format";
import type { Translator } from "../lib/i18n";
import type { ItemDatabase } from "../lib/ipc";
import { itemLabel } from "../lib/itemLabel";
import { registerSections } from "../lib/layout/registry";
import { COARSE_CLOCK_MS } from "../lib/world/view";
import { dashboardSectionDescriptors } from "../lib/widgets/registry";
import { BaroWidget, CyclesWidget, FissuresWidget } from "../components/widgets/WorldWidgets";
import { FoundryReadyWidget, GoalsWidget, InventoryValueWidget, MarketAlertsWidget, TradeSummaryWidget } from "../components/widgets/DataWidgets";
import { LastMissionWidget, RecentRunsWidget } from "../components/widgets/RunsMissionWidgets";
import DailiesTracker from "../components/world/DailiesTracker";
import { requestedDashboardSection } from "../stores/nav";
import { memoryMode } from "../stores/dataSource";
import { useClock, usePersistedFissureMode, useWorldState } from "../stores/world";
import { useInventory, useParsedInventory, useResources } from "../stores/inventory";

interface Props {
  items: ItemDatabase;
  tr: Translator;
  locale: string;
}

// Module scope so the sections exist for the layout store and edit bar before the view mounts.
const SECTIONS = [
  ...dashboardSectionDescriptors(),
  { id: "dashboard.inventory", view: "dashboard" as const, labelKey: "common.inventory" as const, defaultSpan: 1 as const, canPopout: true },
  { id: "dashboard.resources", view: "dashboard" as const, labelKey: "nav.resources" as const, defaultSpan: 1 as const, canPopout: true },
];
registerSections("dashboard", SECTIONS);
/** The last-mission widget reads game memory; passive mode keeps no slot for it. */
const PASSIVE_SECTIONS = SECTIONS.map((s) => s.id).filter((id) => id !== "dashboard.lastMission");

const TOP_RESOURCES = 5;

// The view owns the clocks and the world poll and hands them down, so no widget starts a timer of its own.
export default function DashboardView({ items, tr, locale }: Props) {
  const { data: wd, loading } = useWorldState();
  const nowMs = useClock(1000);
  const nowCoarseMs = useClock(COARSE_CLOCK_MS);
  const [fissureMode] = usePersistedFissureMode();
  const world = { wd, loading, nowMs, nowCoarseMs, tr };
  const { data } = useInventory();
  const parsed = useParsedInventory(data, items);
  const resources = useResources(data, items);
  const owned = parsed.filter((i) => i.inventoryGroup !== "full_sets" && i.inventoryGroup !== "incomplete_sets");
  const primes = owned.filter((i) => i.isPrime).length;
  const memory = memoryMode.use();

  // Another view asked for a widget ("Open daily and weekly tasks"): scroll to it once laid out and flash it.
  const wanted = requestedDashboardSection.use();
  useEffect(() => {
    if (!wanted) return;
    requestedDashboardSection.set(null);
    requestAnimationFrame(() => {
      const el = document.querySelector<HTMLElement>(`[data-layout-section="${wanted}"]`);
      el?.scrollIntoView({ behavior: "smooth", block: "start" });
      el?.animate([{ outline: "2px solid var(--accent)", outlineOffset: "2px" }, { outline: "2px solid transparent", outlineOffset: "2px" }], { duration: 1600 });
    });
  }, [wanted]);

  return (
    <div className="w-full">
      <PageHeader title={tr("nav.dashboard")} actions={<EditLayoutBar view="dashboard" tr={tr} />} />
      <LayoutGrid view="dashboard" tr={tr} gapClass="gap-4" columnGapClass="gap-4" available={memory ? null : PASSIVE_SECTIONS}>
        {(id) => (
          <>
            {id === "dashboard.cycles" && <CyclesWidget {...world} />}
            {id === "dashboard.fissures" && <FissuresWidget {...world} mode={fissureMode} />}
            {id === "dashboard.baro" && <BaroWidget {...world} />}
            {id === "dashboard.lastMission" && memory && <LastMissionWidget tr={tr} locale={locale} items={items} />}
            {id === "dashboard.recentRuns" && <RecentRunsWidget tr={tr} locale={locale} />}
            {id === "dashboard.dailies" && <DailiesTracker wd={wd} tr={tr} />}
            {id === "dashboard.foundryReady" && <FoundryReadyWidget tr={tr} items={items} nowCoarseMs={nowCoarseMs} />}
            {id === "dashboard.goals" && <GoalsWidget tr={tr} locale={locale} items={items} />}
            {id === "dashboard.marketAlerts" && <MarketAlertsWidget tr={tr} locale={locale} />}
            {id === "dashboard.inventoryValue" && <InventoryValueWidget tr={tr} locale={locale} items={items} />}
            {id === "dashboard.tradeSummary" && <TradeSummaryWidget tr={tr} locale={locale} />}
            {id === "dashboard.inventory" && (
              <section data-widget="inventory">
                {data ? (
                  <StatStrip
                    items={[
                      { key: "owned", label: tr("common.inventory"), value: <span data-stat="owned">{formatNumber(owned.length)}</span> },
                      { key: "primes", label: tr("common.prime"), value: <span data-stat="primes">{formatNumber(primes)}</span> },
                      { key: "resources", label: tr("nav.resources"), value: <span data-stat="resources">{formatNumber(resources.length)}</span> },
                    ]}
                  />
                ) : (
                  <Card>
                    <CardHeader title={tr("common.inventory")} />
                    <EmptyState title={tr("app.noInventoryLoaded")}>{tr("noInventory.widget")}</EmptyState>
                  </Card>
                )}
              </section>
            )}
            {id === "dashboard.resources" && (
              <Card data-widget="resources">
                <CardHeader title={tr("nav.resources")} />
                {resources.length === 0 ? (
                  <EmptyState title={tr(data ? "resources.noResourcesFound" : "noInventory.widget")} />
                ) : (
                  <ul className="ui-list">
                    {resources.slice(0, TOP_RESOURCES).map((r) => (
                      <li key={r.internalName} className="text-sm">
                        <span className="ui-thumb">
                          <ItemImage src={r.imageUrl} alt="" cls="h-full w-full object-contain" tr={tr} />
                        </span>
                        <span className="min-w-0 flex-1 truncate">{itemLabel(r.internalName, r)}</span>
                        <span className="font-semibold tabular-nums">{formatNumber(r.count)}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </Card>
            )}
          </>
        )}
      </LayoutGrid>
    </div>
  );
}
