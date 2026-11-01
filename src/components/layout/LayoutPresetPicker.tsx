import { useState } from "react";
import { LAYOUT_PRESETS } from "../../config/layoutPresets";
import type { Translator } from "../../lib/i18n";
import type { LayoutView } from "../../lib/layout/types";
import { applyPreset } from "../../stores/layout";

interface Props {
  view: LayoutView;
  tr: Translator;
  onClose: () => void;
}

export default function LayoutPresetPicker({ view, tr, onClose }: Props) {
  const [thisViewOnly, setThisViewOnly] = useState(false);
  return (
    <div
      className="absolute right-0 top-full z-30 mt-1 flex w-56 flex-col gap-1 rounded-[var(--radius-lg)] border border-[color:var(--ui-panel-border,var(--border))] bg-surface-tooltip p-2 text-xs shadow-[var(--ui-panel-shadow)]"
      data-layout-presets
    >
      <label className="flex cursor-pointer items-center gap-1.5 text-text-secondary">
        <input type="checkbox" data-layout-preset-scope checked={thisViewOnly} onChange={(e) => setThisViewOnly(e.target.checked)} />
        {tr("layout.presetThisTabOnly")}
      </label>
      {LAYOUT_PRESETS.map((preset) => (
        <button
          key={preset.id}
          type="button"
          className="cursor-pointer rounded border border-border px-2 py-1 text-left text-text-secondary transition-colors hover:border-accent hover:text-accent"
          data-layout-preset={preset.id}
          onClick={() => {
            applyPreset(preset.id, thisViewOnly ? [view] : undefined);
            onClose();
          }}
        >
          {tr(preset.labelKey)}
        </button>
      ))}
    </div>
  );
}
