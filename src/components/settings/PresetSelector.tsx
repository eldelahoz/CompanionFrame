import { askConfirm } from "../../stores/confirm";
import { useState } from "react";
import type { Translator } from "../../lib/i18n";
import { PRESET_KEYS, THEME_PRESETS } from "../../config/themePresets";
import { themeSettings, useThemeSettings } from "../../lib/theme/themeStore";
import ThemeDropdown, { ThemeOption } from "./ThemeDropdown";

/** BuiltInThemeDropdown.svelte + PresetSelector.svelte: built-in and custom themes, save and delete. */
export default function PresetSelector({ tr }: { tr: Translator }) {
  const { activePreset, customThemes } = useThemeSettings();
  const [builtInOpen, setBuiltInOpen] = useState(false);
  const [customOpen, setCustomOpen] = useState(false);
  const [customName, setCustomName] = useState("");

  const activeCustom = customThemes.find((t) => t.id === activePreset);
  const customLabel = activeCustom?.label ?? (customThemes.length > 0 ? tr("appearance.selectCustomTheme") : tr("appearance.noCustomThemes"));
  const select = (key: string) => {
    themeSettings.applyPreset(key);
    setBuiltInOpen(false);
    setCustomOpen(false);
  };

  return (
    <div className="mb-3">
      <h4 className="m-0 mb-[0.35rem] font-display text-[0.85rem] font-semibold tracking-[0.03em] text-text-primary">{tr("appearance.presets")}</h4>
      <div className="grid gap-2">
        <ThemeDropdown
          label={tr("appearance.builtinThemes")}
          valueLabel={THEME_PRESETS[activePreset]?.label ?? activeCustom?.label ?? tr("common.custom")}
          open={builtInOpen}
          onOpenChange={setBuiltInOpen}
        >
          {PRESET_KEYS.map((key) => (
            <ThemeOption key={key} active={activePreset === key} colors={THEME_PRESETS[key].colors} label={THEME_PRESETS[key].label} onSelect={() => select(key)} />
          ))}
        </ThemeDropdown>
        <ThemeDropdown label={tr("appearance.customThemes")} valueLabel={customLabel} open={customOpen} onOpenChange={setCustomOpen} disabled={customThemes.length === 0}>
          {customThemes.map((t) => (
            <ThemeOption key={t.id} active={activePreset === t.id} colors={t.colors} label={t.label} onSelect={() => select(t.id)} />
          ))}
        </ThemeDropdown>
      </div>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <input
          type="text"
          className="min-w-0 flex-1 rounded-[var(--radius-md)] border border-[var(--ui-control-border)] bg-[var(--ui-control-bg)] px-2 py-1.5 text-xs text-text-primary outline-none focus:border-accent-dim focus:shadow-[0_0_0_2px_rgba(212,168,67,0.12)]"
          maxLength={40}
          placeholder={tr("appearance.customThemeName")}
          value={customName}
          onChange={(e) => setCustomName(e.target.value)}
        />
        <button
          className="btn-secondary btn-sm"
          type="button"
          onClick={() => {
            themeSettings.saveCustomTheme(customName);
            setCustomName("");
          }}
        >
          {tr("appearance.saveCustomTheme")}
        </button>
        {activeCustom && (
          <button
            className="btn-danger btn-sm"
            type="button"
            onClick={async () => (await askConfirm(tr("appearance.confirmDeleteTheme", { name: activeCustom.label }))) && themeSettings.deleteCustomTheme(activeCustom.id)}
          >
            {tr("appearance.deleteCustomTheme")}
          </button>
        )}
      </div>
    </div>
  );
}
