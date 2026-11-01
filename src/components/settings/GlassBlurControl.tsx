import type { ReactNode } from "react";
import { GLASS_BLUR_MAX_PX, GLASS_BLUR_MIN_PX } from "../../config/themeDefaults";
import type { Translator } from "../../lib/i18n";
import { themeSettings, useThemeSettings } from "../../lib/theme/themeStore";

export default function GlassBlurControl({ tr, children }: { tr: Translator; children: ReactNode }) {
  const { effects } = useThemeSettings();
  return (
    <>
      <label className="flex min-w-0 cursor-pointer flex-wrap items-center justify-between gap-2.5">
        {children}
        <input type="checkbox" checked={effects.glass} onChange={(e) => themeSettings.setEffects({ glass: e.target.checked })} />
      </label>
      {effects.glass && (
        <div className="mt-2 flex items-center gap-2">
          <input
            type="range"
            min={GLASS_BLUR_MIN_PX}
            max={GLASS_BLUR_MAX_PX}
            step={1}
            className="w-full accent-accent"
            aria-label={tr("appearance.glassBlurStrength")}
            value={effects.glassBlurPx}
            onChange={(e) => themeSettings.setEffects({ glassBlurPx: Number(e.target.value) })}
          />
          <span className="w-9 shrink-0 text-right text-xs tabular-nums text-text-primary">{effects.glassBlurPx}px</span>
        </div>
      )}
    </>
  );
}
