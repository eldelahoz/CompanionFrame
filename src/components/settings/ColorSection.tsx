import type { Translator } from "../../lib/i18n";
import { setThemeInspectorActive, themeSettings, useThemeInspectorActive, useThemeSettings } from "../../lib/theme/themeStore";
import ContrastBadge from "./ContrastBadge";
import { COLOR_GROUPS } from "./colorGroups";

/** Native color inputs only accept hex; rgba values show their rgb part, anything else a neutral grey. */
function toHexInput(value: string): string {
  if (value.startsWith("#")) return value.length > 7 ? value.slice(0, 7) : value;
  const match = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/.exec(value);
  if (!match) return "#888888";
  return `#${[match[1], match[2], match[3]].map((n) => parseInt(n, 10).toString(16).padStart(2, "0")).join("")}`;
}

export default function ColorSection({ tr }: { tr: Translator }) {
  const { colors, contrastSafeMode } = useThemeSettings();
  const inspecting = useThemeInspectorActive();
  return (
    <div className="mb-3">
      <div className="mb-[0.35rem] flex items-center justify-between">
        <h4 className="m-0 font-display text-[0.85rem] font-semibold tracking-[0.03em] text-text-primary">{tr("appearance.colors")}</h4>
        <div className="flex gap-1.5">
          <button className="btn-secondary btn-sm" data-theme-pick-element aria-pressed={inspecting} onClick={() => setThemeInspectorActive(!inspecting)}>
            {inspecting ? tr("appearance.inspectorExit") : tr("appearance.inspectorPick")}
          </button>
          <button className="btn-secondary btn-sm" onClick={() => themeSettings.resetColors()}>
            {tr("common.reset")}
          </button>
        </div>
      </div>

      <p className="mb-2 mt-0 text-xs text-text-muted">{tr("appearance.inspectorHint")}</p>

      {COLOR_GROUPS.map((group) => (
        <div key={group.labelKey} className="mb-2.5">
          <span className="mb-1 block font-display text-xs font-semibold uppercase tracking-[0.04em] text-text-muted">{tr(group.labelKey)}</span>
          <div className="flex flex-wrap gap-2">
            {group.keys.map((item) => (
              <label key={item.key} className="flex cursor-pointer flex-col items-center gap-1">
                <input
                  type="color"
                  className="h-8 w-8 cursor-pointer rounded-[var(--radius-md)] border border-[var(--ui-control-border)] bg-transparent p-0 [&::-webkit-color-swatch-wrapper]:p-0.5 [&::-webkit-color-swatch]:rounded-[var(--radius-sm)] [&::-webkit-color-swatch]:border-none"
                  value={toHexInput(colors[item.key])}
                  onChange={(e) => themeSettings.setColor(item.key, e.target.value)}
                />
                <span className="font-display text-xs tracking-[0.02em] text-text-secondary">{tr(item.labelKey)}</span>
                {item.isText && contrastSafeMode && <ContrastBadge fg={colors[item.key]} bg={colors.bgBase} tr={tr} />}
              </label>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
