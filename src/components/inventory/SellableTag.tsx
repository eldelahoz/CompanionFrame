import type { Translator } from "../../lib/i18n";
import type { Sellable } from "../../lib/inventory/sellable";

/** "Sellable N" with the main rule that keeps the other copies as its tooltip. */
export default function SellableTag({ sellable, tr }: { sellable: Sellable; tr: Translator }) {
  const why = sellable.reasonKey ? tr(sellable.reasonKey, sellable.params as Record<string, string | number>) : undefined;
  return (
    <span className="inv-sellable" data-inventory-sellable title={why}>
      {tr("inventory.sellable", { count: sellable.count })}
    </span>
  );
}
