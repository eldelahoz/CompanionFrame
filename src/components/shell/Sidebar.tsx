import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useEffect, useSyncExternalStore } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import { usePersisted } from "../../lib/usePersisted";
import { VIEWS, type ViewName } from "../../lib/viewRegistry";
import { NavIcon } from "../icons";
import { memoryMode } from "../../stores/dataSource";

// Sidebar sections; views not listed in a group (settings) live in the footer.
const GROUPS: ReadonlyArray<readonly [MessageKey, readonly ViewName[]]> = [
  ["nav.group.overview", ["dashboard", "inventory", "foundry", "mastery", "stats"]],
  ["nav.group.live", ["world", "syndicates", "missions"]],
  ["nav.group.trade", ["market", "analytics", "rivens"]],
  ["nav.group.data", ["relics", "wiki", "arbi"]],
];
const LABEL = Object.fromEntries(VIEWS.map(([name, key]) => [name, key])) as Record<ViewName, MessageKey>;

/** Live media-query match (false where matchMedia is missing, e.g. jsdom). */
function useMedia(query: string): boolean {
  return useSyncExternalStore(
    (cb) => {
      const m = typeof window !== "undefined" && window.matchMedia ? window.matchMedia(query) : null;
      m?.addEventListener("change", cb);
      return () => m?.removeEventListener("change", cb);
    },
    () => (typeof window !== "undefined" && window.matchMedia ? window.matchMedia(query).matches : false),
  );
}

interface Props {
  tr: Translator;
  view: ViewName | "setup";
  onSelect: (view: ViewName) => void;
  /** Drawer state under the narrow breakpoint. */
  drawerOpen: boolean;
  onCloseDrawer: () => void;
}

export default function Sidebar({ tr, view, onSelect, drawerOpen, onCloseDrawer }: Props) {
  const [mode, setMode] = usePersisted("cf.sidebar", ["expanded", "rail"] as const, "expanded");
  const memory = memoryMode.use();
  const compact = useMedia("(max-width: 1024px)");
  const narrow = useMedia("(max-width: 640px)");
  // In the drawer the labels always show; between the breakpoints the rail is forced.
  const collapsed = narrow ? false : compact || mode === "rail";

  useEffect(() => {
    if (!drawerOpen) return;
    const key = (e: KeyboardEvent) => e.key === "Escape" && onCloseDrawer();
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [drawerOpen, onCloseDrawer]);

  const item = (name: ViewName) => (
    <button
      key={name}
      type="button"
      className="nav-btn"
      data-view={name}
      data-tip={tr(LABEL[name])}
      data-tip-when="collapsed"
      data-tip-side="right"
      aria-current={view === name ? "page" : undefined}
      onClick={() => onSelect(name)}
    >
      <NavIcon view={name} />
      <span className="nav-label-text">{tr(LABEL[name])}</span>
    </button>
  );

  return (
    <>
      <nav id="sidebar" className="sidebar" aria-label={tr("nav.main")} data-collapsed={collapsed} data-drawer-open={narrow && drawerOpen}>
        {GROUPS.map(([label, views]) => (
          <div key={label} className="nav-group">
            <div className="nav-label">{tr(label)}</div>
            {views.filter((v) => memory || v !== "missions").map(item)}
          </div>
        ))}
        <div className="sidebar-foot">
          {item("settings")}
          <button
            type="button"
            className="nav-btn sidebar-collapse"
            aria-label={tr(mode === "rail" ? "nav.expand" : "nav.collapse")}
            data-tip={tr(mode === "rail" ? "nav.expand" : "nav.collapse")}
            data-tip-side="right"
            onClick={() => setMode(mode === "rail" ? "expanded" : "rail")}
          >
            {mode === "rail" ? <PanelLeftOpen aria-hidden="true" /> : <PanelLeftClose aria-hidden="true" />}
          </button>
        </div>
      </nav>
      <div className="sidebar-scrim" onClick={onCloseDrawer} aria-hidden="true" />
    </>
  );
}
