import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { appInfo, getOnboarding, getRawInventory, getSettings, setZoom, onInventoryUpdated, setGameLocale, setSettings, type AppInfo, type Settings } from "./lib/ipc";
import { setUiLocale } from "./lib/format";
import { translator } from "./lib/i18n";
import { bindThemePersistence, getThemeSettings, hydrateFromSettings, legacyThemeOf } from "./lib/theme/themeStore";
import { VIEWS, type ViewName } from "./lib/viewRegistry";
import { useItemDatabase } from "./stores/items";
import { PopoutSoloSection, isPopoutWindow, popoutSectionId, popoutView } from "./stores/popout";
import RelicsView from "./views/RelicsView";
import SettingsView from "./views/SettingsView";
import MasteryView from "./views/MasteryView";
import InventoryView from "./views/InventoryView";
import DashboardView from "./views/DashboardView";
import SyndicatesView from "./views/SyndicatesView";
import WikiView from "./views/WikiView";
import { requestedView } from "./stores/nav";
import WorldView from "./views/WorldView";
import SetupView from "./views/SetupView";
import Tour from "./components/Tour";
import MarketView from "./views/MarketView";
import ArbiAnalyzeView from "./views/ArbiAnalyzeView";
import RivensView from "./views/RivensView";
import MissionsView from "./views/MissionsView";
import HelperStatusPill from "./components/shell/HelperStatusPill";
import { isMemoryMode, memoryMode } from "./stores/dataSource";
import MarketAnalysisView from "./views/MarketAnalysisView";
import StatsView from "./views/StatsView";
import FoundryView from "./views/FoundryView";
import Titlebar from "./components/shell/Titlebar";
import StatusBar from "./components/shell/StatusBar";
import Sidebar from "./components/shell/Sidebar";
import { TooltipHost } from "./components/ui";
import { useThemeSettings } from "./lib/theme/themeStore";
import ToastHost from "./components/shell/ToastHost";
import UpdateBanner from "./components/shell/UpdateBanner";
import { startAutoCheck } from "./stores/updater";
import ConfirmHost from "./components/shell/ConfirmHost";
import ErrorBoundary from "./components/shell/ErrorBoundary";
import WfmStatusPill from "./components/market/WfmStatusPill";
import { useWfmSession } from "./stores/wfmSession";
import { usePresence } from "./lib/wfm/presence";
import CustomCssHost from "./components/CustomCssHost";
import ThemeInspector from "./components/ThemeInspector";
import NotificationHistory from "./components/NotificationHistory";
import ModalHost from "./modals/ModalHost";
import { markNotificationsSeen, useNotificationFeed, useNotificationsUnread } from "./stores/notifications";

const DEFAULTS: Settings = { language: "en", game_language: "auto", theme: "dark", reward_scan_hotkey: "F8", overlay_move_hotkey: "F7", riven_rescan_hotkey: "F6", eelog_path: null, overlay_layouts: {}, overlay_enabled: {}, overlay_opacity: {}, message_notifications_enabled: true, message_notifications_while_focused: false, riven_similar_auctions_shown: true, theme_settings: "", ui_scale: 1, keep_running_on_close: false, data_source_mode: "passive", memory_consent_at: null, auto_inventory_sync: true, save_scan_captures: false, auto_update_check: true, auto_close_trade_listings: false, trade_thanks_enabled: false, trade_thanks_message: "Thank you for your purchase. I appreciate the +rep <3" };

