import { Check, Pencil, Trash2 } from "lucide-react";
import { useState } from "react";
import { normalizePerTrade } from "../../../config/shared/wfmOrders";
import type { Translator } from "../../lib/i18n";
import { openItemBy } from "../../stores/modals";
import type { MyOrder } from "../../lib/ipc";
import { orderAge } from "../../lib/market/orderAge";
import { listingWarning, type ListingInventoryMatch } from "../../lib/marketListing";
import { GameIcon } from "../icons";
import { Badge, Button, Checkbox, IconButton, Switch } from "../ui";
import ItemThumb from "./ItemThumb";
import OrderStepper from "./OrderStepper";
import { shiftOf } from "../../lib/useRangeSelect";

interface Props {
  order: MyOrder;
  /** A write to this order is in flight; its other writes stay disabled. */
  busy: boolean;
  onEdit: (o: MyOrder) => void;
  onDelete: (id: string) => void;
  onCloseOne: (o: MyOrder) => void;
  onToggleVisible: (o: MyOrder) => void;
  onInlineSave: (o: MyOrder, u: { platinum: number; quantity: number }) => Promise<void>;
  /** Whether the listing still lines up with the inventory; absent = no opinion. */
  inventoryMatch?: ListingInventoryMatch | null;
  /** Public item art for the thumb (the item DB's), ahead of the wfmimg cache. */
  art?: string | null;
  selected: boolean;
  onSelect: (id: string, shift: boolean) => void;
  locale: string;
  tr: Translator;
}

/** One order as a table row (MarketView wraps it in a stacking ui Table). */
export default function MarketOrderRow({ order, busy, onEdit, onDelete, onCloseOne, onToggleVisible, onInlineSave, inventoryMatch = null, art = null, selected, onSelect, locale, tr }: Props) {
  // Drafts reset when the server value changes (a close patches the quantity) via the key on the row.
  const [platinum, setPlatinum] = useState(order.platinum);
  const [quantity, setQuantity] = useState(order.quantity);
  const dirty = platinum !== order.platinum || quantity !== order.quantity;
  const buy = order.orderType === "buy";
  const tradeSize = normalizePerTrade(order.perTrade);
  const warning = listingWarning(inventoryMatch, order.modRank, tr);
  const closeTitle = dirty
    ? tr("market.orderSoldApplyFirst")
    : tradeSize > 1
      ? tr(buy ? "market.orderBoughtTradeTitle" : "market.orderSoldTradeTitle", { count: tradeSize })
      : tr(buy ? "market.orderBoughtTitle" : "market.orderSoldTitle");

  return (
    <tr data-market-order={order.id} data-selected={selected || undefined}>
      <td>
        <Checkbox checked={selected} onChange={(e) => onSelect(order.id, shiftOf(e))} aria-label={tr("market.selectOrder", { name: order.itemName })} />
      </td>
      <td data-primary>
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <Badge tone={buy ? "info" : "warning"}>{buy ? "WTB" : "WTS"}</Badge>
          <ItemThumb name={order.itemName} slug={order.itemSlug} src={art} tr={tr} />
          <button type="button" className="ui-item-name cursor-pointer border-0 bg-transparent p-0 text-left text-[length:inherit] hover:underline" onClick={() => openItemBy({ name: order.itemName })}>
            {order.itemName}
          </button>
          {order.subtype && <Badge tone="accent">{order.subtype}</Badge>}
          {order.modRank != null && <span className="text-xs text-text-muted">R{order.modRank}</span>}
          {warning && (
            <span className="listing-warning" data-order-warning title={warning.title}>
              {warning.label}
            </span>
          )}
        </div>
      </td>
      <td data-label={tr("common.platinum")}>
        <span className="ui-readout">
          <GameIcon name="platinum" />
          <OrderStepper value={platinum} label={tr("common.platinum")} onChange={setPlatinum} tr={tr} />
        </span>
      </td>
      <td data-label={tr("market.col.quantity")}>
        <OrderStepper value={quantity} label={tr("market.quantity")} onChange={setQuantity} tr={tr} />
      </td>
      <td data-label={tr("market.col.listed")} title={order.createdAt ?? undefined}>
        {orderAge(order.createdAt, Date.now(), locale)}
      </td>
      <td data-label={tr(order.visible ? "market.liveLower" : "market.hiddenLower")}>
        <Switch checked={order.visible} disabled={busy} aria-label={tr(order.visible ? "market.setHidden" : "market.setVisible")} data-tip={tr(order.visible ? "market.setHidden" : "market.setVisible")} onChange={() => onToggleVisible(order)} />
      </td>
      <td data-actions>
        <div className="flex items-center justify-end gap-1">
          {dirty && (
            <Button size="sm" variant="primary" disabled={busy} onClick={() => onInlineSave(order, { platinum, quantity })}>
              {tr("market.applyNewPriceQty")}
            </Button>
          )}
          <Button size="sm" icon={<Check />} title={closeTitle} disabled={dirty || busy || order.quantity < tradeSize} onClick={() => onCloseOne(order)}>
            {tr(buy ? "market.orderBought" : "market.orderSold")}
          </Button>
          <IconButton label={tr("market.edit")} disabled={busy} onClick={() => onEdit(order)}>
            <Pencil />
          </IconButton>
          <IconButton label={tr("common.delete")} danger disabled={busy} onClick={() => onDelete(order.id)}>
            <Trash2 />
          </IconButton>
        </div>
      </td>
    </tr>
  );
}
