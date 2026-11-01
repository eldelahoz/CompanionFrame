import { useEffect, useMemo, useState } from "react";
import { fallbackNameFromUniqueName } from "../../../config/shared/displayName";
import { formatNumber } from "../../lib/format";
import type { Translator } from "../../lib/i18n";
import { isFoundryBuildClaimable, isFoundryRecipeReady } from "../../lib/inventory/foundryResources";
import { buildRows, valueEntries } from "../../lib/inventory/inventoryRows";
import { useItemPrices } from "../../lib/inventory/itemPrices";
import { computeInventoryValueTotals } from "../../lib/inventory/valueTotals";
import { marketAlertsHitsGet, marketAlertsStatus, type ItemDatabase, type MarketAlertEngineStatus, type MarketAlertHit } from "../../lib/ipc";
import { buildMasteryPlan, type PlannerPin } from "../../lib/masteryPlanner";
import { pageLedgerRange } from "../../lib/stats/ledgerPaging";
import { computeFlow, type PlatFlow } from "../../lib/stats/tradeAnalytics";
import { withRowKeys } from "../../lib/widgets/registry";
import { settingBoolean, settingNumber, useWidgetSettings } from "../../stores/dashboard";
import { useMasteryProgress } from "../../stores/mastery";
import { chainFor, foundryFor, ownershipFor, useInventory, useParsedInventory } from "../../stores/inventory";
import { useMasteryPins } from "../../stores/masteryPins";
import { useWfmCatalog } from "../../stores/market";
import type { ItemDbEntry } from "../../types/inventory";
import { Badge, Readout } from "../ui";
import WidgetFrame, { WIDGET_LIST, WIDGET_SUBTITLE } from "./WidgetFrame";

interface Props {
  tr: Translator;
  locale: string;
}
interface ItemsProps extends Props {
  items: ItemDatabase;
}

// A month of trades is small; the cap only stops a pathological ledger.
const MAX_MONTH_EVENTS = 2000;

export function InventoryValueWidget({ tr, locale, items }: ItemsProps) {
  const allTradables = settingBoolean(useWidgetSettings("widget.inventoryValue"), "allTradables", false);
  const scope = allTradables ? "tradable" : "prime";
  const { data } = useInventory();
  const parsed = useParsedInventory(data, items);
  const { catalog } = useWfmCatalog();
  const rows = useMemo(() => buildRows(parsed, catalog), [parsed, catalog]);
  // Same rows and prices the Inventory value strip counts.
  const slugs = useMemo(() => rows.flatMap((r) => (r.slug && (allTradables || (r.item.inventoryGroup === "all_parts" && r.item.isPrime)) ? [r.slug] : [])), [rows, allTradables]);
  const prices = useItemPrices(slugs, []);
  const totals = useMemo(() => computeInventoryValueTotals(valueEntries(rows, prices), scope, 0), [rows, prices, scope]);
  return (
    <WidgetFrame
      widgetId="widget.inventoryValue"
      tr={tr}
      empty={totals.counted === 0}
      emptyKey={data ? "inventory.noItemsFound" : "noInventory.widget"}
      subtitle={<p className={WIDGET_SUBTITLE} data-widget-status>{tr(allTradables ? "inventory.value.allTradables" : "inventory.value.scopePrime")}</p>}
    >
      <div className="flex flex-col gap-2 px-4 py-3">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          <Readout currency="platinum" size="lg" label={totals.platinumUnpriced > 0 ? "≥" : undefined} value={totals.platinum} locale={locale} />
          <Readout currency="ducat" chip label={totals.ducatsUnpriced > 0 ? "≥" : undefined} value={totals.ducats} locale={locale} />
        </div>
        {totals.unpriced > 0 && <p className="m-0 text-xs text-text-muted">{tr("inventory.value.unpriced", { count: totals.unpriced })}</p>}
      </div>
    </WidgetFrame>
  );
}

