import type { ReactNode } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import SegmentedControl from "../SegmentedControl";
import ThemedControlCard from "../ThemedControlCard";
import { usePersisted } from "../../lib/usePersisted";
import { THEME_CORNER_OPTIONS, THEME_SURFACE_OPTIONS } from "../../lib/theme/styleOptions";
import { themeSettings, useThemeSettings } from "../../lib/theme/themeStore";
import type { RelicCardStyle } from "../../types/theme";
import GlassBlurControl from "./GlassBlurControl";

type Option<T extends string> = { value: T; labelKey: MessageKey };

const RELIC_CARDS: Option<RelicCardStyle>[] = [
  { value: "ornate", labelKey: "common.detailed" },
  { value: "plain", labelKey: "appearance.relicCardsPlain" },
];
const RIVEN_SIZES = ["full", "compact"] as const;
const RIVEN_CARDS: Option<(typeof RIVEN_SIZES)[number]>[] = [
  { value: "full", labelKey: "appearance.surfaceFull" },
  { value: "compact", labelKey: "appearance.rivenCardsCompact" },
];
const DENSITIES = ["compact", "row"] as const;
const DENSITY_OPTIONS: Option<(typeof DENSITIES)[number]>[] = [
  { value: "compact", labelKey: "appearance.densityCards" },
  { value: "row", labelKey: "appearance.densityRows" },
];
const VIEW_MODES = ["cards", "list"] as const;
const VIEW_MODE_OPTIONS: Option<(typeof VIEW_MODES)[number]>[] = [
  { value: "cards", labelKey: "appearance.densityCards" },
  { value: "list", labelKey: "appearance.densityRows" },
];

const LABEL = "min-w-0 flex-[1_1_5rem] text-text-secondary text-xs font-medium";
const HINT = "block text-xs text-text-muted font-normal mt-0.5";

function Row<T extends string>({ label, hint, options, value, onChange, tr, attr }: { label: string; hint?: string; options: readonly Option<T>[]; value: T; onChange: (v: T) => void; tr: Translator; attr?: Record<string, string> }): ReactNode {
  return (
    <ThemedControlCard>
      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1.5">
        <span className={LABEL}>
          {label}
          {hint && <span className={HINT}>{hint}</span>}
        </span>
        <span className="shrink-0" {...attr}>
          <SegmentedControl value={value} options={options.map((o) => ({ value: o.value, label: tr(o.labelKey) }))} onChange={onChange} />
        </span>
      </div>
    </ThemedControlCard>
  );
}

// The three view preferences live in localStorage under WFHelper's keys; the Rivens, Market and Inventory views read them.
export default function StyleSection({ tr }: { tr: Translator }) {
  const { effects } = useThemeSettings();
  const [rivenSize, setRivenSize] = usePersisted("wf_rivens_card_size", RIVEN_SIZES, "full");
  const [density, setDensity] = usePersisted("ui.marketDensity", DENSITIES, "compact");
  const [viewMode, setViewMode] = usePersisted("wf_inventory_view_mode", VIEW_MODES, "cards");

  return (
    <div className="mb-3" data-style-section>
      <h4 className="m-0 mb-[0.35rem] font-display text-[0.85rem] font-semibold tracking-[0.03em] text-text-primary">{tr("appearance.style")}</h4>
      <div className="grid gap-2">
        <Row tr={tr} label={tr("appearance.cornerStyle")} options={THEME_CORNER_OPTIONS} value={effects.cornerStyle} onChange={(v) => themeSettings.setEffects({ cornerStyle: v })} />
        <Row tr={tr} label={tr("appearance.surfaceStyle")} options={THEME_SURFACE_OPTIONS} value={effects.surfaceStyle} onChange={(v) => themeSettings.setEffects({ surfaceStyle: v })} />
        <ThemedControlCard>
          <GlassBlurControl tr={tr}>
            <span className={LABEL}>
              {tr("common.glassBlur")}
              <span className={HINT}>{tr("appearance.glassHint")}</span>
            </span>
          </GlassBlurControl>
        </ThemedControlCard>
        <Row tr={tr} label={tr("appearance.relicCards")} options={RELIC_CARDS} value={effects.relicCardStyle} onChange={(v) => themeSettings.setEffects({ relicCardStyle: v })} />
        <Row tr={tr} label={tr("appearance.rivenCardSize")} options={RIVEN_CARDS} value={rivenSize} onChange={setRivenSize} attr={{ "data-riven-card-size-control": "" }} />
        <Row tr={tr} label={tr("appearance.marketListDensity")} hint={tr("appearance.marketListDensityHint")} options={DENSITY_OPTIONS} value={density} onChange={setDensity} />
        <Row tr={tr} label={tr("appearance.inventoryViewMode")} hint={tr("appearance.inventoryViewModeHint")} options={VIEW_MODE_OPTIONS} value={viewMode} onChange={setViewMode} attr={{ "data-inventory-view-mode": "" }} />
      </div>
    </div>
  );
}
