import { askConfirm } from "../../stores/confirm";
import { openView } from "../../stores/nav";
import { useEffect, useMemo, useRef, useState } from "react";
import { WORKBENCH_MAX_ROWS_PER_RUN } from "../../../config/shared/tradeWorkbenchTypes";
import type { MessageKey, Translator } from "../../lib/i18n";
import { setRootOf } from "../../lib/inventory/fullSets";
import { SAFETY_REASON_KEYS } from "../../lib/inventory/safetyRules";
import { verdictResolver } from "../../lib/inventory/sellable";
import {
  jobsDismiss,
  jobsMarketBooks,
  jobsStartMarketLoad,
  jobsStop,
  workbenchAckOverride,
  workbenchCancelRun,
  workbenchExecutePlan,
  workbenchPreviewPlan,
  workbenchReconcile,
  workbenchResolveReview,
  wfmSessionStatus,
  type MyOrder,
  type WorkbenchPlanValidation as Validation,
  type WorkbenchReviewClassification,
  type WorkbenchReviewReport,
  type WorkbenchRowProgress,
  type WorkbenchStopReason,
} from "../../lib/ipc";
import { workbenchMedianLoader } from "../../lib/tradeWorkbench/medianReference";
import { DEFAULT_DAMPING_RULE, isMedianStrategy, WORKBENCH_STRATEGY_IDS, type StrategyConfig, type WorkbenchStrategyId } from "../../lib/tradeWorkbench/pricingStrategies";
import { markQueueMarketFetched, readCachedQueueRows, writeCachedQueueRows } from "../../lib/tradeWorkbench/queueCache";
import {
  acknowledgeRowOverride,
  applyStrategy,
  attachExistingOrders,
  attachMarketData,
  bindingReasonKeys,
  buildPlanFromRows,
  buildSelectedQueueRows,
  captureSafetySnapshot,
  filterQueueRows,
  mergeQueueRows,
  planTotals,
  rowNeedsOverride,
  rowSafetyKey,
  rowsNeedingMarketData,
  setRowQuantity,
  unpricedHiddenCount,
  unpricedSelectedRows,
  type QueueListedFilter,
  type WorkbenchQueueRow as QueueRow,
} from "../../lib/tradeWorkbench/queueModel";
import { rowBookOf } from "../../lib/tradeWorkbench/rowBook";
import { STRATEGY_KEYS } from "../../lib/tradeWorkbench/strategyLabels";
import type { CatalogIndex } from "../../lib/wfm/catalog";
import { useInventorySafety } from "../../stores/inventorySafety";
import { isActive, useJobs } from "../../stores/jobs";
import { loadMyOrders } from "../../stores/myOrders";
import { useWorkbenchState } from "../../stores/workbench";
import type { ItemDbEntry, ParsedItem, RawInventoryData } from "../../types/inventory";
import WorkbenchQueueRow from "./WorkbenchQueueRow";
import WorkbenchReview from "./WorkbenchReview";
import { useRangeSelect } from "../../lib/useRangeSelect";

interface Props {
  items: readonly ParsedItem[];
  /** Inventory selection keys (`selectionKeyFor`) the queue is built from. */
  selection: ReadonlySet<string>;
  inventory: RawInventoryData | null;
  itemDb: Record<string, ItemDbEntry>;
  catalog: CatalogIndex | null;
  locale: string;
  tr: Translator;
  onClose: () => void;
}

const PLAN_ERROR_KEYS: Record<NonNullable<Validation["planError"]>, MessageKey> = {
  empty: "workbench.planError.empty",
  "too-many-rows": "workbench.planError.too-many-rows",
  "duplicate-row-ids": "workbench.planError.duplicate-row-ids",
};
const ROW_ERROR_KEYS: Record<NonNullable<Validation["rows"][number]["reason"]>, MessageKey> = {
  "missing-safety": "workbench.rowError.missing-safety",
  "over-safe": "workbench.rowError.over-safe",
  "over-total": "workbench.rowError.over-total",
  "bad-quantity": "workbench.rowError.bad-quantity",
  "bad-price": "workbench.rowError.bad-price",
  "missing-order-id": "workbench.rowError.missing-order-id",
};
const STOP_REASON_KEYS: Record<WorkbenchStopReason, MessageKey> = {
  completed: "workbench.stopReason.completed",
  cancelled: "workbench.stopReason.cancelled",
  "order-limit": "workbench.stopReason.order-limit",
  auth: "workbench.stopReason.auth",
  error: "workbench.stopReason.error",
};
const ROW_STATUS_KEYS: Record<WorkbenchRowProgress["status"], MessageKey> = {
  pending: "workbench.rowStatus.pending",
  "in-flight": "workbench.rowStatus.in-flight",
  done: "workbench.rowStatus.done",
  failed: "workbench.rowStatus.failed",
  cancelled: "workbench.rowStatus.cancelled",
  blocked: "workbench.rowStatus.blocked",
};

