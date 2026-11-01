import type { Translator } from "../../lib/i18n";
import { Readout } from "../ui";

/** Best sell and buy offers of an item (or one rank of it) as two Readouts; without them the 48 h average stands in. */
export default function MarketOrderSummary({ wts, wtb, median, rank, tr }: { wts?: number | null; wtb?: number | null; median?: number | null; rank?: number | null; tr: Translator }) {
  const suffix = rank != null ? ` R${rank}` : "";
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2" data-order-summary>
      {wts !== undefined || wtb !== undefined ? (
        <>
          <Readout currency="platinum" chip label={`${tr("market.summary.wts")}${suffix}`} value={wts ?? null} />
          <Readout currency="platinum" chip label={`${tr("market.summary.wtb")}${suffix}`} value={wtb ?? null} />
        </>
      ) : (
        <Readout currency="platinum" chip label={tr("market.averageSale48h")} value={median ?? null} />
      )}
    </div>
  );
}
