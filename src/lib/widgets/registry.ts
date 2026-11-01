import type { MessageKey } from "../i18n";
import { SPAN_ORDER } from "../layout/plan";
import type { LayoutView, SectionDescriptor, SectionSpan } from "../layout/types";

type SettingKind = "boolean" | "number" | "string";

export interface WidgetDescriptor {
  id: string;
  labelKey: MessageKey;
  homeView: LayoutView;
  defaultSpan: SectionSpan;
  allowedSpans: readonly SectionSpan[];
  settings?: Readonly<Record<string, SettingKind>>;
  canPopout?: boolean;
  /** Joined after release: a saved dashboard layout gets it hidden. */
  hiddenInSavedLayouts?: boolean;
}

const TIERS = { lith: "boolean", meso: "boolean", neo: "boolean", axi: "boolean", requiem: "boolean", omnia: "boolean" } as const;

// allowedSpans must stay a contiguous tail of SPAN_ORDER: the layout grid cycles upward from minSpan.
export const DASHBOARD_WIDGETS: readonly WidgetDescriptor[] = [
  { id: "widget.cycles", labelKey: "world.planetCycles", homeView: "world", defaultSpan: 1, allowedSpans: SPAN_ORDER, canPopout: true },
  { id: "widget.fissures", labelKey: "world.voidFissures", homeView: "world", defaultSpan: 1, allowedSpans: SPAN_ORDER, settings: { limit: "number", ...TIERS }, canPopout: true },
  { id: "widget.dailies", labelKey: "dailies.tab", homeView: "world", defaultSpan: "full", allowedSpans: ["full"], canPopout: true },
  { id: "widget.foundryReady", labelKey: "dashboard.foundryReady", homeView: "foundry", defaultSpan: 1, allowedSpans: SPAN_ORDER, settings: { limit: "number" } },
  { id: "widget.marketAlerts", labelKey: "marketAlerts.title", homeView: "market", defaultSpan: 1, allowedSpans: SPAN_ORDER, settings: { limit: "number" }, canPopout: true },
  { id: "widget.goals", labelKey: "mastery.planner.pinnedTitle", homeView: "mastery", defaultSpan: 1, allowedSpans: SPAN_ORDER, settings: { limit: "number" } },
  { id: "widget.baro", labelKey: "world.baroKiteer", homeView: "world", defaultSpan: 1, allowedSpans: SPAN_ORDER, canPopout: true },
  { id: "widget.inventoryValue", labelKey: "inventory.value.title", homeView: "inventory", defaultSpan: 1, allowedSpans: SPAN_ORDER, settings: { allTradables: "boolean" } },
  { id: "widget.tradeSummary", labelKey: "layout.section.analyticsSummary", homeView: "analytics", defaultSpan: 1, allowedSpans: SPAN_ORDER },
  { id: "widget.lastMission", labelKey: "dashboard.lastMission", homeView: "missions", defaultSpan: 1, allowedSpans: SPAN_ORDER, hiddenInSavedLayouts: true },
  { id: "widget.recentRuns", labelKey: "arbi.title", homeView: "arbi", defaultSpan: "full", allowedSpans: [2, "full"], settings: { limit: "number" }, canPopout: true },
];

export const WIDGET_SETTING_LABEL_KEYS: Readonly<Record<string, MessageKey>> = {
  limit: "dashboard.rowLimit",
  allTradables: "inventory.value.allTradables",
  lith: "relics.tier.lith",
  meso: "relics.tier.meso",
  neo: "relics.tier.neo",
  axi: "relics.tier.axi",
  requiem: "relics.tier.requiem",
  omnia: "relics.tier.omnia",
};

export const WIDGET_SETTING_DEFAULTS: Readonly<Record<string, boolean | number | string>> = {
  limit: 5,
  allTradables: false,
  lith: true,
  meso: true,
  neo: true,
  axi: true,
  requiem: true,
  omnia: true,
};

export const WIDGET_SETTING_RANGES: Readonly<Record<string, { min: number; max: number }>> = { limit: { min: 1, max: 20 } };

export const widgetById = (id: string): WidgetDescriptor | null => DASHBOARD_WIDGETS.find((w) => w.id === id) ?? null;

/** `widget.cycles` -> `dashboard.cycles` */
export const sectionIdFor = (widgetId: string): string => `dashboard.${widgetId.slice("widget.".length)}`;

export const dashboardSectionDescriptors = (only?: ReadonlySet<string>): SectionDescriptor[] =>
  DASHBOARD_WIDGETS.filter((w) => !only || only.has(w.id)).map((w) => ({
    id: sectionIdFor(w.id),
    view: "dashboard",
    labelKey: w.labelKey,
    defaultSpan: w.defaultSpan,
    minSpan: w.allowedSpans[0] ?? 1,
    canCollapse: true,
    ...(w.canPopout ? { canPopout: true } : {}),
    ...(w.hiddenInSavedLayouts ? { hiddenInSavedLayouts: true } : {}),
  }));

/** Keys for rows that may legitimately repeat: a repeat gets a numbered suffix. */
export function withRowKeys<T extends object>(rows: readonly T[], keyOf: (row: T) => string): (T & { rowKey: string })[] {
  const used = new Set<string>();
  return rows.map((row) => {
    const base = keyOf(row);
    let rowKey = base;
    for (let copy = 2; used.has(rowKey); copy += 1) rowKey = `${base}#${String(copy)}`;
    used.add(rowKey);
    return { ...row, rowKey };
  });
}
