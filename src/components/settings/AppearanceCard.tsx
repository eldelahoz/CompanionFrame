import type { ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import type { Settings } from "../../lib/ipc";
import { themeSettings, useThemeSettings } from "../../lib/theme/themeStore";
import ThemedControlCard from "../ThemedControlCard";
import AppScaleSection from "./AppScaleSection";
import ColorSection from "./ColorSection";
import FontSizeSection from "./FontSizeSection";
import PresetSelector from "./PresetSelector";
import StyleSection from "./StyleSection";

const Card = ({ className = "", children }: { className?: string; children: ReactNode }) => (
  <article className={`w-full rounded-[var(--radius-xl)] border border-[var(--ui-panel-border)] bg-[var(--ui-panel-bg)] p-4 shadow-[var(--ui-panel-shadow)] [backdrop-filter:var(--ui-backdrop-blur)] ${className}`}>{children}</article>
);

interface Props {
  section: "theme" | "colors";
  tr: Translator;
  settings: Settings;
  onChange: (patch: Partial<Settings>) => Promise<string | null> | void;
}

/** Port of AppearanceCard.svelte. The per-view overrides card belongs to the settings-structure task. */
export default function AppearanceCard({ section, tr, settings, onChange }: Props) {
  const { contrastSafeMode } = useThemeSettings();
  if (section === "theme") {
    return (
      <>
        {/* z-20 clears the stacking contexts of the sibling cards under the dropdown menus. */}
        <Card className="relative z-20">
          <PresetSelector tr={tr} />
          <div className="flex flex-wrap gap-1.5">
            <button className="btn-danger btn-sm" onClick={() => themeSettings.resetAll()}>
              {tr("appearance.restoreAll")}
            </button>
          </div>
        </Card>
        <Card>
          <StyleSection tr={tr} />
        </Card>
        <Card>
          <AppScaleSection tr={tr} scale={settings.ui_scale} onCommit={(ui_scale) => void onChange({ ui_scale })} />
          <FontSizeSection tr={tr} />
        </Card>
      </>
    );
  }
  return (
    <Card>
      <ColorSection tr={tr} />
      <div>
        <ThemedControlCard as="label">
          <span className="text-xs font-medium text-text-secondary">
            {tr("appearance.contrastSafeMode")}
            <span className="mt-0.5 block text-xs font-normal text-text-muted">{tr("appearance.contrastSafeModeHint")}</span>
          </span>
          <input type="checkbox" checked={contrastSafeMode} onChange={(e) => themeSettings.setContrastSafeMode(e.target.checked)} />
        </ThemedControlCard>
      </div>
    </Card>
  );
}
