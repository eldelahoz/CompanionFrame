import { readStorage, writeStorage } from "../lib/persistence";
import { createSignal } from "../lib/signal";
import { DASHBOARD_WIDGETS, WIDGET_SETTING_DEFAULTS, WIDGET_SETTING_RANGES, widgetById } from "../lib/widgets/registry";

type SettingValue = boolean | number | string;
export type WidgetSettings = Record<string, SettingValue>;
type DashboardSettings = Record<string, WidgetSettings>;

// Same key and shape as WFHelper's file so an import keeps the user's row limits.
const DASHBOARD_STORAGE_KEY = "wf_dashboard_v1";
const MAX_SETTING_CHARS = 120;

function readSetting(name: string, kind: "boolean" | "number" | "string", raw: unknown): SettingValue | null {
  if (kind === "boolean") return typeof raw === "boolean" ? raw : null;
  if (kind === "number") {
    if (typeof raw !== "number" || !Number.isFinite(raw)) return null;
    const range = WIDGET_SETTING_RANGES[name];
    const rounded = Math.round(raw);
    return range ? Math.min(Math.max(rounded, range.min), range.max) : rounded;
  }
  return typeof raw === "string" && raw.length <= MAX_SETTING_CHARS ? raw : null;
}

/** Unknown widgets drop, bad values fall back to the registry defaults. */
export function normalizeDashboard(raw: unknown): DashboardSettings {
  const stored = new Map<string, Record<string, unknown>>();
  const record = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : null;
  if (record?.version === 1 && Array.isArray(record.widgets)) {
    for (const w of record.widgets) {
      const entry = w as { id?: unknown; settings?: unknown } | null;
      if (typeof entry?.id === "string" && entry.settings && typeof entry.settings === "object") stored.set(entry.id, entry.settings as Record<string, unknown>);
    }
  }
  const out: DashboardSettings = {};
  for (const widget of DASHBOARD_WIDGETS) {
    const settings: WidgetSettings = {};
    for (const [name, kind] of Object.entries(widget.settings ?? {})) {
      const value = readSetting(name, kind, stored.get(widget.id)?.[name]) ?? WIDGET_SETTING_DEFAULTS[name];
      if (value !== undefined) settings[name] = value;
    }
    out[widget.id] = settings;
  }
  return out;
}

function load(): DashboardSettings {
  try {
    return normalizeDashboard(JSON.parse(readStorage(DASHBOARD_STORAGE_KEY) ?? "null"));
  } catch {
    return normalizeDashboard(null);
  }
}

const state = createSignal<DashboardSettings>(load());

/** Settings for one widget, merged over the registry defaults. */
export const useWidgetSettings = (widgetId: string): WidgetSettings => state.use()[widgetId] ?? {};

export const settingNumber = (s: WidgetSettings, name: string, fallback: number): number => (typeof s[name] === "number" ? (s[name] as number) : fallback);
export const settingBoolean = (s: WidgetSettings, name: string, fallback: boolean): boolean => (typeof s[name] === "boolean" ? (s[name] as boolean) : fallback);

/** Re-validated against the widget's schema, so a bad editor value never reaches disk. */
export function setWidgetSetting(widgetId: string, name: string, value: SettingValue): void {
  const kind = widgetById(widgetId)?.settings?.[name];
  const checked = kind ? readSetting(name, kind, value) : null;
  if (checked === null) return;
  const next = { ...state.get(), [widgetId]: { ...state.get()[widgetId], [name]: checked } };
  writeStorage(DASHBOARD_STORAGE_KEY, JSON.stringify({ version: 1, widgets: Object.entries(next).map(([id, settings]) => ({ id, settings })) }));
  state.set(next);
}
