import { Lock } from "lucide-react";
import ItemImage from "../ItemImage";
import MarketMetricStrip from "../MarketMetricStrip";
import { RelicIcon, type RelicTier } from "../icons";
import { Badge } from "../ui";
import { itemLabel } from "../../lib/itemLabel";
import type { Translator } from "../../lib/i18n";
import { fissureTierClass, QUALITY_MODES, RELIC_QUALITY_LABEL_KEY, RELIC_QUALITY_SHORT_KEY } from "../../lib/relic/relicConstants";
import type { RowEvData } from "../../lib/relic/relicEv";
import type { RelicGroup, RelicQuality, RelicReward } from "../../lib/relic/types";
import type { RelicQualityMode } from "../../../config/shared/relicPlannerView";

interface Props {
  group: RelicGroup;
  qualityMode: RelicQualityMode;
  selectedOwned: RelicQuality | null;
  selected: RowEvData;
  rewardIcons: RelicReward[];
  ownedCount: (group: RelicGroup, quality: RelicQuality) => number;
  isOwnedReward: (reward: RelicReward) => boolean;
  rewardIconSrc: (reward: RelicReward) => string | null;
  rewardTooltip: (reward: RelicReward) => string;
  setOwnedQuality: (group: RelicGroup, quality: RelicQuality) => void;
  openRelic?: (group: RelicGroup) => void;
  tr: Translator;
}

const fmt = (v: number | null) => (v != null ? v.toFixed(1) : null);
const TIERS: ReadonlySet<string> = new Set(["lith", "meso", "neo", "axi", "requiem"]);

export default function RelicCompactCard({
  group,
  qualityMode,
  selectedOwned,
  selected,
  rewardIcons,
  ownedCount,
  isOwnedReward,
  rewardIconSrc,
  rewardTooltip,
  setOwnedQuality,
  openRelic,
  tr,
}: Props) {
  const tierClass = fissureTierClass(group.tier);
  const tier = (TIERS.has(tierClass) ? tierClass : tierClass === "omnia" ? "requiem" : "lith") as RelicTier;
  const label = (q: RelicQuality) => tr(RELIC_QUALITY_LABEL_KEY[q]);
  const total = QUALITY_MODES.reduce((sum, q) => sum + ownedCount(group, q), 0);
  const header =
    qualityMode === "owned"
      ? selectedOwned
        ? tr("relics.selectedEv", { quality: label(selectedOwned) })
        : tr("relics.selectedEvOwned")
      : tr("relics.selectedEv", { quality: label(qualityMode) });

  return (
    <div className="relic-compact-card ui-card" data-relic={group.key}>
      <button type="button" className="relic-compact-head" onClick={() => openRelic?.(group)} disabled={!openRelic}>
        <RelicIcon tier={tier} refinement={selectedOwned ?? (qualityMode === "owned" ? "intact" : qualityMode)} label={group.tier} className="relic-card-icon" />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="relic-row-name">{group.name}</span>
          <span className="text-xs text-text-muted">{total > 0 ? tr("browse.owned", { count: String(total) }) : tr("relics.detail.noneOwned")}</span>
        </span>
        {group.vaulted ? (
          <Badge icon={<Lock aria-hidden="true" />}>{tr("common.vaulted")}</Badge>
        ) : (
          <Badge tone="accent">{tr("common.unvaulted")}</Badge>
        )}
      </button>

      <div className="flex flex-col gap-1">
        <span className="text-xs text-text-muted">{header}</span>
        <MarketMetricStrip platinum={fmt(selected.plat)} ducats={fmt(selected.ducat)} ratio={fmt(selected.ratio)} state={selected.cls} size="compact" tr={tr} />
      </div>

      <span className="relic-quality-inline-counts" role="group" aria-label={tr("relics.qualityLabel")}>
        {QUALITY_MODES.map((quality) => {
          const count = ownedCount(group, quality);
          const selectable = qualityMode === "owned" && count > 0;
          return (
            <button
              key={quality}
              type="button"
              className={`relic-quality-inline-pill ${count === 0 ? "emptyCount" : ""} ${selectable ? "" : "notSelectable"} ${qualityMode === "owned" && selectedOwned === quality ? "active" : ""}`}
              title={`${label(quality)}: ${count}`}
              aria-pressed={qualityMode === "owned" && selectedOwned === quality}
              onClick={() => selectable && setOwnedQuality(group, quality)}
            >
              <RelicIcon tier={tier} refinement={quality} />
              <span className="sr-only">{tr(RELIC_QUALITY_SHORT_KEY[quality])}:</span>
              <span className={`relic-quality-inline-value ${count === 0 ? "emptyCount" : ""}`}>{count}</span>
            </button>
          );
        })}
      </span>

      <span className="relic-rewards">
        {rewardIcons.map((reward, i) => (
          <span key={`${reward.name}-${i}`} className={`relic-reward-preview-icon ${isOwnedReward(reward) ? "owned" : ""}`} data-tip={rewardTooltip(reward)}>
            <ItemImage src={rewardIconSrc(reward)} alt={itemLabel(reward.uniqueName ?? "", reward)} cls="relic-reward-preview-img" tr={tr} />
          </span>
        ))}
      </span>
    </div>
  );
}
