import { useState, type ChangeEvent, type FocusEvent, type KeyboardEvent } from "react";
import { FONT_SCALE_MAX, FONT_SCALE_MIN, FONT_SCALE_STEP } from "../../config/themeDefaults";
import type { Translator } from "../../lib/i18n";
import { themeSettings, useThemeSettings } from "../../lib/theme/themeStore";
import ThemedControlCard from "../ThemedControlCard";

const clampScale = (v: number) => Math.max(FONT_SCALE_MIN, Math.min(FONT_SCALE_MAX, v));
const NUMBER_INPUT =
  "border border-[var(--ui-control-border)] rounded-[var(--radius-md)] bg-bg-base text-text-primary py-1 px-2 outline-none text-right focus:border-accent-dim focus:shadow-[0_0_0_2px_rgba(212,168,67,0.12)]";

function parseOptionalRem(raw: string): number | undefined {
  if (!raw.trim()) return undefined;
  const n = Number(raw);
  return Number.isFinite(n) && n > 0 ? Math.max(0.3, Math.min(5, n)) : undefined;
}

const SIZES = [
  { key: "headingSize", label: "appearance.headingSize", min: 0.5, max: 5 },
  { key: "bodySize", label: "appearance.bodySize", min: 0.5, max: 5 },
  { key: "smallSize", label: "appearance.smallSize", min: 0.3, max: 3 },
] as const;

export default function FontSizeSection({ tr }: { tr: Translator }) {
  const { fontSizes } = useThemeSettings();
  const [draft, setDraft] = useState<number | null>(null);
  const scaleValue = draft ?? fontSizes.globalScale;
  const scalePercent = Math.round(scaleValue * 100);

  const commitPercent = (e: FocusEvent<HTMLInputElement> | KeyboardEvent<HTMLInputElement>) => {
    const input = e.currentTarget;
    const parsed = Number(input.value);
    if (!input.value.trim() || !Number.isFinite(parsed)) {
      input.value = String(scalePercent);
      return;
    }
    const value = clampScale(parsed / 100);
    setDraft(null);
    input.value = String(Math.round(value * 100));
    themeSettings.setGlobalScale(value);
  };
  const commitDraft = () => {
    if (draft === null) return;
    themeSettings.setGlobalScale(draft);
    setDraft(null);
  };

  return (
    <div className="mb-3" data-font-sizes>
      <div className="mb-[0.35rem] flex items-center justify-between">
        <h4 className="m-0 font-display text-[0.85rem] font-semibold tracking-[0.03em] text-text-primary">{tr("appearance.fontSizes")}</h4>
        <button className="btn-secondary btn-sm" onClick={() => themeSettings.resetFontSizes()}>
          {tr("common.reset")}
        </button>
      </div>
      <div className="grid gap-2">
        <ThemedControlCard as="label" density="tight">
          <span className="text-xs font-medium text-text-secondary">{tr("appearance.globalScale")}</span>
          <div className="flex items-center gap-1.5">
            <input
              type="range"
              className="w-32 min-w-0 accent-accent"
              min={FONT_SCALE_MIN}
              max={FONT_SCALE_MAX}
              step={FONT_SCALE_STEP}
              value={scaleValue}
              onChange={(e: ChangeEvent<HTMLInputElement>) => {
                const v = Number(e.target.value);
                if (Number.isFinite(v)) setDraft(clampScale(v));
              }}
              onPointerUp={commitDraft}
              onKeyUp={commitDraft}
              onBlur={commitDraft}
            />
            <input
              key={scalePercent}
              type="number"
              className={`w-16 min-w-0 text-xs ${NUMBER_INPUT}`}
              min={Math.round(FONT_SCALE_MIN * 100)}
              max={Math.round(FONT_SCALE_MAX * 100)}
              step={Math.round(FONT_SCALE_STEP * 100)}
              defaultValue={scalePercent}
              onBlur={commitPercent}
              onKeyDown={(e) => e.key === "Enter" && commitPercent(e)}
            />
            <span className="font-display text-xs font-bold text-accent">%</span>
          </div>
        </ThemedControlCard>

        {SIZES.map(({ key, label, min, max }) => (
          <ThemedControlCard key={key} as="label" density="tight">
            <span className="text-xs font-medium text-text-secondary">{tr(label)}</span>
            <input
              type="number"
              className={`w-20 text-sm ${NUMBER_INPUT}`}
              min={min}
              max={max}
              step={0.05}
              placeholder="auto"
              value={fontSizes[key] ?? ""}
              onChange={(e) => themeSettings.setOptionalFontSize(key, parseOptionalRem(e.target.value))}
            />
          </ThemedControlCard>
        ))}
      </div>
    </div>
  );
}
