import type { CSSProperties, ReactNode } from "react";
import { Check, CircleHelp, Crown, Hammer, Lock, Timer } from "lucide-react";
import { REWARD_OVERLAY_CANVAS } from "../../../config/shared/rewardOverlayLayout";
import type { OverlayFieldStyle, OverlayLayout } from "../../../config/shared/overlayLayout";
import ItemImage from "../../components/ItemImage";
import { GameIcon, RARITY_ICON } from "../../components/icons";
import type { Translator } from "../../lib/i18n";
import { OvReadout } from "../common/OvReadout";
import { rewardStatusLabel, type RewardStatus } from "./rewardStatus";

/** Offset a field has on one card: its own entry, else the shared x/y. */
const cardOffset = (style: OverlayFieldStyle | undefined, key: string): { x: number; y: number } => style?.cards?.[key] ?? { x: style?.x ?? 0, y: style?.y ?? 0 };

/** What one card shows; `rewardCardViews` maps the live payload onto it. */
export interface CardView {
  name: string;
  rarity: "common" | "uncommon" | "rare" | null;
  rarityLabel: string;
  platinum: number | null;
  ducats: number | null;
  status: RewardStatus;
  /** Set progress, drawn as pips in the status badge; null when the badge already counts it or there is no set. */
  setOwned: { owned: number; required: number } | null;
  setPrice: number | null;
  foundry: boolean;
  vaulted: boolean;
  best: boolean;
  /** warframe.market slug (art through the wfmimg:// cache), then the item DB's image. */
  slug: string | null;
  imageUrl: string | null;
}

const REWARD_CARD_COUNT = 4;
/** The canvas is designed at 1080p, where 1em is 16px; em keeps it the same share of the screen at any height. */
const PX_PER_EM = 16;

/** The offset/scale/colour a saved field style gives one card's element. */
function fieldStyle(layout: OverlayLayout, field: string, slot: number): CSSProperties {
  const s = layout.fields[field];
  const { x, y } = cardOffset(s, String(slot));
  return {
    transform: `translate(${x}px, ${y}px) scale(${s?.scale ?? 1})`,
    transformOrigin: "left top",
    ...(s?.color ? { color: s.color } : {}),
    ...(s?.hidden ? { display: "none" } : {}),
  };
}

const STATUS_ICON: Record<RewardStatus["kind"], ReactNode> = {
  unknown: <CircleHelp className="ov-icon" aria-hidden />,
  mastered: <GameIcon name="mastery" className="ov-gi" />,
  unbuilt: <Hammer className="ov-icon" aria-hidden />,
  owned: <Check className="ov-icon" aria-hidden />,
  needed: null,
  notOwned: null,
};

/** Set progress as pips (one per part), with the k/n count as its own chip. */
function Pips({ owned, required }: { owned: number; required: number }) {
  return (
    <span className="ov-pips" aria-hidden>
      {Array.from({ length: Math.min(required, 8) }, (_, i) => (
        <i key={i} data-on={i < owned || undefined} />
      ))}
    </span>
  );
}

/** The status, led by the set progress pips when the item belongs to a set. */
function StatusBadge({ status, set, tr }: { status: RewardStatus; set?: CardView["setOwned"]; tr: Translator }) {
  const pips = status.kind === "needed" ? status : set;
  return (
    <span className="ov-badge" data-status={status.kind} title={set ? tr("overlay.reward.setParts", set) : undefined}>
      {pips && <Pips owned={pips.owned} required={pips.required} />}
      {STATUS_ICON[status.kind]}
      {rewardStatusLabel(status, tr)}
      {status.kind === "needed" && <b className="ov-badge-count">{`${status.owned}/${status.required}`}</b>}
    </span>
  );
}

