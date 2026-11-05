import type { Translator } from "../../lib/i18n";
import type { Sellable } from "../../lib/inventory/sellable";

const why = (sellable: Sellable, tr: Translator) => (sellable.reasonKey ? tr(sellable.reasonKey, sellable.params as Record<string, string | number>) : undefined);

/** List view: "Sellable N" beside the quantity, the main rule that keeps the other copies as its tooltip. */
export default function SellableTag({ sellable, tr }: { sellable: Sellable; tr: Translator }) {
  return (
    <span className="inv-sellable" data-inventory-sellable title={why(sellable, tr)}>
      {tr("inventory.sellable", { count: sellable.count })}
    </span>
  );
}

/** Card view: a value row, "Sellable   1 of 3". */
export function SellableRow({ sellable, tr }: { sellable: Sellable; tr: Translator }) {
  return (
    <div className="inv-price-row" title={why(sellable, tr)}>
      <span className="inv-price-label">{tr("inventory.sellableLabel")}</span>
      <span className="inv-sellable-fig" data-inventory-sellable>
        {tr("inventory.sellableOf", { count: sellable.count, total: sellable.of })}
      </span>
    </div>
  );
}
