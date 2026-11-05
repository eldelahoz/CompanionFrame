import { useMemo, useState } from "react";
import type { Translator } from "../../lib/i18n";
import type { TradeEvent, TradeType } from "../../types/stats";
import { useMarketArt } from "../../lib/useMarketArt";
import ItemImage from "../ItemImage";
import { ArrowDown, ArrowLeftRight, ArrowUp, SearchX } from "lucide-react";
import { Badge, Button, Card, CardHeader, Chip, EmptyState, Readout, SearchInput } from "../ui";

type TradeFilter = "all" | TradeType;
const FILTERS: TradeFilter[] = ["all", "sale", "purchase", "trade"];
const PAGE_ROWS = 300;

const TYPE_LABEL = { sale: "stats.filterSale", purchase: "stats.filterPurchase", trade: "stats.filterTrade" } as const;
const TYPE_TONE = { sale: "success", purchase: "info", trade: "neutral" } as const;

export default function StatsTradePanel({ trades, locale, tr }: { trades: TradeEvent[]; locale: string; tr: Translator }) {
  const art = useMarketArt();
  const [filter, setFilter] = useState<TradeFilter>("all");
  const [search, setSearch] = useState("");
  const [visibleRows, setVisibleRows] = useState(PAGE_ROWS);

  const counts = useMemo(() => {
    const c: Record<TradeFilter, number> = { all: trades.length, sale: 0, purchase: 0, trade: 0 };
    for (const t of trades) c[t.type]++;
    return c;
  }, [trades]);
  const filtered = useMemo(() => {
    const q = search.toLowerCase();
    return trades.filter(
      (t) =>
        (filter === "all" || t.type === filter) &&
        (!q || t.items.some((i) => i.displayName.toLowerCase().includes(q)) || t.partner?.toLowerCase().includes(q)),
    );
  }, [trades, filter, search]);
  const visible = filtered.slice(0, visibleRows);
  const hidden = filtered.length - visible.length;
  const time = useMemo(() => new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }), [locale]);
  const when = (iso: string) => {
    const d = new Date(iso);
    return `${d.getMonth() + 1}/${d.getDate()}  ${time.format(d)}`;
  };

  return (
    <Card data-stats-trade-panel className="flex min-w-0 flex-col">
      <CardHeader title={tr("stats.trades")} meta={filtered.length.toLocaleString(locale)} />
      <div className="flex flex-col gap-2 px-3 pt-3">
        <div className="flex flex-wrap gap-1.5" data-trade-filters>
          {FILTERS.map((f) => (
            <Chip
              key={f}
              pressed={filter === f}
              data-trade-filter={f}
              onClick={() => {
                setFilter(f);
                setVisibleRows(PAGE_ROWS);
              }}
            >
              {f === "all" ? tr("common.all") : tr(TYPE_LABEL[f])}
              <span className="tabular-nums text-text-muted">{counts[f]}</span>
            </Chip>
          ))}
        </div>
        <SearchInput
          label={tr("stats.tradeSearchPlaceholder")}
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
            setVisibleRows(PAGE_ROWS);
          }}
          placeholder={tr("stats.tradeSearchPlaceholder")}
        />
      </div>

      <div className="max-h-[min(70vh,48rem)] overflow-y-auto p-3" data-stats-trade-list>
        {filtered.length === 0 ? (
          trades.length === 0 ? (
            <EmptyState icon={<ArrowLeftRight />} title={tr("common.noTradesYet")}>
              {tr("stats.noTradesDesc")}
            </EmptyState>
          ) : (
            <EmptyState icon={<SearchX />} title={tr("stats.noMatchingTrades")} />
          )
        ) : (
          <div className="flex flex-col gap-2">
            {visible.map((trade) => (
              <article key={trade.id} className="rounded-[var(--radius-md)] border border-border bg-bg-raised p-3 [contain-intrinsic-size:auto_110px] [content-visibility:auto]">
                <div className="mb-1.5 flex flex-wrap items-center gap-2" data-trade-row={trade.id}>
                  <Badge tone={TYPE_TONE[trade.type]}>{tr(TYPE_LABEL[trade.type])}</Badge>
                  {trade.wfmClosed && (
                    <Badge tone="accent" title={tr("stats.wfmAutoClosedTitle")}>
                      {tr("stats.wfmBadge")}
                    </Badge>
                  )}
                  {trade.platChange > 0 && <Readout currency="platinum" value={trade.type === "purchase" ? -trade.platChange : trade.platChange} sign={trade.type !== "trade"} locale={locale} />}
                  {trade.partner && (
                    <span data-trade-partner className="max-w-[8rem] truncate text-xs font-medium text-text-link">
                      {trade.partner}
                    </span>
                  )}
                  <span className="ml-auto whitespace-nowrap text-xs tabular-nums text-text-muted">{when(trade.date)}</span>
                </div>
                {trade.items.length > 0 && (
                  <div className="mt-1 flex flex-wrap gap-1">
                    {trade.items.map((item, i) => (
                      <span key={i} className="inline-flex max-w-full items-center gap-1 truncate rounded-[var(--radius-sm)] border border-border bg-bg-surface px-1.5 py-0.5 text-xs text-text-secondary">
                        {item.wfmSlug && <ItemImage src={art(item.wfmSlug)} slug={item.wfmSlug} cls="size-4! shrink-0 rounded-sm object-contain" tr={tr} />}
                        {item.direction === "received" ? <ArrowDown className="size-3 shrink-0 text-success" aria-label={tr("stats.itemReceived")} /> : <ArrowUp className="size-3 shrink-0 text-danger" aria-label={tr("stats.itemSent")} />}
                        {item.count > 1 ? `${item.count}×` : ""}
                        {item.displayName}
                      </span>
                    ))}
                  </div>
                )}
              </article>
            ))}
            {hidden > 0 && (
              <Button size="sm" className="mt-1 self-center" data-stats-trades-more={hidden} onClick={() => setVisibleRows((n) => n + PAGE_ROWS)}>
                {tr("browse.showMore", { n: hidden })}
              </Button>
            )}
          </div>
        )}
      </div>
    </Card>
  );
}
