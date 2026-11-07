import type { ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import type { PriceInfo } from "../../lib/inventory/itemPrices";
import { rankCap, rowAvg, rowPrice, type InvRow } from "../../lib/inventory/inventoryRows";
import { formatNumber } from "../../lib/format";
import type { ParsedItem } from "../../types/inventory";
import { Badge, Readout, Skeleton } from "../ui";

const NotTradable = ({ tr }: { tr: Translator }) => <span className="inv-muted">{tr("inventory.notTradable")}</span>;

/** 48 h average sale price: skeleton while loading. Not-tradable items say so only where `showNotTradable` (the list). */
export function PlatinumValue({ row, info, tr, showNotTradable = false }: { row: InvRow; info?: PriceInfo; tr: Translator; showNotTradable?: boolean }): ReactNode {
  if (!row.slug) return showNotTradable ? <NotTradable tr={tr} /> : null;
  if (!info || info.state === "loading") return <Skeleton width="3.5rem" height="1.25rem" />;
  const { price, basis } = rowPrice(row, info);
  const hint = basis === "max" ? "inventory.priceMaxHint" : basis === "rank0" ? "inventory.priceRank0Hint" : "inventory.priceHint";
  return (
    <span title={tr(hint)}>
      <Readout currency="platinum" chip value={price} />
    </span>
  );
}

/** Median of the last week (muted, the label comes from the column or the card row); it arrives with the price. */
export function AverageValue({ row, info, tr }: { row: InvRow; info?: PriceInfo; tr: Translator }): ReactNode {
  if (!row.slug) return null;
  if (!info || info.avgState === "loading") return <Skeleton width="3.5rem" height="1.25rem" />;
  return (
    <span title={tr("inventory.averageHint")}>
      <Readout currency="platinum" className="inv-median" value={rowAvg(row, info)} />
    </span>
  );
}

/** Share of the 48 h price above or below the 7-day median, shown only when it differs by more than TREND_MIN; `compact` keeps just the arrow, for the narrow card. */
const TREND_MIN = 0.1;
export function PriceTrend({ info, tr, compact = false, row }: { info?: PriceInfo; tr: Translator; compact?: boolean; row?: InvRow }): ReactNode {
  // A max-rank price is compared with the max-rank median, which may not be loaded.
  const price = row ? rowPrice(row, info).price : (info?.price ?? null);
  const avg = row ? rowAvg(row, info) : (info?.avg ?? null);
  if (price == null || !avg || (row && rowPrice(row, info).basis === "max" && info?.maxAvg == null)) return null;
  const ratio = price / avg - 1;
  if (Math.abs(ratio) <= TREND_MIN) return null;
  const pct = formatNumber(Math.round(Math.abs(ratio) * 100));
  const up = ratio > 0;
  return (
    <span className={`inv-trend inv-trend--${up ? "up" : "down"}`} title={tr(up ? "inventory.trendUp" : "inventory.trendDown", { pct })}>
      {up ? "▲" : "▼"}
      {!compact && ` ${pct}%`}
    </span>
  );
}

/** One dot per rank, filled for the ranks reached, with "3 of 10" beside them, all in the accent colour at max; "Rank 3/10" is the accessible name and tooltip. Nothing for unranked items. */
export function RankIndicator({ row, tr }: { row: InvRow; tr: Translator }): ReactNode {
  const cap = rankCap(row);
  if (cap <= 0) return null;
  const rank = Math.min(Math.max(row.item.rank || 0, 0), cap);
  const text = tr("inventory.rank", { rank, max: cap });
  return (
    <span className="inv-rank" data-max={rank >= cap || undefined} data-many={cap > 10 || undefined} role="img" aria-label={text} title={text}>
      <span className="inv-rank-dots" aria-hidden="true">
        {Array.from({ length: cap }, (_, i) => (
          <span key={i} className="inv-rank-dot" data-on={i < rank || undefined} />
        ))}
      </span>
      <span className="inv-rank-text" aria-hidden="true">
        {tr("inventory.rankOf", { rank, max: cap })}
      </span>
    </span>
  );
}

/** Ducat value of a prime part, straight from the item database; nothing without one. */
export function DucatValue({ row }: { row: InvRow }): ReactNode {
  return row.item.ducats ? <Readout currency="ducat" chip value={row.item.ducats} /> : null;
}

/** The stack count, or "k/n parts" for an incomplete set (it has no stack). */
export function OwnedBadge({ item, tr }: { item: ParsedItem; tr: Translator }): ReactNode {
  if (item.inventoryGroup !== "incomplete_sets") return <Badge tone="success">x{item.amount ?? 0}</Badge>;
  const progress = tr("inventory.sets.progress", { owned: item.ownedPartTypes ?? 0, total: item.totalPartTypes ?? 0 });
  // Compact like the "xN" badge so it does not cover the art; the full text is the tooltip.
  return (
    <Badge tone="accent" title={progress}>
      {item.ownedPartTypes ?? 0}/{item.totalPartTypes ?? 0}
    </Badge>
  );
}