export function FoundryReadyWidget({ tr, items, nowCoarseMs }: Omit<ItemsProps, "locale"> & { nowCoarseMs: number }) {
  const limit = settingNumber(useWidgetSettings("widget.foundryReady"), "limit", 5);
  const { data } = useInventory();
  const foundry = foundryFor(data, items);
  const owned = ownershipFor(data, items);
  // Walking the recipe chain is the expensive half and only moves with the inventory.
  const chain = chainFor(foundry, owned, items);
  const rows = withRowKeys(
    [
      ...foundry.building.filter((b) => isFoundryBuildClaimable(b, nowCoarseMs)).map((b) => ({ key: `building:${b.productUniqueName ?? b.name}`, name: b.displayName || b.name, claimable: true })),
      ...foundry.recipes.filter((r) => isFoundryRecipeReady(r, owned, chain)).map((r) => ({ key: `recipe:${r.uniqueName ?? r.name}`, name: r.displayName || r.name, claimable: false })),
    ],
    (r) => r.key,
  );
  const shown = rows.slice(0, limit);
  return (
    <WidgetFrame widgetId="widget.foundryReady" tr={tr} empty={rows.length === 0} emptyKey={data ? "foundry.noItems" : "noInventory.widget"} overflow={rows.length - shown.length}>
      <ul className={WIDGET_LIST}>
        {shown.map((row) => (
          <li key={row.rowKey} className="text-sm">
            <span className="min-w-0 flex-1 truncate text-text-secondary">{row.name}</span>
            <Badge tone={row.claimable ? "success" : "neutral"}>{tr(row.claimable ? "mastery.roadmap.accessClaimable" : "foundry.status.readyToBuild")}</Badge>
          </li>
        ))}
      </ul>
    </WidgetFrame>
  );
}

export function GoalsWidget({ tr, items }: ItemsProps) {
  const limit = settingNumber(useWidgetSettings("widget.goals"), "limit", 5);
  const db = items as unknown as Record<string, ItemDbEntry>;
  const pins = useMasteryPins();
  const { data: inventory } = useInventory();
  const { data: mastery } = useMasteryProgress();
  const ownership = ownershipFor(inventory, items);
  // One crafting-tree walk per pin, so it only reruns when the pins or the inventory move.
  const plan = useMemo(() => {
    if (pins.length === 0) return null;
    const byName = new Map((mastery?.items ?? []).map((i) => [i.uniqueName, i]));
    const planned: PlannerPin[] = pins.map((uniqueName) => {
      const match = byName.get(uniqueName);
      const entry = db[uniqueName];
      const displayName = match?.displayName || entry?.displayName;
      return { uniqueName, name: match?.name || entry?.name || fallbackNameFromUniqueName(uniqueName), ...(displayName ? { displayName } : {}), imageUrl: match?.imageUrl ?? entry?.imageUrl ?? null, masteryXpRemaining: match?.masteryXpRemaining ?? 0 };
    });
    return buildMasteryPlan(planned, db, ownership);
  }, [pins, mastery, db, ownership]);
  const rows = [...(plan?.items ?? [])].sort((a, b) => b.masteryXpRemaining - a.masteryXpRemaining).slice(0, limit);
  return (
    <WidgetFrame
      widgetId="widget.goals"
      tr={tr}
      empty={pins.length === 0}
      emptyKey="mastery.planner.empty"
      subtitle={<p className={WIDGET_SUBTITLE} data-widget-status>{tr("mastery.planner.pinnedCount", { count: String(pins.length) })}, {tr("mastery.planner.craftableCount", { count: String(plan?.craftableCount ?? 0) })}</p>}
    >
      <ul className={WIDGET_LIST}>
        {rows.map((item) => (
          <li key={item.uniqueName} className="text-sm">
            <span className="min-w-0 flex-1 truncate text-text-secondary">{item.displayName || item.name}</span>
            {item.craftableNow && <Badge tone="success">{tr("mastery.planner.craftableNow")}</Badge>}
            <span className="shrink-0 font-semibold tabular-nums text-text-primary">{formatNumber(item.masteryXpRemaining)}</span>
          </li>
        ))}
      </ul>
    </WidgetFrame>
  );
}

