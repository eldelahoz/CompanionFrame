import { Scale } from "lucide-react";
import { useRef, type ReactNode } from "react";
import ItemThumb from "../market/ItemThumb";
import { Badge, Checkbox, IconButton, Table } from "../ui";
import { formatNumber } from "../../lib/format";
import type { Translator } from "../../lib/i18n";
import type { MyOrder } from "../../lib/ipc";
import type { PriceInfo } from "../../lib/inventory/itemPrices";
import type { InvRow } from "../../lib/inventory/inventoryRows";
import type { MasteryMark as Mark } from "../../lib/inventory/masteryMark";
import MasteryMark from "./MasteryMark";
import SellableTag from "./SellableTag";
import EquippedPopover from "./EquippedOn";
import type { Sellable } from "../../lib/inventory/sellable";
import { AverageValue, DucatValue, OwnedBadge, PlatinumValue, PriceTrend, RankIndicator } from "./ItemValue";
import { shiftOf } from "../../lib/useRangeSelect";
import { useVirtualRows } from "./useVirtualRows";

interface Props {
  rows: readonly InvRow[];
  prices: ReadonlyMap<string, PriceInfo>;
  selecting: boolean;
  selection: ReadonlySet<string>;
  onOpen: (row: InvRow) => void;
  onToggle: (row: InvRow, shift: boolean) => void;
  onBook: (row: InvRow) => void;
  extraOf?: (row: InvRow) => ReactNode;
  masteryOf?: (row: InvRow) => Mark | null;
  sellableOf?: (row: InvRow) => Sellable | undefined;
  showEquipped?: boolean;
  /** My sell order backing a row; the column shows only when some row has one. */
  onSaleOf?: (row: InvRow) => MyOrder | undefined;
  tr: Translator;
}

/** Rows turn into labelled cards in a narrow container (Table mode="stack"). */
export default function InventoryList({ rows, prices, selecting, selection, onOpen, onToggle, onBook, extraOf, masteryOf, sellableOf, showEquipped, onSaleOf, tr }: Props) {
  const anySale = !!onSaleOf && rows.some((r) => onSaleOf(r));
  const body = useRef<HTMLTableSectionElement>(null);
  const v = useVirtualRows(rows.length, "list", body);
  const items = v.getVirtualItems();
  // Spacer rows stand for the rows out of the window, so the table keeps its real height and column widths.
  const above = items.length ? items[0].start - v.options.scrollMargin : 0;
  const below = items.length ? v.getTotalSize() - items[items.length - 1].end : 0;
  const columns = 7 + (anySale ? 1 : 0) + (showEquipped ? 1 : 0);
  const spacer = (h: number) => (
    <tr aria-hidden="true" style={{ height: h, padding: 0, border: 0 }}>
      <td colSpan={columns} style={{ height: h, padding: 0, border: 0 }} />
    </tr>
  );
  return (
    <Table mode="stack" label={tr("inventory.viewList")} className="inv-list">
      <thead>
        <tr>
          <th>{tr("inventory.col.item")}</th>
          <th>{tr("inventory.col.type")}</th>
          <th className="ui-num">{tr("inventory.col.owned")}</th>
          <th className="ui-num">{tr("inventory.col.platinum")}</th>
          <th className="ui-num">{tr("inventory.col.average")}</th>
          <th className="ui-num">{tr("inventory.col.ducats")}</th>
          {anySale && <th>{tr("inventory.col.onSale")}</th>}
          {showEquipped && <th>{tr("inventory.equipped")}</th>}
          <th aria-hidden="true" />
        </tr>
      </thead>
      <tbody data-inventory-list ref={body}>
        {above > 0 && spacer(above)}
        {items.map((it) => {
          const row = rows[it.index];
          const info = row.slug ? prices.get(row.slug) : undefined;
          const picked = selection.has(row.key);
          return (
            <tr key={row.key} ref={v.measureElement} data-index={it.index} data-inventory-card={row.item.internalName} data-selected={picked || undefined}>
              <td data-primary>
                <div className="ui-item">
                  {selecting && <Checkbox checked={picked} onChange={(e) => onToggle(row, shiftOf(e))} aria-label={tr("inventory.selectItem", { name: row.label })} />}
                  <ItemThumb name={row.label} src={row.item.imageUrl} fallback={row.item.fallbackImageUrl} slug={row.artSlug} tr={tr} focusable={false} />
                  <button type="button" className="inv-link" onClick={(e) => (selecting ? onToggle(row, e.shiftKey) : onOpen(row))}
                    onMouseDown={(e) => selecting && e.shiftKey && e.preventDefault()}>
                    {row.label}
                  </button>
                  {row.item.vaulted && (
                    <Badge tone="warning" title={tr("common.vaulted")}>
                      V
                    </Badge>
                  )}
                  <MasteryMark mark={masteryOf?.(row)} tr={tr} />
                </div>
                {extraOf?.(row)}
              </td>
              <td data-label={tr("inventory.col.type")}>
                {row.item.categoryLabel}
                <RankIndicator row={row} tr={tr} />
              </td>
              <td className="ui-num" data-label={tr("inventory.col.owned")}>
                {row.item.inventoryGroup === "incomplete_sets" ? <OwnedBadge item={row.item} tr={tr} /> : `x${formatNumber(row.item.amount ?? 0)}`}
                {sellableOf?.(row) && <SellableTag sellable={sellableOf(row)!} tr={tr} />}
              </td>
              <td className="ui-num" data-label={tr("inventory.col.platinum")}>
                <span className="inv-price-fig">
                  <PriceTrend info={info} row={row} tr={tr} />
                  <PlatinumValue row={row} info={info} tr={tr} showNotTradable />
                </span>
              </td>
              <td className="ui-num" data-label={tr("inventory.col.average")}>
                <AverageValue row={row} info={info} tr={tr} />
              </td>
              <td className="ui-num" data-label={tr("inventory.col.ducats")}>
                <DucatValue row={row} />
              </td>
              {anySale && (
                <td data-label={tr("inventory.col.onSale")}>
                  {(() => {
                    const o = onSaleOf!(row);
                    return o ? <span className={o.visible ? undefined : "text-text-muted"}>{tr(o.visible ? "inventory.onSaleCell" : "inventory.onSaleCellHidden", { quantity: o.quantity, price: o.platinum })}</span> : null;
                  })()}
                </td>
              )}
              {showEquipped && (
                <td data-label={tr("inventory.equipped")}>
                  {row.item.equippedOn?.length ? <EquippedPopover list={row.item.equippedOn} variant="link" tr={tr} /> : <span className="text-text-muted">{tr("inventory.equipped.no")}</span>}
                </td>
              )}
              <td data-actions>
                {row.slug && (
                  <IconButton label={tr("inventory.openMarket")} onClick={() => onBook(row)}>
                    <Scale />
                  </IconButton>
                )}
              </td>
            </tr>
          );
        })}
        {below > 0 && spacer(below)}
      </tbody>
    </Table>
  );
}