export default function App() {
  const [settings, setLocal] = useState<Settings>(DEFAULTS);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [view, setView] = useState<ViewName | "setup">(() => VIEWS.find(([name]) => name === popoutView)?.[0] ?? "inventory");
  // First run opens the setup wizard; the backend remembers whether it was finished.
  useEffect(() => {
    if (!popoutView) void getOnboarding().then((o) => !o.setupDone && setView("setup"), () => {});
  }, []);
  const items = useItemDatabase();
  const requested = requestedView.use();
  useEffect(() => {
    if (!requested) return;
    setView(requested);
    requestedView.set(null);
  }, [requested]);
  const [inventory, setInventory] = useState<Record<string, unknown> | null>(null);
  const wfm = useWfmSession();
  const presence = usePresence(wfm.session?.logged_in === true);
  useNotificationFeed();
  const unread = useNotificationsUnread();
  const [showNotifications, setShowNotifications] = useState(false);
  const tr = useMemo(() => translator(settings.language), [settings.language]);

  useEffect(() => {
    void getSettings().then((s) => {
      hydrateFromSettings(s);
      setLocal(s);
    });
    void appInfo().then(setInfo);
    void getRawInventory().then(setInventory);
    const off = onInventoryUpdated((d) => setInventory(d as Record<string, unknown> | null));
    return () => void off.then((f) => f());
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
    document.documentElement.lang = settings.language;
  }, [settings.theme, settings.language]);

  // Warframe's language is its own setting; "auto" follows EE.log, the display language plays no part.
  useEffect(() => void setGameLocale(settings.game_language), [settings.game_language]);

  useEffect(() => void setZoom(settings.ui_scale).catch(() => {}), [settings.ui_scale]);

  const memory = isMemoryMode(settings);
  useEffect(() => memoryMode.set(memory), [memory]);

  /** Resolves to the backend's refusal (e.g. a duplicate hotkey), after rolling the form back. */
  const update = async (patch: Partial<Settings>): Promise<string | null> => {
    const prev = settings;
    setLocal({ ...settings, ...patch });
    try {
      await setSettings({ ...settings, ...patch });
      return null;
    } catch (e) {
      setLocal(prev);
      return e instanceof Error ? e.message : String(e);
    }
  };

  // The theme store saves on its own debounce, so it needs the latest `update`, not the one it was bound with.
  const updateRef = useRef(update);
  updateRef.current = update;
  useEffect(() => (settings.auto_update_check && !popoutView ? startAutoCheck() : undefined), [settings.auto_update_check]);
  useEffect(() => bindThemePersistence((json) => void updateRef.current({ theme_settings: json, theme: legacyThemeOf(getThemeSettings()) })), []);

  setUiLocale(settings.language);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const { branding } = useThemeSettings();
  // One shared scroll container: each view keeps its own offset, and a view never inherits another's.
  const content = useRef<HTMLElement>(null);
  const offsets = useRef(new Map<string, number>());
  const scrolledView = useRef(view);
  useLayoutEffect(() => {
    scrolledView.current = view;
    if (content.current) content.current.scrollTop = offsets.current.get(view) ?? 0; // never animated, so reduced motion is respected too
  }, [view]);
  const selectView = (next: ViewName) => {
    setView(next);
    setDrawerOpen(false);
  };

  const current = VIEWS.find(([name]) => name === view);
  const title = current ? tr(current[1]) : "";

  return (
    <div id="shell" className="contents">
      <CustomCssHost />
      <ThemeInspector tr={tr} />
      <Titlebar
        tr={tr}
        appName={branding.appName || info?.name || "CompanionFrame"}
        logo={branding.logoDataUrl}
        onSelectView={isPopoutWindow ? undefined : selectView}
        onOpenMenu={isPopoutWindow ? undefined : () => setDrawerOpen(true)}
        menuOpen={drawerOpen}
      >
        {memory && <HelperStatusPill tr={tr} locale={settings.language} />}
        <WfmStatusPill loggedIn={wfm.session?.logged_in === true} state={presence.state} tr={tr} />
      </Titlebar>
      <div id="app">
        {!isPopoutWindow && <Sidebar tr={tr} view={view} onSelect={selectView} drawerOpen={drawerOpen} onCloseDrawer={() => setDrawerOpen(false)} />}
        <PopoutSoloSection.Provider value={popoutSectionId}>
        <main id="content" ref={content} data-view={view} onScroll={(e) => offsets.current.set(scrolledView.current, e.currentTarget.scrollTop)}>
          <section className="view active">
            {view !== "relics" && view !== "rivens" && view !== "inventory" && view !== "setup" && view !== "arbi" && view !== "analytics" && view !== "stats" && view !== "dashboard" && view !== "market" && view !== "settings" && view !== "mastery" && view !== "foundry" && view !== "missions" && view !== "world" && view !== "syndicates" && view !== "wiki" && (
              <div className="view-header">
                <h2>{title}</h2>
              </div>
            )}
            <ErrorBoundary tr={tr} resetKey={view}>
            {view === "setup" ? (
              <SetupView
                tr={tr}
                settings={settings}
                onChange={update}
                market={{ session: wfm.session, signIn: wfm.signIn, signInWebview: wfm.signInWebview }}
                onComplete={setView}
              />
            ) : view === "settings" ? (
              <SettingsView settings={settings} onChange={update} tr={tr} market={{ loggedIn: wfm.session?.logged_in === true, presence }} />
            ) : view === "relics" ? (
              <RelicsView tr={tr} locale={settings.language} items={items} inventory={inventory} />
            ) : view === "inventory" ? (
              <InventoryView items={items} tr={tr} locale={settings.language} wfmLoggedIn={wfm.session?.logged_in === true} />
            ) : view === "dashboard" ? (
              <DashboardView items={items} tr={tr} locale={settings.language} />
            ) : view === "world" ? (
              <WorldView tr={tr} locale={settings.language} />
            ) : view === "mastery" ? (
              <MasteryView tr={tr} locale={settings.language} />
            ) : view === "market" ? (
              <MarketView tr={tr} items={items} locale={settings.language} session={wfm.session} signIn={wfm.signIn} signInWebview={wfm.signInWebview} signOut={wfm.signOut} presence={presence} />
            ) : view === "arbi" ? (
              <ArbiAnalyzeView tr={tr} language={settings.language} />
            ) : view === "rivens" ? (
              <RivensView tr={tr} locale={settings.language} items={items} similarAuctions={settings.riven_similar_auctions_shown !== false} onSimilarAuctions={(shown) => void update({ riven_similar_auctions_shown: shown })} />
            ) : view === "analytics" ? (
              <MarketAnalysisView tr={tr} locale={settings.language} items={items} />
            ) : view === "stats" ? (
              <StatsView tr={tr} locale={settings.language} />
            ) : view === "missions" && memory ? (
              <MissionsView tr={tr} locale={settings.language} items={items} />
            ) : view === "foundry" ? (
              <FoundryView items={items} tr={tr} />
            ) : view === "syndicates" ? (
              <SyndicatesView tr={tr} locale={settings.language} />
            ) : view === "wiki" ? (
              <WikiView tr={tr} />
            ) : (
              <div className="empty-state text-text-muted">{title}</div>
            )}
            </ErrorBoundary>
          </section>
        </main>
        </PopoutSoloSection.Provider>
      </div>
      {!isPopoutWindow && (
      <StatusBar
        tr={tr}
        version={info?.version}
        unread={unread}
        onOpenNotifications={() => {
          markNotificationsSeen();
          setShowNotifications(true);
        }}
      />
      )}
      <ToastHost tr={tr} />
      {!popoutView && <UpdateBanner tr={tr} />}
      <ConfirmHost tr={tr} />
      <TooltipHost />
      <Tour tr={tr} enabled={!popoutView} />
      <ModalHost tr={tr} locale={settings.language} db={items} inventory={inventory} />
      {showNotifications && <NotificationHistory tr={tr} locale={settings.language} onClose={() => setShowNotifications(false)} />}
    </div>
  );
}
