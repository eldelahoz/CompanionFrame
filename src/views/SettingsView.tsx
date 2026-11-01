import { Bell, Info, Stethoscope, LifeBuoy, LayoutGrid, MonitorCog, Package, Palette, SearchX, Store } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import CustomCssSection from "../components/settings/CustomCssSection";
import { LOCALE_OPTIONS, type MessageKey, type Translator } from "../lib/i18n";
import WorldNotifySettings from "../components/settings/WorldNotifySettings";
import OverlaySettings from "../components/settings/OverlaySettings";
import DiagnosticsCard from "../components/settings/DiagnosticsCard";
import AboutCard from "../components/settings/AboutCard";
import UpdatesCard from "../components/settings/UpdatesCard";
import HelpCard from "../components/settings/HelpCard";
import NotificationChannelsSettings from "../components/settings/NotificationChannelsSettings";
import AppearanceCard from "../components/settings/AppearanceCard";
import DataSourceMode from "../components/settings/DataSourceMode";
import GameLanguageRow from "../components/settings/GameLanguageRow";
import WfmPresenceBar from "../components/market/WfmPresenceBar";
import { Button, Card, CardHeader, EmptyState, PageHeader, SearchInput, Select, SettingRow, Switch } from "../components/ui";
import type { Language, PresenceOptions, Settings, WfmPresenceState, WfmStatus } from "../lib/ipc";
import { openView, requestedSettingsCard } from "../stores/nav";

interface Props {
  settings: Settings;
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
  tr: Translator;
  /** warframe.market session and presence, owned by App; absent in isolated tests. */
  market?: {
    loggedIn: boolean;
    presence: { state: WfmPresenceState; options: PresenceOptions; setStatus: (s: WfmStatus) => void; saveOptions: (p: Partial<PresenceOptions>) => void };
  };
}

interface Section {
  id: string;
  title: MessageKey;
  icon: ReactNode;
  body: ReactNode;
}

