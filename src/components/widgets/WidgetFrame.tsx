import { useState, type ReactNode } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import { VIEWS } from "../../lib/viewRegistry";
import { WIDGET_SETTING_LABEL_KEYS, WIDGET_SETTING_RANGES, widgetById } from "../../lib/widgets/registry";
import { setWidgetSetting, useWidgetSettings } from "../../stores/dashboard";
import { editMode } from "../../stores/layout";
import { openView } from "../../stores/nav";
import { ArrowUpRight, Settings2 } from "lucide-react";
import { Card, IconButton, INPUT, Skeleton } from "../ui";

interface Props {
  widgetId: string;
  tr: Translator;
  /** Nothing to show: the body is replaced by emptyKey. */
  empty?: boolean;
  emptyKey?: MessageKey;
  /** The widget's own data source failed; wins over the empty state. */
  errorKey?: MessageKey | null;
  /** First payload still on its way: a placeholder stands in so a cold start never reads as "no data". */
  loading?: boolean;
  /** Under the header even while empty, for state the body would hide. */
  subtitle?: ReactNode;
  /** Rows the body left out because of the row limit; 0 hides the footer. */
  overflow?: number;
  children: ReactNode;
}

/** Shared chrome of every dashboard widget: title, link to its home tab, edit-mode settings and the loading/empty/error states. */
export default function WidgetFrame({ widgetId, tr, empty = false, emptyKey, errorKey = null, loading = false, subtitle, overflow = 0, children }: Props) {
  const descriptor = widgetById(widgetId);
  const settings = useWidgetSettings(widgetId);
  const editing = editMode.use() === "dashboard";
  const [showSettings, setShowSettings] = useState(false);
  const homeView = descriptor?.homeView ?? "inventory";
  const homeLabel = tr(VIEWS.find(([name]) => name === homeView)?.[1] ?? "common.unknown");
  const label = tr(descriptor?.labelKey ?? "common.unknown");
  const settingNames = Object.keys(descriptor?.settings ?? {});
  const open = () => openView(homeView);
  const openLabel = tr("dashboard.openTab", { label: homeLabel });

  const placeholder = (text: string, attr: "error" | "empty") => (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-4 py-6 text-center text-sm text-text-secondary" {...{ [`data-widget-${attr}`]: widgetId }}>
      <p className="m-0">{text}</p>
      <button type="button" className="cursor-pointer border-0 bg-transparent p-0 text-sm text-[var(--text-link)] hover:underline" onClick={open}>
        {openLabel}
      </button>
    </div>
  );

  return (
    <Card as="div" className="flex min-h-[10rem] min-w-0 flex-col overflow-hidden">
      <div data-widget={widgetId} className="contents">
        <header className="ui-card-head">
          <h2 className="truncate">{label}</h2>
          <div className="ui-card-tools">
            {editing && settingNames.length > 0 && (
              <IconButton data-widget-gear={widgetId} aria-expanded={showSettings} label={tr("dashboard.widgetSettings", { label })} onClick={() => setShowSettings((v) => !v)}>
                <Settings2 />
              </IconButton>
            )}
            <IconButton data-widget-open={widgetId} label={openLabel} onClick={open}>
              <ArrowUpRight />
            </IconButton>
          </div>
        </header>
        {subtitle && <div className="px-4 pt-3">{subtitle}</div>}
        {editing && showSettings && settingNames.length > 0 && (
          <div className="flex flex-col gap-2 border-b border-border px-4 py-3 text-sm" data-widget-settings={widgetId}>
            {settingNames.map((name) => {
              const kind = descriptor?.settings?.[name];
              const range = WIDGET_SETTING_RANGES[name];
              return (
                <label key={name} className="flex items-center justify-between gap-2">
                  <span className="text-text-secondary">{tr(WIDGET_SETTING_LABEL_KEYS[name] ?? "common.unknown")}</span>
                  {kind === "boolean" ? (
                    <input type="checkbox" data-widget-setting={name} checked={settings[name] === true} onChange={(e) => setWidgetSetting(widgetId, name, e.currentTarget.checked)} />
                  ) : (
                    <input
                      type={kind === "number" ? "number" : "text"}
                      className={`${INPUT} ${kind === "number" ? "w-16" : "w-32"}`}
                      data-widget-setting={name}
                      min={range?.min}
                      max={range?.max}
                      defaultValue={String(settings[name] ?? "")}
                      onChange={(e) => {
                        const v = kind === "number" ? Number(e.currentTarget.value) : e.currentTarget.value;
                        if (kind !== "number" || (e.currentTarget.value !== "" && Number.isFinite(v))) setWidgetSetting(widgetId, name, v);
                      }}
                    />
                  )}
                </label>
              );
            })}
          </div>
        )}
        {errorKey ? (
          placeholder(tr(errorKey), "error")
        ) : loading ? (
          <div className="flex flex-col gap-3 px-4 py-4" data-widget-loading={widgetId}>
            {[70, 55, 62].map((w) => (
              <Skeleton key={w} width={`${w}%`} />
            ))}
          </div>
        ) : empty ? (
          placeholder(tr(emptyKey ?? "world.noData"), "empty")
        ) : (
          <>
            {children}
            {overflow > 0 && (
              <button type="button" className="mx-4 mb-3 cursor-pointer self-end border-0 bg-transparent p-0 text-xs text-text-muted hover:text-[var(--text-link)] hover:underline" data-widget-more title={openLabel} onClick={open}>
                {tr("common.moreCount", { count: String(overflow) })}
              </button>
            )}
          </>
        )}
      </div>
    </Card>
  );
}

/** Muted meta next to the widget title. */
export const WIDGET_SUBTITLE = "m-0 truncate text-xs text-text-muted";
/** Row list filling the card body; rows are divided by hairlines (.ui-list). */
export const WIDGET_LIST = "ui-list max-h-[21rem] flex-1 overflow-y-auto";
/** Body padding for content that is not a row list. */
export const WIDGET_BODY = "flex flex-col gap-2 px-4 py-3";