// Rows named one by one in the confirm dialog; the backend truncates long messages.
const CONFIRM_ROW_PREVIEW = 8;
// Display only. Bulk ticks cover every filtered row, not this slice.
const QUEUE_DISPLAY_CAP = 100;

const FIELD = "rounded-md border border-border bg-surface-input px-2 py-1.5 text-sm text-text-primary outline-none focus:border-accent/70";
const LABEL = "flex flex-col gap-1 font-display text-xs uppercase tracking-[0.04em] text-text-muted";
const DEGRADED_KEYS: Record<"pinnedGoal" | "unmasteredRecipe", MessageKey> = {
  pinnedGoal: "workbench.safety.degraded.pinnedGoal",
  unmasteredRecipe: "workbench.safety.degraded.unmasteredRecipe",
};
const BANNER = "rounded-md border p-2.5 text-sm";

const numOrNull = (v: string): number | null => (v.trim() === "" || !Number.isFinite(Number(v)) ? null : Number(v));
const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e));

export default function BulkSellModal({ items, selection, inventory, itemDb, catalog, locale, tr, onClose }: Props) {
  const safety = useInventorySafety(inventory, itemDb);
  const ctx = safety.context;
  // Mods and arcanes keep one copy at item level, the same rule the inventory's sellable count uses.
  const verdictOf = useMemo(() => verdictResolver(items, ctx), [items, ctx]);
  const [wb, setWb] = useWorkbenchState();

  const [rows, setRows] = useState<QueueRow[]>(() => [...readCachedQueueRows()]);
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const [queueBuilt, setQueueBuilt] = useState(false);
  const [filter, setFilter] = useState("");
  const [minPlat, setMinPlat] = useState<number | null>(null);
  const [maxPlat, setMaxPlat] = useState<number | null>(null);
  const [listed, setListed] = useState<QueueListedFilter>("all");
  const ordersRef = useRef<MyOrder[]>([]);
  // False until a fetch of our own orders succeeded; every row would otherwise be planned as a create and duplicate what is listed.
  const [ordersReady, setOrdersReady] = useState(false);
  const [ordersBusy, setOrdersBusy] = useState(false);
  // null until the session answers: no sign-in or orders notice is shown on a guess.
  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  const [ownUser, setOwnUser] = useState<string | null>(null);
  // The market load is a background job: it goes on when this modal closes, and its state is read from the jobs store.
  const marketJobs = useJobs().jobs.filter((j) => j.kind.type === "marketLoad");
  const marketJob = marketJobs[marketJobs.length - 1] ?? null;
  const marketBusy = marketJob != null && isActive(marketJob);
  const marketProgress: [number, number] = marketJob ? [marketJob.done + marketJob.failed, marketJob.total] : [0, 0];
  const marketFailed = new Set(marketJob?.items.filter((i) => i.status === "failed").map((i) => i.id));
  const medianAfterLoad = useRef(false);
  const booksProbed = useRef(false);
  const [report, setReport] = useState<WorkbenchReviewReport | null>(null);
  const [reviewBusy, setReviewBusy] = useState(false);
  const [preview, setPreview] = useState<Validation | null>(null);
  const previewToken = useRef(0);
  const [error, setError] = useState<string | null>(null);
  const [legend, setLegend] = useState(false);

  const [strategyId, setStrategyId] = useState<WorkbenchStrategyId>("cheapest-minus-one");
  const [percent, setPercent] = useState(-5);
  const [avgCount, setAvgCount] = useState(5);
  const [avgThreshold, setAvgThreshold] = useState(10);
  const [marginCost, setMarginCost] = useState(0);
  const [marginPercent, setMarginPercent] = useState(20);
  const [medianOffset, setMedianOffset] = useState(0);
  const [medianLoads, setMedianLoads] = useState(0);
  const [dampBelow, setDampBelow] = useState(DEFAULT_DAMPING_RULE.minListingsBelow);
  const [dampPercent, setDampPercent] = useState(DEFAULT_DAMPING_RULE.maxDropPercent);
  const [dampPlat, setDampPlat] = useState(DEFAULT_DAMPING_RULE.maxDropPlat);
  // Per row the newest apply wins, so a slow median load never overwrites a later one.
  const applyTokens = useRef(new Map<string, number>());
  const applyCounter = useRef(0);

  const strategyConfig = (): StrategyConfig => {
    switch (strategyId) {
      case "percent-offset":
        return { id: "percent-offset", percent };
      case "bounded-cheapest-average":
        return { id: "bounded-cheapest-average", count: avgCount, thresholdPercent: avgThreshold };
      case "target-margin":
        return { id: "target-margin", costPlat: marginCost, marginPercent };
      case "manual":
        return { id: "manual" };
      case "match-cheapest":
        return { id: "match-cheapest" };
      case "median-48h":
      case "median-90d":
        return { id: strategyId, offsetPlat: medianOffset };
      default:
        return { id: "cheapest-minus-one" };
    }
  };
  const damping = { minListingsBelow: dampBelow, maxDropPercent: dampPercent, maxDropPlat: dampPlat };

  const queueFilter = { text: filter, plat: { min: minPlat, max: maxPlat }, listed };
  const filtered = useMemo(() => filterQueueRows(rows, { text: filter, plat: { min: minPlat, max: maxPlat }, listed }), [rows, filter, minPlat, maxPlat, listed]);
  const visible = filtered.slice(0, QUEUE_DISPLAY_CAP);
  const noPriceHidden = unpricedHiddenCount(rows, queueFilter);
  const selectedCount = rows.filter((r) => r.selected).length;
  const totals = planTotals(rows);
  const unpricedCount = unpricedSelectedRows(rows).length;
  const running = wb?.phase === "running" || wb?.phase === "cancelling";
  const reviewRequired = wb?.reviewRequired === true;
  const run = wb?.run ?? null;

  async function loadOwnOrders(): Promise<boolean> {
    setOrdersBusy(true);
    try {
      ordersRef.current = (await loadMyOrders()).sell;
      setOrdersReady(true);
      return true;
    } catch (e) {
      ordersRef.current = [];
      setOrdersReady(false);
      setError(messageOf(e));
      return false;
    } finally {
      setOrdersBusy(false);
    }
  }

  // Built once on open: the queue is the snapshot of what the user ticked, so a later inventory push must not drop rows being priced.
  useEffect(() => {
    let live = true;
    const restored = readCachedQueueRows();
    void (async () => {
      const session = await wfmSessionStatus().catch(() => null);
      if (!live) return;
      const signedIn = session?.logged_in === true;
      setLoggedIn(signedIn);
      setOwnUser(signedIn ? session.user_name : null);
      if (signedIn) await loadOwnOrders();
      if (!live) return;
      // The existing order is always re-read from the fresh account orders, so a carried row reports what is listed right now.
      const built = catalog ? buildSelectedQueueRows(items, ctx, catalog, selection, undefined, verdictOf).map((r) => attachMarketData(r, null, null, ordersRef.current)) : [];
      setRows(mergeQueueRows(restored, built));
      setQueueBuilt(true);
    })();
    return () => {
      live = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- opening snapshot by design
  }, []);

  // Survives a close/reopen while the app runs; held back until the queue exists so a failed open keeps the cache.
  useEffect(() => {
    if (queueBuilt) writeCachedQueueRows(rows);
  }, [rows, queueBuilt]);

  const range = useRangeSelect(visible.map((r) => r.rowId));
  // Shift+click takes every listed row between the last click and this one.
  const pickRow = (row: QueueRow, shift: boolean) => {
    const ids = new Set(range(row.rowId, shift));
    setRows((prev) => prev.map((r) => (ids.has(r.rowId) ? { ...r, selected: !row.selected } : r)));
  };
  const patchRow = (next: QueueRow) => setRows((prev) => prev.map((r) => (r.rowId === next.rowId ? next : r)));

  /** Recompute verdicts against the live safety settings, keeping edits. */
  const refreshSafety = () => setRows((prev) => prev.map((r) => setRowQuantity({ ...r, verdict: verdictOf(r.item) }, r.quantity)));
  // Settings edits rebuild the context; re-run the verdicts once it exists.
  const firstSafety = useRef(true);
  useEffect(() => {
    if (firstSafety.current) firstSafety.current = false;
    else refreshSafety();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ctx]);

  const claim = (ids: Iterable<string>) => {
    const token = ++applyCounter.current;
    for (const id of ids) applyTokens.current.set(id, token);
    return token;
  };

  async function applyConfig(config: StrategyConfig, ids: ReadonlySet<string>, cancelled?: () => boolean) {
    const token = claim(ids);
    if (isMedianStrategy(config.id)) {
      setMedianLoads((n) => n + 1);
      try {
        await workbenchMedianLoader.load(config.id, rowsRef.current.filter((r) => ids.has(r.rowId) && r.sellBook), cancelled);
      } finally {
        setMedianLoads((n) => n - 1);
      }
    }
    if (cancelled?.()) return;
    setRows((prev) => prev.map((r) => (ids.has(r.rowId) && applyTokens.current.get(r.rowId) === token ? applyStrategy(r, config, ownUser, damping, workbenchMedianLoader.peek(config.id, r)) : r)));
  }

  /** Applies the books the job already loaded to the rows still without one (the same path a live fetch took). */
  async function applyLoadedBooks(slugs: string[]) {
    const wanted = rowsRef.current.filter((r) => r.selected && !r.sellBook && slugs.includes(r.slug));
    if (wanted.length === 0) return;
    const books = new Map((await jobsMarketBooks([...new Set(wanted.map((r) => r.slug))])).map((b) => [b.slug, b.orders]));
    const config = strategyConfig();
    const ids = new Set<string>();
    rowsRef.current = rowsRef.current.map((current) => {
      const orders = books.get(current.slug);
      if (!orders || current.sellBook || !current.selected) return current;
      const book = rowBookOf(orders, current);
      claim([current.rowId]);
      ids.add(current.rowId);
      const next = attachMarketData(current, book.sell, book.buy, ordersRef.current);
      return applyStrategy(next, config, ownUser, damping, workbenchMedianLoader.peek(config.id, next));
    });
    if (ids.size === 0) return;
    markQueueMarketFetched();
    const byId = new Map(rowsRef.current.map((r) => [r.rowId, r]));
    setRows((prev) => prev.map((r) => (ids.has(r.rowId) ? (byId.get(r.rowId) ?? r) : r)));
  }

  // Opening asks for everything already loaded; after that each finished read is applied as the job reports it.
  const loadedSlugs = useJobs().jobs.flatMap((j) => (j.kind.type === "marketLoad" ? j.items.filter((i) => i.status === "done").map((i) => i.id) : []));
  const loadedKey = loadedSlugs.join(",");
  const marketDone = marketJob?.state === "done";
  useEffect(() => {
    if (!queueBuilt) return;
    const first = !booksProbed.current;
    booksProbed.current = true;
    void (async () => {
      await applyLoadedBooks(first || marketDone ? rowsRef.current.map((r) => r.slug) : loadedSlugs);
      if (marketDone && medianAfterLoad.current) {
        medianAfterLoad.current = false;
        const config = strategyConfig();
        if (isMedianStrategy(config.id)) await applyConfig(config, new Set(rowsRef.current.filter((r) => r.selected && r.sellBook).map((r) => r.rowId)));
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reacts to the job's progress only
  }, [queueBuilt, loadedKey, marketDone]);

  async function loadMarket() {
    if (marketBusy) return;
    const targets = rowsNeedingMarketData(rows);
    if (targets.length === 0) return;
    setError(null);
    // An earlier load that ended with failures is replaced by this one, not left beside it.
    for (const old of marketJobs) if (!marketBusy && !isActive(old)) void jobsDismiss(old.id).catch(() => {});
    // The job takes 500 items; whatever is left stays without a book for the next click.
    const unique = [...new Map(targets.map((t) => [t.slug, { slug: t.slug, name: t.itemName }])).values()].slice(0, 500);
    try {
      await jobsStartMarketLoad(unique);
      medianAfterLoad.current = true;
    } catch (e) {
      setError(messageOf(e));
    }
  }
  const failedNames = rows.filter((r) => !r.sellBook && marketFailed.has(r.slug)).map((r) => r.itemName);
  // Read fine, but nobody sells it: a different case from a read that failed.
  const noOrdersNames = rows.filter((r) => r.selected && r.sellBook && r.market?.lowestSell == null).map((r) => r.itemName);

  const selectFiltered = (next: (r: QueueRow) => boolean) => {
    const ids = new Set(filtered.map((r) => r.rowId));
    setRows((prev) => prev.map((r) => (ids.has(r.rowId) ? { ...r, selected: next(r) } : r)));
  };
  /** Clears hidden rows too, or a filtered-out tick would still be listed. */
  const clearSelection = () => setRows((prev) => prev.map((r) => (r.selected ? { ...r, selected: false } : r)));

  function acknowledge(row: QueueRow) {
    if (row.overrideAcknowledged) return patchRow({ ...row, overrideAcknowledged: false, overrideAcknowledgedAt: null });
    const at = Date.now();
    const next = acknowledgeRowOverride(row, at);
    patchRow(next);
    if (!rowNeedsOverride(next)) return;
    // Audit trail: the acknowledgement is journaled by the backend too.
    void workbenchAckOverride({ planId: "queue", rowId: next.rowId, itemName: next.itemName, safeQuantity: next.verdict.safe, requestedQuantity: next.quantity, reasonKeys: bindingReasonKeys(next.verdict), acknowledgedAt: at }).catch(() => {});
  }

  const snapshotFor = (planRowIds: Set<string>, now: number) => captureSafetySnapshot(rows.filter((r) => planRowIds.has(r.rowId)), ctx, now, verdictOf);

  function previewPlan() {
    setError(null);
    setPreview(null);
    const token = ++previewToken.current;
    const now = Date.now();
    const { plan, overCap } = buildPlanFromRows(rows, now, ordersRef.current);
    if (overCap) return setError(tr("workbench.error.overCap", { cap: WORKBENCH_MAX_ROWS_PER_RUN }));
    workbenchPreviewPlan(plan, snapshotFor(new Set(plan.rows.map((r) => r.rowId)), now)).then(
      // Two clicks can resolve out of order, so only the newest preview writes.
      (v) => token === previewToken.current && setPreview(v),
      (e) => token === previewToken.current && setError(messageOf(e)),
    );
  }

  async function executePlan() {
    setError(null);
    const now = Date.now();
    const { plan, overCap } = buildPlanFromRows(rows, now, ordersRef.current);
    if (plan.rows.length === 0 || overCap) return setError(overCap ? tr("workbench.error.overCap", { cap: WORKBENCH_MAX_ROWS_PER_RUN }) : tr("workbench.error.emptyPlan"));
    const prices = plan.rows.map((r) => r.platinum);
    const message = [
      tr("workbench.execute.confirm", { rows: plan.rows.length, units: plan.rows.reduce((s, r) => s + r.quantity, 0) }),
      // Prices are what a mispriced plan is spotted by, so they belong in the last dialog before anything is listed.
      tr("workbench.execute.confirmPricing", { platinum: plan.rows.reduce((s, r) => s + r.quantity * r.platinum, 0), min: Math.min(...prices), max: Math.max(...prices) }),
      ...plan.rows.slice(0, CONFIRM_ROW_PREVIEW).map((r) => tr("workbench.execute.confirmRow", { item: r.itemName, units: r.quantity, price: r.platinum })),
      ...(plan.rows.length > CONFIRM_ROW_PREVIEW ? [tr("workbench.execute.confirmMoreRows", { count: plan.rows.length - CONFIRM_ROW_PREVIEW })] : []),
    ].join("\n");
    if (!(await askConfirm(message))) return;
    // Fresh snapshot at confirm time: the safety engine has the final word.
    const result = await workbenchExecutePlan(plan, snapshotFor(new Set(plan.rows.map((r) => r.rowId)), Date.now()));
    setWb(result.state);
    if (!result.started) {
      setError(result.error ?? tr("workbench.error.notStarted"));
      if (result.validation) setPreview(result.validation);
    }
  }

  async function guarded<T>(job: () => Promise<T>): Promise<T | null> {
    setReviewBusy(true);
    try {
      return await job();
    } catch (e) {
      setError(messageOf(e));
      return null;
    } finally {
      setReviewBusy(false);
    }
  }
  const reconcile = async () => setReport((await guarded(workbenchReconcile)) ?? report);
  const resolveReview = async (resolutions: Array<{ intentId: string; classification: WorkbenchReviewClassification }>) => {
    const state = await guarded(() => workbenchResolveReview(resolutions));
    if (state) {
      setWb(state);
      setReport(null);
    }
  };
  const resetJournal = async () => {
    if (!(await askConfirm(tr("workbench.review.resetConfirm")))) return;
    const state = await guarded(() => workbenchResolveReview([], true));
    if (state) setWb(state);
  };

  // After a run finishes, re-read the account orders so rows stop claiming they are unlisted.
  const watched = useRef<string | null>(null);
  const refreshed = useRef<string | null>(null);
  useEffect(() => {
    if (!run) return;
    if (run.finishedAt == null) {
      watched.current = run.planId;
      return;
    }
    if (running || loggedIn !== true || watched.current !== run.planId || refreshed.current === run.planId) return;
    refreshed.current = run.planId;
    void loadOwnOrders().then((ok) => ok && setRows((prev) => attachExistingOrders(prev, ordersRef.current)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, running, loggedIn]);

  const doneCount = run?.rows.filter((r) => r.status === "done").length ?? 0;
  const resetSafetySettings = async () => (await askConfirm(tr("workbench.safety.resetConfirm"))) && safety.reset();
  const retryOrders = async () => {
    setError(null);
    if (await loadOwnOrders()) setRows((prev) => attachExistingOrders(prev, ordersRef.current));
  };

  return (
    <div className="detail-overlay" role="dialog" aria-modal="true" aria-label={tr("workbench.title")}>
      <button className="detail-backdrop" aria-label={tr("common.close")} onClick={onClose} />
      <div className="detail-panel flex max-h-[88vh] w-[1180px] max-w-[95vw] flex-col overflow-hidden" data-bulk-sell-modal>
        <header className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
          <h2 className="m-0 font-display text-xl font-bold text-text-primary">{tr("workbench.title")}</h2>
          <div className="flex items-center gap-3">
            <label className="flex items-center gap-2 text-xs text-text-muted">
              {tr("workbench.safety.spareDefault")}
              <input
                key={safety.settings.spareDefault}
                className={`${FIELD} w-16 text-right`}
                type="number"
                min={0}
                defaultValue={safety.settings.spareDefault}
                onBlur={(e) => {
                  const n = Number(e.currentTarget.value);
                  if (Number.isFinite(n) && n >= 0) safety.setSpareDefault(Math.floor(n));
                }}
              />
            </label>
            <button type="button" className="btn-secondary btn-sm" onClick={resetSafetySettings}>
              {tr("workbench.safety.reset")}
            </button>
            <button type="button" className="btn-secondary btn-sm" aria-pressed={legend} onClick={() => setLegend(!legend)}>
              {tr("workbench.safety.legend")}
            </button>
            <button type="button" className="detail-close" aria-label={tr("common.close")} onClick={onClose}>
              &times;
            </button>
          </div>
        </header>

        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 py-3">
          {legend && (
            // Own scroll box: the legend is a reference list and must not push the queue out of the modal body.
            <section className="shrink-0 rounded-md border border-border bg-surface-card p-3" data-workbench-legend>
              <h3 className="m-0 mb-1.5 font-display text-xs font-bold uppercase tracking-[0.06em] text-text-secondary">{tr("workbench.safety.legend")}</h3>
              <ul className="m-0 grid max-h-40 gap-1 overflow-y-auto pr-1 text-xs text-text-secondary">
                {SAFETY_REASON_KEYS.map((key) => (
                  <li key={key}>{tr(key as MessageKey, { count: 1 })}</li>
                ))}
              </ul>
            </section>
          )}

          {loggedIn === false && (
            <div className={`${BANNER} flex flex-wrap items-center gap-3 border-border bg-surface-card text-text-secondary`} data-workbench-signin-hint>
              <span>{tr("workbench.signInHint")}</span>
              <button type="button" className="btn-secondary btn-sm" data-workbench-signin onClick={() => (onClose(), openView("market"))}>
                {tr("workbench.signIn")}
              </button>
            </div>
          )}
          {loggedIn === true && !ordersReady && !ordersBusy && (
            <div className={`${BANNER} flex flex-wrap items-center gap-3 border-danger/40 bg-danger/10 text-danger`} data-workbench-orders-error>
              <span>{tr("workbench.ordersFetchFailed")}</span>
              <button type="button" className="btn-secondary btn-sm" disabled={ordersBusy} data-workbench-orders-retry onClick={() => void retryOrders()}>
                {tr("common.retry")}
              </button>
            </div>
          )}
          {safety.masteryReady && ctx.degradedRules.length > 0 && (
            <div className={`${BANNER} border-warning/40 bg-warning/10 text-warning`} data-workbench-degraded>
              <div>{tr("workbench.safety.degradedRules")}</div>
              <ul className="m-0 mt-1 list-disc pl-5 text-xs">
                {ctx.degradedRules.map((rule) => (
                  <li key={rule}>{tr(DEGRADED_KEYS[rule as keyof typeof DEGRADED_KEYS])}</li>
                ))}
              </ul>
            </div>
          )}
          {error && (
            <div className={`${BANNER} border-danger/40 bg-danger/10 text-danger`} data-workbench-error>
              {error}
            </div>
          )}
          {failedNames.length > 0 && (
            <div className={`${BANNER} border-warning/40 bg-warning/10 text-warning`} data-workbench-price-failed={failedNames.length}>
              <div>{tr("workbench.marketFailed", { count: failedNames.length })}</div>
              <div className="mt-1 max-h-16 overflow-y-auto text-xs">{failedNames.join(", ")}</div>
              <button type="button" className="btn-secondary btn-sm mt-2" disabled={marketBusy} data-workbench-price-retry onClick={() => void loadMarket()}>
                {tr("common.retry")}
              </button>
            </div>
          )}
          {noOrdersNames.length > 0 && (
            <div className={`${BANNER} border-warning/40 bg-warning/10 text-warning`} data-workbench-no-orders={noOrdersNames.length}>
              <div>{tr("workbench.marketNoOrders", { count: noOrdersNames.length })}</div>
              <div className="mt-1 max-h-16 overflow-y-auto text-xs">{noOrdersNames.join(", ")}</div>
            </div>
          )}

          {wb && (reviewRequired || report) && (
            <WorkbenchReview state={wb} report={report} busy={reviewBusy} tr={tr} onReconcile={() => void reconcile()} onResolve={(r) => void resolveReview(r)} onResetCorruptJournal={() => void resetJournal()} />
          )}

          <div className="flex flex-wrap items-center gap-3">
            <input className={`${FIELD} w-56`} placeholder={tr("workbench.filterPlaceholder")} aria-label={tr("workbench.filterPlaceholder")} data-workbench-filter value={filter} onChange={(e) => setFilter(e.target.value)} />
            <input className={`${FIELD} w-24 text-right`} type="number" min={0} placeholder={tr("workbench.filter.minPlat")} aria-label={tr("workbench.filter.minPlat")} data-workbench-min-plat value={minPlat ?? ""} onChange={(e) => setMinPlat(numOrNull(e.target.value))} />
            <input className={`${FIELD} w-24 text-right`} type="number" min={0} placeholder={tr("workbench.filter.maxPlat")} aria-label={tr("workbench.filter.maxPlat")} data-workbench-max-plat value={maxPlat ?? ""} onChange={(e) => setMaxPlat(numOrNull(e.target.value))} />
            <select className={`${FIELD} w-44`} aria-label={tr("workbench.filter.listedLabel")} data-workbench-listed value={listed} onChange={(e) => setListed(e.target.value as QueueListedFilter)}>
              <option value="all">{tr("workbench.filter.listedAll")}</option>
              <option value="unlisted">{tr("workbench.filter.listedUnlisted")}</option>
              <option value="listed">{tr("workbench.filter.listedListed")}</option>
            </select>
            {noPriceHidden > 0 && (
              <span className="text-xs text-warning" data-workbench-no-price-hidden={noPriceHidden}>
                {tr("workbench.filter.noPriceHidden", { count: noPriceHidden })}
              </span>
            )}
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <button type="button" className="btn-secondary btn-sm" disabled={filtered.length === 0} data-workbench-select-all onClick={() => selectFiltered(() => true)}>
              {tr("common.selectMatching", { count: filtered.length })}
            </button>
            <button type="button" className="btn-secondary btn-sm" disabled={selectedCount === 0} data-workbench-select-none onClick={clearSelection}>
              {tr("common.selectNoneCount", { count: selectedCount })}
            </button>
            <button type="button" className="btn-secondary btn-sm" disabled={filtered.length === 0} data-workbench-select-invert onClick={() => selectFiltered((r) => !r.selected)}>
              {tr("common.selectInvert", { count: filtered.length })}
            </button>
          </div>

          <div className="flex flex-wrap items-center gap-3">
            <button type="button" className="btn-secondary" disabled={marketBusy} data-workbench-load-market onClick={() => void loadMarket()}>
              {tr(marketBusy ? "workbench.loadingMarket" : "workbench.loadMarket")}
            </button>
            {marketProgress[1] > 0 && (
              <span className="text-xs text-text-muted" data-workbench-price-progress={`${marketProgress[0]}/${marketProgress[1]}`}>
                {tr("workbench.marketProgress", { done: marketProgress[0], total: marketProgress[1] })}
              </span>
            )}
            {marketBusy && (
              <button type="button" className="btn-secondary btn-sm" data-workbench-price-stop onClick={() => marketJob && void jobsStop(marketJob.id).catch((e: unknown) => setError(messageOf(e)))}>
                {tr("workbench.stopLoadingMarket")}
              </button>
            )}
            <button type="button" className="btn-secondary" onClick={refreshSafety}>
              {tr("workbench.refreshSafety")}
            </button>
          </div>

          <div className="grid gap-3 md:grid-cols-2" data-workbench-strategy>
            <fieldset className="flex flex-wrap items-end gap-3 rounded-md border border-border p-3">
              <legend className="px-1 font-display text-xs uppercase tracking-[0.06em] text-text-secondary">{tr("workbench.section.pricing")}</legend>
              <label className={LABEL}>
                {tr("workbench.strategyLabel")}
                <select className={`${FIELD} w-52`} value={strategyId} onChange={(e) => setStrategyId(e.target.value as WorkbenchStrategyId)}>
                  {WORKBENCH_STRATEGY_IDS.map((id) => (
                    <option key={id} value={id}>
                      {tr(STRATEGY_KEYS[id])}
                    </option>
                  ))}
                </select>
              </label>
              {strategyId === "percent-offset" && <NumField label={tr("workbench.strategy.percentLabel")} value={percent} onChange={setPercent} />}
              {strategyId === "bounded-cheapest-average" && (
                <>
                  <NumField label={tr("workbench.strategy.countLabel")} value={avgCount} min={1} onChange={setAvgCount} />
                  <NumField label={tr("workbench.strategy.thresholdLabel")} value={avgThreshold} min={0} onChange={setAvgThreshold} />
                </>
              )}
              {strategyId === "target-margin" && (
                <>
                  <NumField label={tr("workbench.strategy.costLabel")} value={marginCost} min={0} onChange={setMarginCost} />
                  <NumField label={tr("workbench.strategy.marginLabel")} value={marginPercent} onChange={setMarginPercent} />
                </>
              )}
              {isMedianStrategy(strategyId) && <NumField label={tr("workbench.strategy.medianOffsetLabel")} value={medianOffset} onChange={setMedianOffset} />}
              <button type="button" className="btn-secondary" disabled={medianLoads > 0} data-workbench-apply-strategy onClick={() => void applyConfig(strategyConfig(), new Set(rows.filter((r) => r.selected).map((r) => r.rowId)))}>
                {tr("workbench.applyStrategy")}
              </button>
            </fieldset>

            <fieldset className="flex flex-wrap items-end gap-3 rounded-md border border-border p-3">
              <legend className="px-1 font-display text-xs uppercase tracking-[0.06em] text-text-secondary">{tr("workbench.damping.title")}</legend>
              <NumField label={tr("workbench.damping.minBelow")} value={dampBelow} min={0} onChange={setDampBelow} />
              <NumField label={tr("workbench.damping.maxDropPercent")} value={dampPercent} min={1} onChange={setDampPercent} />
              <NumField label={tr("workbench.damping.maxDropPlat")} value={dampPlat} min={1} onChange={setDampPlat} />
            </fieldset>
          </div>

          {rows.length === 0 ? (
            <p className="py-6 text-center text-sm text-text-muted" data-workbench-empty>
              {tr("workbench.queueEmpty")}
            </p>
          ) : (
            <>
              <div className="text-xs text-text-muted">{tr("workbench.queueSummary", { total: rows.length, shown: visible.length, selected: selectedCount })}</div>
              <div className="grid gap-1" data-workbench-queue>
                {visible.map((row) => {
                  const key = rowSafetyKey(row);
                  const root = setRootOf(row.item.internalName);
                  return (
                    <WorkbenchQueueRow
                      key={row.rowId}
                      row={row}
                      canSetKeep={row.item.inventoryGroup === "full_sets"}
                      isLocked={safety.settings.locks.includes(key)}
                      isSetKept={safety.settings.setKeep.includes(root)}
                      spareOverride={safety.settings.spares[key] ?? null}
                      tr={tr}
                      onToggleSelect={pickRow}
                      onQuantity={(r, q) => patchRow(setRowQuantity(r, q))}
                      onManualPrice={(r, p) => patchRow({ ...r, manualPrice: p != null && p >= 1 ? Math.round(p) : null })}
                      onOverride={acknowledge}
                      onToggleLock={() => safety.toggleLock(key)}
                      onToggleSetKeep={() => safety.toggleSetKeep(root)}
                      onSpare={(_r, s) => safety.setItemSpare(key, s != null && s >= 0 ? Math.floor(s) : null)}
                    />
                  );
                })}
              </div>
            </>
          )}

          {preview && preview.rows.some((r) => !r.ok) && (
            <div className="rounded-md border border-border bg-surface-card p-2.5 text-xs" data-workbench-preview>
              {preview.rows
                .filter((r) => !r.ok)
                .map((r) => (
                  <div key={r.rowId} className="text-danger">
                    {rows.find((q) => q.rowId === r.rowId)?.itemName ?? r.rowId}: {tr(ROW_ERROR_KEYS[r.reason ?? "bad-quantity"])}
                  </div>
                ))}
            </div>
          )}

          {run && (
            <div className="rounded-md border border-border bg-surface-card p-2.5 text-xs" data-workbench-progress data-workbench-run-state={run.finishedAt != null ? "finished" : "running"}>
              <div className="mb-1 font-semibold">
                {run.finishedAt != null && <span className="text-text-muted">{tr("workbench.lastRunAt", { at: new Date(run.finishedAt).toLocaleString(locale) })}, </span>}
                {tr("workbench.runProgress", { done: doneCount, total: run.rows.length })}
                {run.stopReason && <>, {tr(STOP_REASON_KEYS[run.stopReason])}</>}
              </div>
              {run.rows.map((row) => (
                <div key={row.rowId} className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate">{row.itemName}</span>
                  <span className={row.status === "done" ? "text-success" : row.status === "failed" || row.status === "blocked" ? "text-danger" : "text-text-muted"}>{tr(ROW_STATUS_KEYS[row.status])}</span>
                  {row.error && (
                    <span className="truncate text-danger" title={row.error}>
                      {row.error}
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        <footer className="flex flex-wrap items-center gap-3 border-t border-border px-4 py-3" data-workbench-footer>
          <span className="text-sm text-text-secondary">{tr("workbench.totals", { rows: totals.rows, units: totals.units, platinum: totals.platinum })}</span>
          {totals.rows > WORKBENCH_MAX_ROWS_PER_RUN && <span className="text-sm text-warning">{tr("workbench.error.overCap", { cap: WORKBENCH_MAX_ROWS_PER_RUN })}</span>}
          {unpricedCount > 0 && (
            <span className="text-sm text-danger" data-workbench-unpriced>
              {tr("workbench.error.unpricedRows", { count: unpricedCount })}
            </span>
          )}
          {preview && (
            <span className={preview.ok ? "text-sm text-text-secondary" : "text-sm text-danger"} data-workbench-preview-summary={preview.ok ? "ok" : "invalid"}>
              {tr("workbench.previewResult", { units: preview.totalUnits, platinum: preview.totalPlatinum ?? 0 })} {!preview.ok && (preview.planError ? tr(PLAN_ERROR_KEYS[preview.planError]) : tr("workbench.previewInvalid"))}
            </span>
          )}
          <div className="ml-auto flex items-center gap-2">
            <button type="button" className="btn-secondary" data-workbench-preview-btn onClick={previewPlan}>
              {tr("workbench.preview")}
            </button>
            {running && (
              <button type="button" className="btn-danger" data-workbench-cancel onClick={() => void workbenchCancelRun().then(setWb)}>
                {tr(wb?.phase === "cancelling" ? "workbench.cancelling" : "workbench.cancel")}
              </button>
            )}
            <button
              type="button"
              className="btn-primary px-6 py-2.5 text-base"
              disabled={loggedIn !== true || !ordersReady || ordersBusy || running || medianLoads > 0 || reviewRequired || unpricedCount > 0 || totals.rows === 0}
              title={loggedIn === false ? tr("workbench.signInHint") : undefined}
              data-workbench-execute
              onClick={() => void executePlan()}
            >
              {tr("workbench.execute")}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}

function NumField({ label, value, min, onChange }: { label: string; value: number; min?: number; onChange: (n: number) => void }) {
  return (
    <label className={LABEL}>
      {label}
      <input className={`${FIELD} w-20`} type="number" min={min} value={value} onChange={(e) => Number.isFinite(e.target.valueAsNumber) && onChange(e.target.valueAsNumber)} />
    </label>
  );
}
