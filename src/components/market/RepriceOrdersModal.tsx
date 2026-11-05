import { useEffect, useMemo, useRef, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { wfmOrderUpdate, wfmSessionStatus, type MyOrder } from "../../lib/ipc";
import { buildRepriceRows, priceRepriceRow, repriceRowsToSend, repriceTotals, runReprice, type RepriceRow, type RepriceSkipReason } from "../../lib/market/repriceOrders";
import { workbenchMedianLoader } from "../../lib/tradeWorkbench/medianReference";
import { DEFAULT_DAMPING_RULE, isMedianStrategy, type StrategyConfig, type WorkbenchStrategyId } from "../../lib/tradeWorkbench/pricingStrategies";
import { fetchItemOrderBookBySlug } from "../../lib/wfm/orderBook";
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
  /** Called when at least one price went out, so the list refetches. */
  onApplied: () => void;
  tr: Translator;
}

export default function RepriceOrdersModal({ orders, ownUserName, onClose, onApplied, tr }: Props) {
  const [rows, setRows] = useState<RepriceRow[]>(() => buildRepriceRows(orders));
  const [strategyId, setStrategyId] = useState<StrategyId>("cheapest-minus-one");
  const [percent, setPercent] = useState(-1);
  const [medianOffset, setMedianOffset] = useState(0);
  // Bumped when median references land: the loader keeps them outside React state.
  const [medianRev, setMedianRev] = useState(0);
  const [filter, setFilter] = useState<Filter>("all");
  const [progress, setProgress] = useState<{ phase: "loading" | "applying"; done: number; total: number } | null>(null);
  const [failed, setFailed] = useState<string[]>([]);
  const [stopped, setStopped] = useState(false);
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
    if (!isMedianStrategy(config.id) || booked === 0) return;
    const id = config.id;
    let stale = false;
    void workbenchMedianLoader.load(id, rows, () => stale || cancelled.current).then(() => !stale && setMedianRev((n) => n + 1));
    return () => void (stale = true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config.id, booked]);
  const totals = repriceTotals(priced);
  const shown = priced.filter((r) => filter === "all" || (filter === "changed" ? r.skipReason === null : r.skipReason !== null));
  const busy = progress !== null;

  const loadBooks = async () => {
    const pending = rows.filter((r) => r.selected && r.sellBook === null);
    if (busy || pending.length === 0) return;
    setProgress({ phase: "loading", done: 0, total: pending.length });
    let done = 0;
    for (const row of pending) {
      if (cancelled.current) return;
      const res = await fetchItemOrderBookBySlug(row.slug, { rank: row.rank, subtype: row.subtype });
      const book = res.status === "ok" ? res.data.sell : null;
      setRows((rs) => rs.map((r) => (r.rowId === row.rowId ? { ...r, sellBook: book } : r)));
      setProgress({ phase: "loading", done: ++done, total: pending.length });
    }
    setProgress(null);
  };

  const apply = async () => {
    const sending = repriceRowsToSend(priced);
    if (busy || sending.length === 0) return;
    setFailed([]);
    setStopped(false);
    setProgress({ phase: "applying", done: 0, total: sending.length });
    const result = await runReprice(sending, {
      updateOrder: (row, platinum) => wfmOrderUpdate(row.order.id, { platinum } as never),
      isCancelled: () => cancelled.current,
      isSignedOut: async () => !(await wfmSessionStatus()).logged_in,
      onProgress: (applied, bad) => setProgress({ phase: "applying", done: applied + bad.length, total: sending.length }),
    });
    // closed mid-run: the orders already sent still changed, so the table must refresh
    if (cancelled.current) return void (result.applied.length > 0 && onApplied());
    setProgress(null);
    setFailed(result.failed);
    setStopped(result.stopReason === "auth");
    if (result.applied.length > 0) onApplied();
    // applied rows now sit at their new price; drop them from the preview
    const done = new Set(result.applied.map((a) => a.id));
    setRows((rs) => rs.filter((r) => !done.has(r.rowId)));
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
            {tr(progress.phase === "loading" ? "market.reprice.loading" : "market.reprice.applying", { done: progress.done, total: progress.total })}
          </p>
        )}
        {failed.length > 0 && (
          <p className="mb-0 mt-2 text-sm text-danger" role="alert" title={failed.join(", ")}>
            {tr("market.reprice.failed", { count: failed.length })}
          </p>
        )}
        {stopped && (
          <p className="mb-0 mt-2 text-sm text-warning" role="alert">
            {tr("market.reprice.stoppedAuth")}
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