export default function SettingsView({ settings, onChange, tr, market }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState("appearance");
  const [hidden, setHidden] = useState<ReadonlySet<string>>(new Set());
  const grid = useRef<HTMLDivElement>(null);

  const sections: Section[] = [
    {
      id: "appearance",
      title: "settings.group.appearance",
      icon: <Palette />,
      body: (
        <>
          <SettingRow
            label={tr("settings.languageRow")}
            htmlFor="settings-language"
            control={
              <Select id="settings-language" value={settings.language} options={LOCALE_OPTIONS.map((o) => [o.code, o.label] as const)} onChange={(e) => onChange({ language: e.target.value as Language })} />
            }
          />
          <GameLanguageRow id="settings-game-language" tr={tr} settings={settings} onChange={onChange} />
          <AppearanceCard section="theme" tr={tr} settings={settings} onChange={onChange} />
          <AppearanceCard section="colors" tr={tr} settings={settings} onChange={onChange} />
          <CustomCssSection tr={tr} />
        </>
      ),
    },
    { id: "overlays", title: "settings.group.overlays", icon: <MonitorCog />, body: <OverlaySettings settings={settings} onChange={onChange} tr={tr} /> },
    {
      id: "inventory",
      title: "settings.group.inventory",
      icon: <Package />,
      body: (
        <>
          <h4 className="m-0 text-sm font-semibold text-text-primary">{tr("dataSource.title")}</h4>
          <DataSourceMode tr={tr} settings={settings} onChange={onChange} locale={settings.language} />
          <p className="m-0 text-xs text-text-muted">{tr("settings.inventoryInfo")}</p>
        </>
      ),
    },
    {
      id: "market",
      title: "settings.group.market",
      icon: <Store />,
      body: market?.loggedIn ? (
        <WfmPresenceBar {...market.presence} tr={tr} />
      ) : (
        <SettingRow
          label={tr("market.pill.signedOut")}
          description={tr("settings.marketSignInHint")}
          control={
            <Button size="sm" onClick={() => openView("market")}>
              {tr("settings.openMarket")}
            </Button>
          }
        />
      ),
    },
    {
      id: "notifications",
      title: "settings.group.notifications",
      icon: <Bell />,
      body: (
        <>
          <NotificationChannelsSettings tr={tr} />
          <WorldNotifySettings tr={tr} />
        </>
      ),
    },
    {
      id: "behavior",
      title: "settings.behaviorTitle",
      icon: <LayoutGrid />,
      body: (
        <div data-setting="behavior">
          <SettingRow
            label={tr("settings.keepRunningOnClose")}
            description={tr("settings.behaviorInfo")}
            control={<Switch checked={settings.keep_running_on_close} aria-label={tr("settings.keepRunningOnClose")} onChange={(e) => void onChange({ keep_running_on_close: e.target.checked })} />}
          />
        </div>
      ),
    },
    { id: "help", title: "help.title", icon: <LifeBuoy />, body: <HelpCard tr={tr} /> },
    { id: "about", title: "settings.aboutTitle", icon: <Info />, body: (
        <>
          <AboutCard tr={tr} locale={settings.language} />
          <UpdatesCard tr={tr} settings={settings} onChange={onChange} />
        </>
      ),
    },
    { id: "diagnostics", title: "settings.group.diagnostics", icon: <Stethoscope />, body: <DiagnosticsCard tr={tr} /> },
  ];

  // Search filters whole cards by their visible text (labels, descriptions, options), so it covers the nested editors too.
  useEffect(() => {
    const q = query.trim().toLowerCase();
    const cards = grid.current?.querySelectorAll<HTMLElement>("[data-settings-card]") ?? [];
    const next = new Set<string>();
    cards.forEach((card) => {
      if (q && !(card.textContent ?? "").toLowerCase().includes(q)) next.add(card.dataset.settingsCard ?? "");
    });
    setHidden(next);
  }, [query, settings.language]);

  // The index follows the card nearest the top of the scroll area.
  useEffect(() => {
    const root = grid.current?.closest("#content");
    if (!root || typeof IntersectionObserver === "undefined") return;
    const io = new IntersectionObserver(
      (entries) => {
        const top = entries.filter((e) => e.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
        if (top) setActive((top.target as HTMLElement).dataset.settingsCard ?? "appearance");
      },
      { root, rootMargin: "0px 0px -60% 0px" },
    );
    grid.current?.querySelectorAll("[data-settings-card]").forEach((c) => io.observe(c));
    return () => io.disconnect();
  }, []);

  const jump = (id: string) => {
    setActive(id);
    grid.current?.querySelector(`[data-settings-card="${id}"]`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  const visible = sections.filter((s) => !hidden.has(s.id));

  // Another view asked for a card ("Open Settings › Inventory"): scroll to it once it is laid out.
  const wanted = requestedSettingsCard.use();
  useEffect(() => {
    if (!wanted) return;
    requestedSettingsCard.set(null);
    requestAnimationFrame(() => jump(wanted));
  }, [wanted]);

  return (
    <div className="settings-view">
      <PageHeader
        title={tr("common.settings")}
        actions={<SearchInput label={tr("settings.search")} placeholder={tr("settings.search")} value={query} onChange={(e) => setQuery(e.target.value)} className="settings-search" />}
      />
      <div className="settings-layout">
        <nav className="settings-index" aria-label={tr("settings.sections")}>
          {visible.map((s) => (
            <button key={s.id} type="button" className="settings-index-item" aria-current={active === s.id ? "true" : undefined} onClick={() => jump(s.id)}>
              {s.icon}
              <span>{tr(s.title)}</span>
            </button>
          ))}
        </nav>
        <div className="settings-main">
        <div ref={grid} className="settings-grid">
          {sections.map((s) => (
            <Card key={s.id} className="settings-card ui-cq" data-settings-card={s.id} hidden={hidden.has(s.id)}>
              <CardHeader
                title={
                  <span className="inline-flex items-center gap-2">
                    {s.icon}
                    {tr(s.title)}
                  </span>
                }
              />
              <div className="settings-card-body">{s.body}</div>
            </Card>
          ))}
          {visible.length === 0 && (
            <Card className="settings-card">
              <EmptyState icon={<SearchX />} title={tr("settings.searchEmpty", { query })} />
            </Card>
          )}
        </div>
        </div>
      </div>
    </div>
  );
}
