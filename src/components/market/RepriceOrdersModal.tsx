import { useEffect, useMemo, useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { jobsDismiss, jobsMarketBooks, jobsStartMarketLoad, jobsStartReprice, type MyOrder } from "../../lib/ipc";
import { isActive, useJobs } from "../../stores/jobs";
import { rowBookOf } from "../../lib/tradeWorkbench/rowBook";
import { addToast } from "../../stores/toasts";
import { buildRepriceRows, priceRepriceRow, repriceRowsToSend, repriceTotals, type RepriceRow, type RepriceSkipReason } from "../../lib/market/repriceOrders";
import { workbenchMedianLoader } from "../../lib/tradeWorkbench/medianReference";
import { DEFAULT_DAMPING_RULE, isMedianStrategy, type StrategyConfig, type WorkbenchStrategyId } from "../../lib/tradeWorkbench/pricingStrategies";
import SegmentedControl from "../SegmentedControl";
import { shiftOf, useRangeSelect } from "../../lib/useRangeSelect";

// shortcut: no target-margin here, an order has no cost basis to price against; BulkSellModal asks for one by hand.
const STRATEGIES = ["match-cheapest", "cheapest-minus-one", "percent-offset", "median-48h", "median-90d"] as const satisfies readonly WorkbenchStrategyId[];
type StrategyId = (typeof STRATEGIES)[number];
type Filter = "all" | "changed" | "skipped";

const SKIP_KEY = {
  "no-book": "market.reprice.skip.noBook",
  "no-price": "market.reprice.skip.noPrice",
  "not-sell": "market.reprice.skip.notSell",
  unchanged: "market.reprice.skip.unchanged",
} as const satisfies Record<RepriceSkipReason, string>;

interface Props {
  orders: readonly MyOrder[];
  ownUserName: string | null;
  onClose: () => void;
  tr: Translator;
}

export default function RepriceOrdersModal({ orders, ownUserName, onClose, tr }: Props) {
  const [rows, setRows] = useState<RepriceRow[]>(() => buildRepriceRows(orders));
  const [strategyId, setStrategyId] = useState<StrategyId>("cheapest-minus-one");
  const [percent, setPercent] = useState(-1);
  const [medianOffset, setMedianOffset] = useState(0);
  // Bumped when median references land: the loader keeps them outside React state.
  const [medianRev, setMedianRev] = useState(0);
  const [filter, setFilter] = useState<Filter>("all");
  const marketJobs = useJobs().jobs.filter((j) => j.kind.type === "marketLoad");
  const marketJob = marketJobs[marketJobs.length - 1] ?? null;
  const busy = marketJob != null && isActive(marketJob);
  const marketDone = marketJob?.state === "done";
  const progress = busy ? { done: marketJob.done + marketJob.failed, total: marketJob.total } : null;
  const booksProbed = useRef(false);
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const cancelled = useRef(false);
  // reset on mount: StrictMode runs the cleanup once before the real mount
  useEffect(() => {
    cancelled.current = false;
    return () => void (cancelled.current = true);
  }, []);

  const config: StrategyConfig = useMemo(
    () => (strategyId === "percent-offset" ? { id: strategyId, percent } : isMedianStrategy(strategyId) ? { id: strategyId, offsetPlat: medianOffset } : { id: strategyId }),
    [strategyId, percent, medianOffset],
  );
  // Pricing is a pure function of the loaded book (and median references), so a strategy change re-prices in place without a refetch.
  const priced = useMemo(
    () => rows.map((r) => priceRepriceRow(r, config, ownUserName, DEFAULT_DAMPING_RULE, workbenchMedianLoader.peek(config.id, r))),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [rows, config, ownUserName, medianRev],
  );
  const booked = rows.filter((r) => r.sellBook !== null).length;
  // A median strategy needs its reference per priced row: fetch it once the books are in or the strategy changes.
  useEffect(() => {
    if (!isMedianStrategy(config.id) || booked === 0 || busy) return;
    const id = config.id;
    let stale = false;
    void workbenchMedianLoader.load(id, rows, () => stale || cancelled.current).then(() => !stale && setMedianRev((n) => n + 1));
    return () => void (stale = true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config.id, booked, busy]);
  const totals = repriceTotals(priced);
  const shown = priced.filter((r) => filter === "all" || (filter === "changed" ? r.skipReason === null : r.skipReason !== null));

  // Applies the books the job already loaded to the rows still without one; a failed read leaves the row without a book (no-book).
  const applyLoadedBooks = async (slugs: string[]) => {
    const wanted = [...new Set(rowsRef.current.filter((r) => r.sellBook === null && slugs.includes(r.slug)).map((r) => r.slug))];
    if (wanted.length === 0) return;
    const books = new Map((await jobsMarketBooks(wanted)).map((b) => [b.slug, b.orders]));
    if (cancelled.current || books.size === 0) return;
    setRows((rs) => rs.map((r) => (r.sellBook === null && books.has(r.slug) ? { ...r, sellBook: rowBookOf(books.get(r.slug)!, r).sell } : r)));
  };
  // Opening asks for everything already loaded; after that each finished read is applied as the job reports it.
  const loadedKey = useJobs()
    .jobs.flatMap((j) => (j.kind.type === "marketLoad" ? j.items.filter((i) => i.status === "done").map((i) => i.id) : []))
    .join(",");
  useEffect(() => {
    const first = !booksProbed.current;
    booksProbed.current = true;
    void applyLoadedBooks(first || marketDone ? rowsRef.current.map((r) => r.slug) : loadedKey.split(","));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reacts to the job's progress only
  }, [loadedKey, marketDone]);

  const loadBooks = async () => {
    const pending = rows.filter((r) => r.selected && r.sellBook === null);
    if (busy || pending.length === 0) return;
    // An earlier load that ended is replaced by this one, not left beside it.
    for (const old of marketJobs) if (!isActive(old)) void jobsDismiss(old.id).catch(() => {});
    // The job takes 500 items; whatever is left stays without a book for the next click.
    const unique = [...new Map(pending.map((r) => [r.slug, { slug: r.slug, name: r.label }])).values()].slice(0, 500);
    try {
      await jobsStartMarketLoad(unique);
    } catch (e) {
      addToast({ level: "error", message: (e as Error).message });
    }
  };

  // The backend runs the prices one by one; progress, Stop and Retry live in the jobs pill.
  const apply = async () => {
    const sending = repriceRowsToSend(priced);
    if (busy || sending.length === 0) return;
    try {
      await jobsStartReprice(sending.map((r) => ({ id: r.order.id, platinum: r.nextPrice! })));
      addToast({ level: "success", message: tr("market.reprice.started", { count: sending.length }) });
      onClose();
    } catch (e) {
      addToast({ level: "error", message: (e as Error).message });
    }
  };

  const range = useRangeSelect(shown.map((r) => r.rowId));
  // Shift+click takes every listed row between the last click and this one.
  const toggle = (id: string, shift: boolean) => {
    const on = !rows.find((r) => r.rowId === id)?.selected;
    const ids = new Set(range(id, shift));
    setRows((rs) => rs.map((r) => (ids.has(r.rowId) ? { ...r, selected: on } : r)));
  };
  const inputCls = "w-20 rounded-md border border-border bg-surface-input px-2 py-1 text-sm text-text-primary outline-none focus:border-accent/70";

  return (
    <div className="detail-overlay" role="dialog" aria-modal="true" data-reprice-modal>
      <button className="detail-backdrop" aria-label={tr("common.close")} onClick={onClose} />
      <div className="detail-panel order-modal-panel flex max-h-[85vh] w-[min(52rem,95vw)] flex-col p-4">
        <h3 className="mb-3 mt-0 font-display text-xl font-bold">{tr("market.reprice.title")}</h3>
        <div className="mb-2 flex flex-wrap items-center gap-2">
          <label className="flex items-center gap-1.5 text-sm text-text-secondary">
            {tr("workbench.strategyLabel")}
            <select className={inputCls + " w-auto"} value={strategyId} disabled={busy} onChange={(e) => setStrategyId(e.target.value as StrategyId)}>
              {STRATEGIES.map((id) => (
                <option key={id} value={id}>
                  {tr(`workbench.strategy.${id}`)}
                </option>
              ))}
            </select>
          </label>
          {strategyId === "percent-offset" && (
            <label className="flex items-center gap-1.5 text-sm text-text-secondary">
              {tr("workbench.strategy.percentLabel")}
              <input className={inputCls} type="number" step={1} value={percent} disabled={busy} onChange={(e) => setPercent(Number(e.target.value))} />
            </label>
          )}
          {isMedianStrategy(strategyId) && (
            <label className="flex items-center gap-1.5 text-sm text-text-secondary">
              {tr("workbench.strategy.medianOffsetLabel")}
              <input className={inputCls} type="number" step={1} value={medianOffset} disabled={busy} onChange={(e) => setMedianOffset(Number(e.target.value))} />
            </label>
          )}
          <button className="btn-secondary btn-sm" disabled={busy} data-reprice-load onClick={() => void loadBooks()}>
            {tr("market.reprice.loadPrices")}
          </button>
          <span className="ml-auto flex items-center gap-1.5 text-sm text-text-secondary">
            {tr("market.reprice.showLabel")}
            <SegmentedControl
              value={filter}
              onChange={setFilter}
              options={[
                { value: "all", label: tr("common.all") },
                { value: "changed", label: tr("market.reprice.showChanged") },
                { value: "skipped", label: tr("market.reprice.showSkipped") },
              ]}
            />
          </span>
        </div>
        <p className="mb-2 mt-0 text-xs text-text-muted">{tr("market.reprice.counts", { shown: shown.length, selected: priced.filter((r) => r.selected).length, total: priced.length })}</p>
        <div className="min-h-0 flex-1 overflow-auto rounded-md border border-border">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-bg-surface text-left text-xs text-text-muted">
              <tr>
                <th className="w-8 px-2 py-1" />
                <th className="px-2 py-1">{tr("common.name")}</th>
                <th className="px-2 py-1 text-right">{tr("market.reprice.current")}</th>
                <th className="px-2 py-1 text-right">{tr("market.reprice.next")}</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((r) => (
                <tr key={r.rowId} className="border-t border-border" data-reprice-row={r.rowId}>
                  <td className="px-2 py-1">
                    <input type="checkbox" checked={r.selected} disabled={busy} aria-label={r.label} onChange={(e) => toggle(r.rowId, shiftOf(e))} />
                  </td>
                  <td className="px-2 py-1">
                    {r.label}
                    {r.rank != null && <span className="ml-1.5 text-xs text-text-muted">R{r.rank}</span>}
                  </td>
                  <td className="px-2 py-1 text-right">{r.currentPrice}p</td>
                  <td className="px-2 py-1 text-right">
                    {r.skipReason === null ? (
                      <span className={`font-bold ${r.nextPrice! > r.currentPrice ? "text-success" : "text-warning"}`}>{r.nextPrice}p</span>
                    ) : (
                      <span className="text-xs text-text-muted">{tr(SKIP_KEY[r.skipReason])}</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        {progress && (
          <p className="mb-0 mt-2 text-sm text-text-secondary" role="status">
            {tr("market.reprice.loading", { done: progress.done, total: progress.total })}
          </p>
        )}
        <div className="mt-3 flex items-center justify-end gap-2">
          <span className="mr-auto text-xs text-text-muted" data-reprice-summary>
            {tr("market.reprice.summary", { sending: totals.sending, raised: totals.raised, lowered: totals.lowered })}
          </span>
          <button className="btn-secondary" onClick={onClose}>
            {tr("common.close")}
          </button>
          <button className="btn-primary" disabled={busy || totals.sending === 0} data-reprice-apply onClick={() => void apply()}>
            {tr("market.reprice.apply", { count: totals.sending })}
          </button>
        </div>
      </div>
    </div>
  );
}