export function MarketAlertsWidget({ tr, locale }: Props) {
  const limit = settingNumber(useWidgetSettings("widget.marketAlerts"), "limit", 5);
  const [state, setState] = useState<{ hits: MarketAlertHit[]; status: MarketAlertEngineStatus } | null>(null);
  const [failed, setFailed] = useState(false);
  // Read-only, once per mount: the engine owns the polling and a second interval here would double the request rate.
  useEffect(() => {
    let live = true;
    Promise.all([marketAlertsHitsGet(), marketAlertsStatus()]).then(
      ([hits, status]) => live && setState({ hits, status }),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, []);
  const hits = state?.hits ?? [];
  const shown = hits.slice(0, limit);
  const status = state?.status;
  const time = (at: string) => {
    const d = new Date(at);
    return Number.isNaN(d.getTime()) ? "" : d.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  };
  return (
    <WidgetFrame
      widgetId="widget.marketAlerts"
      tr={tr}
      loading={state === null && !failed}
      errorKey={failed ? "dashboard.widgetError" : null}
      empty={shown.length === 0}
      emptyKey="marketAlerts.noHits"
      overflow={hits.length - shown.length}
      subtitle={
        status && (
          <p className={WIDGET_SUBTITLE} data-widget-status>
            {tr(status.running ? "marketAlerts.healthOk" : "marketAlerts.engineStopped")}, {tr("marketAlerts.enabledCount", { enabled: String(status.enabledCount), total: String(status.ruleCount) })}
          </p>
        )
      }
    >
      <ul className={WIDGET_LIST}>
        {shown.map((hit) => (
          <li key={hit.id} className="text-sm">
            <span className="shrink-0 tabular-nums text-text-muted">{time(hit.at)}</span>
            <span className="min-w-0 flex-1 truncate text-text-secondary">{hit.ruleName}</span>
            {hit.platinum != null && <Readout currency="platinum" value={hit.platinum} locale={locale} />}
          </li>
        ))}
      </ul>
    </WidgetFrame>
  );
}

const monthStart = () => {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-01`;
};

export function TradeSummaryWidget({ tr, locale }: Props) {
  const [flow, setFlow] = useState<PlatFlow | null>(null);
  const [failed, setFailed] = useState(false);
  // One paged read per mount, no interval: the ledger only grows when a trade is recorded.
  useEffect(() => {
    let live = true;
    pageLedgerRange({ from: monthStart() }, MAX_MONTH_EVENTS, () => live).then(
      (page) => live && page && setFlow(computeFlow(page.events)),
      () => live && setFailed(true),
    );
    return () => {
      live = false;
    };
  }, []);
  return (
    <WidgetFrame
      widgetId="widget.tradeSummary"
      tr={tr}
      loading={flow === null && !failed}
      errorKey={failed ? "dashboard.widgetError" : null}
      empty={flow === null || flow.events === 0}
      emptyKey="common.noTradesYet"
      subtitle={<p className={WIDGET_SUBTITLE} data-widget-status>{tr("dashboard.thisMonth")}</p>}
    >
      {flow && (
        <div className="flex flex-col gap-2 px-4 py-3">
          <dl className="m-0 grid grid-cols-3 gap-2">
            {(
              [
                ["analysis.platIn", flow.platIn, false],
                ["analysis.platOut", flow.platOut, false],
                ["analysis.netPlat", flow.net, true],
              ] as const
            ).map(([key, value, sign]) => (
              <div key={key} className="min-w-0">
                <dt className="truncate text-xs text-text-muted">{tr(key)}</dt>
                <dd className="m-0">
                  <Readout currency="platinum" sign={sign} value={value} locale={locale} />
                </dd>
              </div>
            ))}
          </dl>
          <p className="m-0 text-xs text-text-muted">
            {tr("analysis.salesCount", { count: String(flow.sales) })}, {tr("analysis.purchasesCount", { count: String(flow.purchases) })}
          </p>
        </div>
      )}
    </WidgetFrame>
  );
}
