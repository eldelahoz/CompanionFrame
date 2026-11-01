import { Readout, StatStrip, type StatItem } from "../ui";
import type { Translator } from "../../lib/i18n";
import type { InventoryValueTotals } from "../../lib/inventory/valueTotals";

interface Props {
  /** Totals of every row that passes the tab, search and filters. */
  filtered: InventoryValueTotals;
  filteredCount: number;
  /** Totals of everything tradable; the filters never change it. */
  allTradable: InventoryValueTotals;
  /** Totals of the picked rows, shown while the selection mode has any. */
  selection: InventoryValueTotals | null;
  selectionCount: number;
  locale: string;
  tr: Translator;
}

function cell(locale: string, totals: InventoryValueTotals) {
  // A hole in the prices makes the real total higher, so the figure reads as a floor.
  const floor = (unpriced: number) => (unpriced > 0 ? "≥" : undefined);
  return (
    <span className="inv-value-line">
      <Readout
        currency="platinum"
        size="lg"
        label={floor(totals.platinumUnpriced)}
        value={totals.platinum}
        locale={locale}
      />
      {totals.ducats > 0 && (
        <Readout
          currency="ducat"
          chip
          label={floor(totals.ducatsUnpriced)}
          value={totals.ducats}
          locale={locale}
        />
      )}
    </span>
  );
}

export default function InventoryValueStrip({ filtered, filteredCount, allTradable, selection, selectionCount, locale, tr }: Props) {
  const items: StatItem[] = [];
  if (selection)
    items.push({
      key: "selection",
      label: tr("inventory.value.selected", { count: selectionCount }),
      value: cell(locale, selection),
    });
  items.push(
    {
      key: "filtered",
      label: tr("inventory.value.filtered", { count: filteredCount }),
      value: cell(locale, filtered),
      hint: filtered.unpriced > 0 ? tr("inventory.value.unpriced", { count: filtered.unpriced }) : undefined,
    },
    {
      key: "allTradable",
      label: tr("inventory.value.allTradableItems"),
      value: cell(locale, allTradable),
    },
  );
  return (
    <div className="inv-value-strip" data-inventory-value-strip title={tr("inventory.value.hint")}>
      <StatStrip items={items} />
    </div>
  );
}
