import { Tag } from "lucide-react";
import type { Translator } from "../../lib/i18n";
import type { MyOrder } from "../../lib/ipc";
import { openView } from "../../stores/nav";

/** Card view: the price of the sell order backing this item; muted while the order is hidden. Opens the Market. */
export default function SaleTag({ order, tr }: { order: MyOrder; tr: Translator }) {
  const title = tr(order.visible ? "inventory.onSaleTitle" : "inventory.hiddenSaleTitle", { quantity: order.quantity, price: order.platinum });
  return (
    <button
      type="button"
      className={`inv-sale-tag ${order.visible ? "" : "is-hidden"}`}
      data-inventory-sale-tag={order.visible ? "visible" : "hidden"}
      title={title}
      aria-label={title}
      onClick={() => openView("market")}
    >
      <Tag aria-hidden />
      {order.platinum}p
    </button>
  );
}
