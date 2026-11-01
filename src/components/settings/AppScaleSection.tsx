import { useState } from "react";
import type { Translator } from "../../lib/i18n";
import ThemedControlCard from "../ThemedControlCard";

// Same bounds as services/zoom.rs (which also rejects anything outside them).
const UI_SCALE_MIN = 0.75;
const UI_SCALE_MAX = 1.5;
const UI_SCALE_STEP = 0.05;

export default function AppScaleSection({ tr, scale, onCommit }: { tr: Translator; scale: number; onCommit: (scale: number) => void }) {
  const [draft, setDraft] = useState<number | null>(null);
  const value = draft ?? scale;
  // On release, not on input: each save re-zooms the window under the cursor.
  const commit = () => {
    if (draft !== null && draft !== scale) onCommit(draft);
    setDraft(null);
  };
  return (
    <div className="mb-3" data-app-scale>
      <h4 className="m-0 mb-[0.35rem] font-display text-[0.85rem] font-semibold tracking-[0.03em] text-text-primary">{tr("common.appSize")}</h4>
      <ThemedControlCard as="label" density="tight">
        <span className="text-xs font-medium text-text-secondary">
          {tr("appearance.appScaleRow")}
          <span className="mt-0.5 block text-xs font-normal text-text-muted">{tr("appearance.appScaleHint")}</span>
        </span>
        <div className="flex items-center gap-1.5">
          <input
            type="range"
            className="w-32 accent-accent"
            min={UI_SCALE_MIN}
            max={UI_SCALE_MAX}
            step={UI_SCALE_STEP}
            value={value}
            onChange={(e) => setDraft(Number(e.target.value))}
            onPointerUp={commit}
            onKeyUp={commit}
            onBlur={commit}
          />
          <span className="w-10 shrink-0 text-right font-display text-xs font-bold text-accent">{Math.round(value * 100)}%</span>
        </div>
      </ThemedControlCard>
    </div>
  );
}
