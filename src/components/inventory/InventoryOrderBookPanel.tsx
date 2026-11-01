import { Copy, ExternalLink, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import ItemImage from "../ItemImage";
import MarketOrderSummary from "../market/MarketOrderSummary";
import { Badge, Card, EmptyState, IconButton, Modal, Readout, Segmented, Select, Skeleton } from "../ui";
import type { Translator } from "../../lib/i18n";
import { fetchItemOrderBookBySlug, type ItemOrderBook, type OrderBookEntry } from "../../lib/wfm/orderBook";
import { matchesSellerStatus, orderWhisper, sellerStatusLabelKey, type SellerStatusFilter } from "../../lib/wfm/orderRows";
import { addToast } from "../../stores/toasts";
import type { InvRow } from "../../lib/inventory/inventoryRows";

const PAGE = 8;
type State = { status: "loading" } | { status: "error" } | { status: "none" } | { status: "ok"; book: ItemOrderBook };

function Side({ side, entries, row, tr }: { side: "sell" | "buy"; entries: OrderBookEntry[]; row: InvRow; tr: Translator }) {
  const copy = async (entry: OrderBookEntry) => {
    try {
      await navigator.clipboard.writeText(orderWhisper(tr, side, entry, row.item.name));
      addToast({ level: "success", message: tr("browse.whisperCopied") });
    } catch {
      addToast({ level: "error", message: tr("common.copyFailed") });
    }
  };
  return (
    <section className="inv-book-side" data-book-side={side}>
      <h3>{tr(side === "sell" ? "orderbook.wts" : "orderbook.wtb")}</h3>
      {entries.length === 0 ? (
        <p className="text-sm text-text-muted">{tr(side === "sell" ? "orderbook.noSellOrders" : "orderbook.noBuyOrders")}</p>
      ) : (
        <ul>
          {entries.slice(0, PAGE).map((e, i) => (
            <li key={`${e.userName}-${i}`}>
              <Readout currency="platinum" chip value={e.unitPlatinum ?? e.platinum} />
              {e.perTrade > 1 && <span className="text-xs text-text-muted">{tr("orderbook.perTrade", { count: e.perTrade, platinum: e.platinum })}</span>}
              {e.rank != null && <Badge>{tr("orderbook.rankOption", { rank: e.rank })}</Badge>}
              <span className="inv-book-user" title={tr(sellerStatusLabelKey(e.status))}>
                <span className="inv-book-dot" data-status={e.status ?? "offline"} aria-hidden="true" />
                {e.userName}
                <span className="sr-only">{tr(sellerStatusLabelKey(e.status))}</span>
              </span>
              <IconButton label={tr("orderbook.whisper")} onClick={() => void copy(e)}>
                <Copy />
              </IconButton>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Book({ row, tr }: { row: InvRow; tr: Translator }) {
  const slug = row.slug as string;
  const maxRank = row.maxRank;
  const [state, setState] = useState<State>({ status: "loading" });
  const [status, setStatus] = useState<SellerStatusFilter>("ingame");
  const [rank, setRank] = useState<number | null>(maxRank ? 0 : null);

  useEffect(() => {
    let live = true;
    setState({ status: "loading" });
    void (async () => {
      let result = await fetchItemOrderBookBySlug(slug, { rank });
      if (result.status === "error") result = await fetchItemOrderBookBySlug(slug, { rank });
      if (live) setState(result.status === "ok" ? { status: "ok", book: result.data } : { status: result.status === "not_found" ? "none" : "error" });
    })();
    return () => {
      live = false;
    };
  }, [slug, rank]);

  const shown = useMemo(() => {
    if (state.status !== "ok") return null;
    const keep = (list: OrderBookEntry[]) => list.filter((e) => matchesSellerStatus(e.status, status));
    return { sell: keep(state.book.sell), buy: keep(state.book.buy) };
  }, [state, status]);

  const best = (list: OrderBookEntry[] | undefined) => (list?.[0] ? (list[0].unitPlatinum ?? list[0].platinum) : null);
  const spread = shown && best(shown.sell.slice(0, 1)) != null && best(shown.buy.slice(0, 1)) != null ? (best(shown.sell) as number) - (best(shown.buy) as number) : null;

  return (
    <>
      <div className="inv-book-controls">
        <Segmented
          label={tr("browse.status.ingame")}
          active={status}
          onSelect={setStatus}
          items={[
            { key: "ingame", label: tr("orderbook.statusIngame") },
            { key: "onsite", label: tr("orderbook.statusOnline") },
            { key: "all", label: tr("orderbook.statusAll") },
          ]}
        />
        {maxRank > 0 && (
          <Select
            aria-label={tr("orderbook.rankView")}
            value={rank ?? 0}
            options={[0, maxRank].map((r) => [r, tr("orderbook.rankOption", { rank: r })] as const)}
            onChange={(e) => setRank(Number(e.target.value))}
          />
        )}
      </div>
      <MarketOrderSummary wts={best(shown?.sell)} wtb={best(shown?.buy)} rank={rank} tr={tr} />
      {spread != null && <Readout currency="platinum" label={tr("orderbook.spread")} value={spread} />}
      {state.status === "loading" && (
        <div className="inv-book-loading" aria-busy="true">
          <Skeleton height="1.6rem" />
          <Skeleton height="1.6rem" />
          <Skeleton height="1.6rem" />
        </div>
      )}
      {(state.status === "none" || state.status === "error") && <EmptyState title={tr(state.status === "none" ? "orderbook.noActiveListings" : "market.priceLoadFailed")} />}
      {shown && (
        <div className="inv-book-sides">
          <Side side="sell" entries={shown.sell} row={row} tr={tr} />
          <Side side="buy" entries={shown.buy} row={row} tr={tr} />
        </div>
      )}
    </>
  );
}

const wide = () => typeof window !== "undefined" && window.matchMedia?.("(min-width: 1180px)").matches;

/** Live WTS/WTB listings of one item: a sticky side panel on wide windows, a sheet when narrow. */
export default function InventoryOrderBookPanel({ row, onClose, tr }: { row: InvRow; onClose: () => void; tr: Translator }) {
  const [inline, setInline] = useState(wide);
  useEffect(() => {
    const mq = window.matchMedia?.("(min-width: 1180px)");
    if (!mq) return;
    const on = () => setInline(mq.matches);
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  const title = tr("orderbook.title2", { name: row.label });
  const open = (
    <a className="ui-btn ui-btn--ghost ui-btn--sm" href={`https://warframe.market/items/${row.slug}`} target="_blank" rel="noreferrer noopener">
      <ExternalLink aria-hidden="true" />
      {tr("orderbook.openWfm")}
    </a>
  );
  if (!inline) {
    return (
      <Modal title={title} onClose={onClose} tr={tr} width="38rem" footer={open}>
        <div className="inv-book" data-inventory-book={row.slug}>
          <Book row={row} tr={tr} />
        </div>
      </Modal>
    );
  }
  return (
    <Card as="section" className="inv-book-panel" data-inventory-book={row.slug} aria-label={title}>
      <header className="inv-book-head">
        <span className="ui-thumb">
          <ItemImage src={row.item.imageUrl} fallback={row.item.fallbackImageUrl} slug={row.artSlug} alt="" tr={tr} cls="h-full w-full" />
        </span>
        <h2>{title}</h2>
        <IconButton label={tr("common.close")} onClick={onClose}>
          <X />
        </IconButton>
      </header>
      <div className="inv-book">
        <Book row={row} tr={tr} />
      </div>
      <footer>{open}</footer>
    </Card>
  );
}
