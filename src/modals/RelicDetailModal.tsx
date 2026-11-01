import { Lock, X } from "lucide-react";
import { useMemo, useState } from "react";
import { highestOwnedQuality } from "../../config/shared/relicPlannerView";
import { sortRelicRewards } from "../../config/shared/relicRewardOrder";
import ItemImage from "../components/ItemImage";
import { RelicIcon, type RelicTier } from "../components/icons";
import WikiButton from "../components/WikiButton";
import { Badge, Button, IconButton, Readout, Segmented, Select, Table } from "../components/ui";
import { fissureTierClass, QUALITY_MODES, RELIC_QUALITY_LABEL_KEY } from "../lib/relic/relicConstants";
import { computeSquadEV } from "../lib/relic/relicMath";
import { relicPrice, useRelicPrices } from "../lib/relic/relicPrices";
import { simpleRelicFor } from "../lib/relic/relicView";
import type { RelicGroup, RelicQuality } from "../lib/relic/types";
import { openItemBy, setActiveItem, setActiveRelic } from "../stores/modals";
import DetailModalBase from "./DetailModalBase";
import RelicViewSwitch from "./RelicViewSwitch";
import type { ModalCtx } from "./types";

const SQUADS = [
  [1, "relics.squad.solo"],
  [2, "relics.squad.p2"],
  [3, "relics.squad.p3"],
  [4, "relics.squad.p4"],
] as const;
const TIERS: ReadonlySet<string> = new Set(["lith", "meso", "neo", "axi", "requiem"]);

/** The relic's rewards per refinement, with live platinum, ducats and the squad expected value. */
export default function RelicDetailModal({ ctx, group }: { ctx: ModalCtx; group: RelicGroup }) {
  const { tr } = ctx;
  const qualities = QUALITY_MODES.filter((q) => group.qualities[q]);
  const owned = ctx.relicOwned[group.key];
  const [picked, setPicked] = useState<RelicQuality | null>(null);
  const quality: RelicQuality = picked ?? highestOwnedQuality(qualities, (q) => owned?.[q] ?? 0) ?? qualities[0] ?? "intact";
  const [squad, setSquad] = useState(1);

  const rewards = useMemo(() => sortRelicRewards(group.qualities[quality]?.rewards ?? []), [group, quality]);
  const rev = useRelicPrices(rewards.flatMap((r) => (r.urlName ? [r.urlName] : [])));
  // `rev` bumps as prices land, so the memo recomputes without listing the module-level cache.
  const plats = useMemo(() => rewards.map((r) => (r.urlName ? relicPrice(r.urlName) : null)), [rewards, rev]); // eslint-disable-line react-hooks/exhaustive-deps
  const ducats = rewards.map((r) => (r.ducats != null && r.ducats >= 0 ? r.ducats : null));
  const hasPlat = plats.some((p) => p != null);
  const hasDucats = ducats.some((d) => d != null);
  const platEv = hasPlat ? computeSquadEV(rewards, plats, squad) : null;
  const ducatEv = hasDucats ? computeSquadEV(rewards, ducats, squad) : null;

  const tierClass = fissureTierClass(group.tier);
  const tier = (TIERS.has(tierClass) ? tierClass : tierClass === "omnia" ? "requiem" : "lith") as RelicTier;
  const close = () => setActiveRelic(null);
  const showSimple = () => {
    const item = simpleRelicFor(group, ctx.db, ctx.ownership, quality);
    if (!item) return;
    setActiveRelic(null);
    setActiveItem(item);
  };
  const openReward = (uniqueName: string | null | undefined, name: string) => {
    close();
    openItemBy(uniqueName ? { uniqueName } : { name });
  };
  const ownedTotal = QUALITY_MODES.reduce((s, q) => s + (owned?.[q] ?? 0), 0);

  return (
    <DetailModalBase ariaLabel={group.name} tr={tr} onClose={close}>
      <div className="detail-panel-top-actions" data-relic-detail={group.key}>
        <RelicViewSwitch view="detailed" onSwitch={showSimple} tr={tr} />
        <WikiButton fallbackName={`${group.tier} ${group.code}`} tr={tr} />
        <IconButton label={tr("common.close")} onClick={close}>
          <X />
        </IconButton>
      </div>

      <div className="detail-header">
        <div className="detail-img-wrap">
          <RelicIcon tier={tier} refinement={quality} label={group.tier} className="relic-card-icon" />
        </div>
        <div className="detail-title-area">
          <h2>{group.name}</h2>
          <div className="detail-tags">
            {group.vaulted ? <Badge icon={<Lock aria-hidden="true" />}>{tr("common.vaulted")}</Badge> : <Badge tone="accent">{tr("common.unvaulted")}</Badge>}
            <span className="text-xs text-text-muted">{ownedTotal > 0 ? tr("browse.owned", { count: String(ownedTotal) }) : tr("relics.detail.noneOwned")}</span>
          </div>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-3 px-4 py-2">
        <Segmented
          label={tr("relics.qualityLabel")}
          active={quality}
          onSelect={setPicked}
          items={qualities.map((q) => ({ key: q, label: `${tr(RELIC_QUALITY_LABEL_KEY[q])}${owned?.[q] ? ` (${owned[q]})` : ""}` }))}
        />
        <label className="flex items-center gap-2 text-xs text-text-muted">
          {tr("relics.detail.squadLabel")}
          <Select title={tr("relics.squadTitle")} value={squad} options={SQUADS.map(([n, key]) => [n, tr(key)] as const)} onChange={(e) => setSquad(Number(e.target.value))} />
        </label>
      </div>

      <div className="flex flex-wrap items-center gap-3 px-4 pb-2 text-xs text-text-muted" data-relic-ev>
        <span>{tr("relics.detail.col.ev")}:</span>
        {hasPlat || hasDucats ? (
          <>
            <Readout currency="platinum" value={platEv} digits={1} chip />
            <Readout currency="ducat" value={ducatEv} digits={1} chip />
          </>
        ) : (
          <span>{tr("relics.detail.evUnavailable", { quality: tr(RELIC_QUALITY_LABEL_KEY[quality]) })}</span>
        )}
        {!hasPlat && rewards.some((r) => r.urlName) && <span>{tr("relics.detail.loadingPrices")}</span>}
      </div>

      <div className="px-4 pb-4">
        <Table mode="scroll" label={group.name}>
          <thead>
            <tr>
              <th>{tr("common.name")}</th>
              <th>{tr("wiki.col.rarity")}</th>
              <th className="ui-num">{tr("common.chance")}</th>
              <th className="ui-num">{tr("common.platinum")}</th>
              <th className="ui-num">{tr("common.ducats")}</th>
            </tr>
          </thead>
          <tbody>
            {rewards.map((reward, i) => (
              <tr key={reward.name}>
                <td>
                  <Button variant="ghost" size="sm" title={tr("common.open", { name: reward.name })} onClick={() => openReward(reward.uniqueName, reward.name)}>
                    <ItemImage src={reward.imageUrl} alt="" cls="relic-reward-preview-img" tr={tr} />
                    {reward.name}
                  </Button>
                </td>
                <td>{reward.rarity}</td>
                <td className="ui-num">
                  {reward.chance != null ? `${reward.chance}%` : "-"}
                </td>
                <td className="ui-num">
                  <Readout currency="platinum" value={plats[i]} />
                </td>
                <td className="ui-num">
                  <Readout currency="ducat" value={ducats[i]} />
                </td>
              </tr>
            ))}
          </tbody>
        </Table>
      </div>
    </DetailModalBase>
  );
}
