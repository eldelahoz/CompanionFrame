import { Scale } from "lucide-react";
import type { ReactNode } from "react";
import ItemImage from "../ItemImage";
import { useItemPreview } from "../market/ItemThumb";
import { Badge, Checkbox, IconButton } from "../ui";
import type { Translator } from "../../lib/i18n";
import type { PriceInfo } from "../../lib/inventory/itemPrices";
import type { InvRow } from "../../lib/inventory/inventoryRows";
import type { MasteryMark as Mark } from "../../lib/inventory/masteryMark";
import MasteryMark from "./MasteryMark";
import { AverageValue, DucatValue, OwnedBadge, PlatinumValue, PriceTrend, RankIndicator } from "./ItemValue";

interface Props {
  row: InvRow;
  info?: PriceInfo;
  avgWanted: boolean;
  onLoadAverage: (slug: string) => void;
  selecting: boolean;
  selected: boolean;
  onOpen: () => void;
  onToggle: () => void;
  onBook: () => void;
  /** Extra block under the values (an incomplete set's missing parts). */
  extra?: ReactNode;
  mastery?: Mark | null;
  /** Copies safe to sell; set only while a mod chip is on. */
  sellable?: number;
  tr: Translator;
}

/** One owned item: art, name, stack and its value chips. In selection mode the whole card toggles. */
export default function InventoryCard({ row, info, avgWanted, onLoadAverage, selecting, selected, onOpen, onToggle, onBook, extra, mastery, sellable, tr }: Props) {
  const { item } = row;
  const { bind, node } = useItemPreview({ name: row.label, slug: row.artSlug, src: item.imageUrl, fallback: item.fallbackImageUrl, tr });
  return (
    <article className="inv-card ui-card" data-inventory-card={item.internalName} data-selected={selected || undefined}>
      <button
        type="button"
        className="inv-card-main"
        onClick={selecting ? onToggle : onOpen}
        aria-label={selecting ? tr("inventory.selectItem", { name: row.label }) : row.label}
        aria-pressed={selecting ? selected : undefined}
        {...bind}
      >
        <span className="inv-card-art">
          <ItemImage src={item.imageUrl} fallback={item.fallbackImageUrl} slug={row.artSlug} alt="" tr={tr} cls="inv-card-img" />
        </span>
        <span className="inv-card-name">{row.label}</span>
        <span className="inv-card-type">{item.categoryLabel}</span>
      </button>
      <div className="inv-card-badges">
        {selecting ? <Checkbox checked={selected} onChange={onToggle} aria-label={tr("inventory.selectItem", { name: row.label })} /> : <OwnedBadge item={item} tr={tr} />}
        {item.vaulted && (
          <Badge tone="warning" title={tr("common.vaulted")}>
            V
          </Badge>
        )}
        <MasteryMark mark={mastery} tr={tr} />
        {sellable !== undefined && <span className="inv-sellable" data-inventory-sellable>{tr("inventory.mods.sellable", { count: sellable })}</span>}
      </div>
      {row.slug && (
        <IconButton label={tr("inventory.openMarket")} className="inv-card-book" onClick={onBook}>
          <Scale />
        </IconButton>
      )}
      <div className="inv-card-values">
        <RankIndicator row={row} tr={tr} />
        {row.slug && (
          <>
            <div className="inv-price-row">
              <span className="inv-price-label">{tr(item.inventoryGroup === "incomplete_sets" ? "inventory.sets.price" : "inventory.col.platinum")}</span>
              <span className="inv-price-fig">
                <PriceTrend info={info} row={row} tr={tr} compact />
                <PlatinumValue row={row} info={info} tr={tr} />
              </span>
            </div>
            <div className="inv-price-row">
              <span className="inv-price-label">{tr("inventory.col.average")}</span>
              <span className="inv-price-fig">
                <AverageValue row={row} info={info} wanted={avgWanted} onLoad={onLoadAverage} tr={tr} />
              </span>
            </div>
          </>
        )}
        {!!row.item.ducats && (
          <div className="inv-price-row inv-price-row--ducat">
            <DucatValue row={row} />
          </div>
        )}
      </div>
      {extra}
      {node}
    </article>
  );
}
