import { Menu, Minus, Moon, Square, Sun, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { freshnessText, freshnessTone, staleHint } from "../../lib/inventoryFreshness";
import { openView } from "../../stores/nav";
import type { Translator } from "../../lib/i18n";
import { getInventoryStatus, onInventoryStatusUpdated, windowClose, windowMinimize, windowToggleMaximize, type InventoryStatus } from "../../lib/ipc";
import { themeSettings, useThemeSettings } from "../../lib/theme/themeStore";
import type { ViewName } from "../../lib/viewRegistry";
import { BrandMark } from "../icons";
import JobsPill from "./JobsPill";
import ViewSearch from "./ViewSearch";

/** The age label moves on its own; the status events only fire on changes. */
const AGE_TICK_MS = 60_000;
const DOT = { success: "bg-success", warning: "bg-warning", danger: "bg-danger", muted: "bg-text-muted" } as const;

interface Props {
  tr: Translator;
  appName: string;
  /** Custom logo from the theme editor's branding, replacing the mark. */
  logo?: string | null;
  /** Jump to a view from the search box; omitted in pop-out windows. */
  onSelectView?: (view: ViewName) => void;
  /** Opens the navigation drawer (only visible under the narrow breakpoint). */
  onOpenMenu?: () => void;
  menuOpen?: boolean;
  /** Right-aligned status pills (warframe.market presence). */
  children?: ReactNode;
}

export default function Titlebar({ tr, appName, logo, onSelectView, onOpenMenu, menuOpen = false, children }: Props) {
  const [inventory, setInventory] = useState<InventoryStatus | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const theme = useThemeSettings();
  const light = theme.activePreset === "light";

  useEffect(() => {
    let live = true;
    const off = onInventoryStatusUpdated((s) => live && setInventory(s));
    void getInventoryStatus().then((s) => live && setInventory((cur) => cur ?? s), () => {});
    const timer = setInterval(() => setNow(Date.now()), AGE_TICK_MS);
    return () => {
      live = false;
      void off.then((f) => f());
      clearInterval(timer);
    };
  }, []);

  const tone = freshnessTone(inventory, now);
  const text = freshnessText(inventory, now, tr);
  const hint = staleHint(inventory);
  const tip = `${text}. ${hint ? `${tr(hint)} ` : ""}${tr("inventory.pill.openSettings")}`;
  const themeLabel = tr(light ? "titlebar.themeDark" : "titlebar.themeLight");
  return (
    <header data-tauri-drag-region className="titlebar app-region-drag">
      {onOpenMenu && (
        <button type="button" className="ui-icon-btn menu-btn app-region-no-drag" aria-label={tr("nav.openMenu")} aria-expanded={menuOpen} aria-controls="sidebar" onClick={onOpenMenu}>
          <Menu />
        </button>
      )}
      <div className="titlebar-brand pointer-events-none">
        {logo ? <img src={logo} alt={tr("titlebar.logoAlt")} /> : <BrandMark />}
        <span>{appName}</span>
      </div>
      {onSelectView && <ViewSearch tr={tr} onSelect={onSelectView} />}
      <div className="titlebar-right app-region-no-drag">
        <JobsPill tr={tr} />
        <button type="button" className="titlebar-pill" data-tip={tip} aria-label={tip} data-inventory-status={tone} onClick={() => openView("settings")}>
          <span className={`status-dot ${DOT[tone]}`} />
          <span className="titlebar-pill-text">{text}</span>
        </button>
        {children}
        <button type="button" className="ui-icon-btn" aria-label={themeLabel} data-tip={themeLabel} onClick={() => themeSettings.applyPreset(light ? "default" : "light")}>
          {light ? <Moon /> : <Sun />}
        </button>
      </div>
      <div className="window-controls app-region-no-drag">
        <button type="button" aria-label={tr("titlebar.minimize")} data-tip={tr("titlebar.minimize")} onClick={() => void windowMinimize()}>
          <Minus />
        </button>
        <button type="button" aria-label={tr("titlebar.maximize")} data-tip={tr("titlebar.maximize")} onClick={() => void windowToggleMaximize()}>
          <Square />
        </button>
        <button type="button" className="is-close" aria-label={tr("common.close")} data-tip={tr("common.close")} onClick={() => void windowClose()}>
          <X />
        </button>
      </div>
    </header>
  );
}