/** Each movable element of a card, in the layout's field names; null = this card has no such element. */
const FIELDS: { field: string; render: (c: CardView, tr: Translator, slot: number) => ReactNode }[] = [
  { field: "slotLabel", render: (_, __, i) => `#${i + 1}` },
  {
    field: "rarity",
    render: (c) =>
      c.rarity && (
        <span className="rw-rarity">
          <GameIcon name={RARITY_ICON[c.rarity]} className="ov-gi" />
          <span className="rw-label">{c.rarityLabel}</span>
        </span>
      ),
  },
  {
    field: "vaulted",
    render: (c, tr) =>
      c.vaulted && (
        <span className="rw-tag">
          <Lock className="ov-icon" aria-hidden />
          <span className="rw-label">{tr("common.vaulted")}</span>
        </span>
      ),
  },
  { field: "itemName", render: (c) => <span className="rw-name">{c.name}</span> },
  {
    field: "platinumValue",
    render: (c, tr) => (c.platinum == null ? <span className="rw-noprice">{tr("overlay.reward.noPrice")}</span> : <OvReadout currency="platinum" value={c.platinum} size="lg" />),
  },
  { field: "ducatValue", render: (c) => c.ducats != null && <OvReadout currency="ducat" value={c.ducats} /> },
  { field: "setPrice", render: (c, tr) => c.setPrice != null && <span className="rw-set">{tr("overlay.reward.setPrice", { value: c.setPrice })}</span> },
  // Set progress rides in the status badge (pips), so `setOwned` has no element of its own any more.
  // "Unknown" (no inventory) says nothing the player needs, and its row pushed the pick chips onto the game's cards.
  { field: "owned", render: (c, tr) => (c.status.kind === "unknown" ? null : <StatusBadge status={c.status} set={c.setOwned} tr={tr} />) },
  {
    field: "foundry",
    render: (c, tr) =>
      c.foundry && (
        <span className="ov-badge" data-status="foundry">
          <Timer className="ov-icon" aria-hidden />
          {tr("overlay.reward.inFoundry")}
        </span>
      ),
  },
];
/** Row each field sits in; the layout offsets move a field from there. */
const ROW: Record<string, "meta" | "name" | "read" | "status"> = {
  slotLabel: "meta", rarity: "meta", vaulted: "meta", itemName: "name",
  platinumValue: "read", ducatValue: "read", setPrice: "read", owned: "status", foundry: "status",
};

/** The card row at its design size; every movable element carries `data-reward-field` for the stylesheet. */
export function RewardOverlayCanvas({ layout, cards, tr }: { layout: OverlayLayout; cards: CardView[]; tr: Translator }) {
  const { width, height } = REWARD_OVERLAY_CANVAS;
  // The slot label only shows when the user made it visible.
  const showSlot = layout.fields.slotLabel?.hidden === false;
  return (
    <div className="reward-canvas" style={{ width: `${width / PX_PER_EM}em`, height: `${height / PX_PER_EM}em` }}>
      {cards.slice(0, REWARD_CARD_COUNT).map((card, slot) => {
        const rows: Record<string, ReactNode[]> = { meta: [], name: [], read: [], status: [] };
        for (const { field, render } of FIELDS) {
          if (field === "slotLabel" && !showSlot) continue;
          const node = render(card, tr, slot);
          if (node == null || node === false) continue;
          rows[ROW[field]].push(
            <span
              key={field}
              className="reward-field"
              data-reward-field={field}
              style={fieldStyle(layout, field, slot)}
            >
              {node}
            </span>,
          );
        }
        return (
          <div key={slot} className="reward-card" data-reward-card={slot} data-rarity={card.rarity ?? undefined} data-best={card.best || undefined} style={{ width: `${100 / REWARD_CARD_COUNT}%` }}>
            <div className="rw">
              {card.best && (
                <span className="rw-pick">
                  <Crown className="ov-icon" aria-hidden />
                  {tr("overlay.reward.bestPick")}
                </span>
              )}
              <div className="rw-meta">{rows.meta}</div>
              <div className="rw-art">
                <ItemImage slug={card.slug} src={card.imageUrl} cls="ov-art" tr={tr} />
              </div>
              <div className="rw-row">{rows.name}</div>
              <div className="rw-row rw-read">{rows.read}</div>
              <div className="rw-row rw-status">{rows.status}</div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
