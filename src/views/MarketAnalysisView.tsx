import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createViewState } from "../stores/viewState";
import { gdprApply, gdprPreviewFile, ledgerEdit, ledgerGetRange, ledgerListBatches, ledgerUndoBatch, type ItemDatabase, type LedgerBatch, type LedgerImportPreview, type LedgerPage, type LedgerPatch, type LedgerQuery } from "../lib/ipc";
import type { MessageKey, Translator } from "../lib/i18n";
import { pageLedgerRange } from "../lib/stats/ledgerPaging";
import {
  LEDGER_INPUT_DEBOUNCE_MS,
  bestSeller,
  computeFlow,
  fifoCostBasis,
  loadCategoryOverrides,
  makeItemKindResolver,
  monthlyFlow,
  partnerRollup,
  recentDailyFlow,
  resolveRangePreset,
  todayFlow,
  topItems,
  typeRollup,
  withCategoryOverrides,
  yearComparison,
  type DateRange,
  type RangePreset,
} from "../lib/stats/tradeAnalytics";
import { askConfirm } from "../stores/confirm";
import { useWfmCatalog } from "../stores/market";
import type { ItemDbLookup, TradeEvent, TradeType, WfmItemsLookup } from "../types/stats";
import { DateRangeBar, Partners, Summary, TopItems, TypePanel, YearCompare } from "../components/analysis/panels";
import { MonthlyChart, RecentDays } from "../components/analysis/charts";
import { ImportDialog, LedgerTable, MatchModal, RowEditor } from "../components/analysis/ledger";
import { BarChart3 } from "lucide-react";
import { Button, Card, EmptyState, PageHeader, Skeleton } from "../components/ui";

const TABLE_PAGE_SIZE = 50;
// Analytics read the whole range through the paged query; the cap keeps a huge archive from stalling the view.
const ANALYTICS_MAX_ROWS = 6000;

// The backend names the failure, the view owns the wording.
const ERROR_KEYS: Record<string, MessageKey> = {
  fileTooLarge: "analysis.err.fileTooLarge",
  importReadFailed: "analysis.err.importRead",
  noRows: "analysis.err.noRows",
  batchGone: "analysis.err.batchGone",
  importWriteFailed: "analysis.err.importWrite",
  rowGone: "analysis.err.rowGone",
  invalidPatch: "analysis.err.invalidPatch",
  saveFailed: "analysis.saveFailed",
  ledgerReadFailed: "analysis.loadFailed",
};
const errorKey = (e: unknown, fallback: MessageKey): MessageKey => ERROR_KEYS[e instanceof Error ? e.message : ""] ?? fallback;

interface Status {
  text: string;
  tone: "ok" | "error";
}

/** The view comes back as the user left it (memory only). */
const analysis = createViewState({
  preset: "ytd" as RangePreset,
  range: resolveRangePreset("ytd") as DateRange,
  offset: 0,
  search: "",
  typeFilter: "all" as TradeType | "all",
});

export default function MarketAnalysisView({ tr, locale, items }: { tr: Translator; locale: string; items: ItemDatabase }) {
  const [preset, setPreset] = analysis.useField("preset");
  const [range, setRange] = analysis.useField("range");
  const [events, setEvents] = useState<TradeEvent[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [failed, setFailed] = useState(false);
  const [comparison, setComparison] = useState<{ events: TradeEvent[]; total: number } | null>(null);
  const [comparisonFailed, setComparisonFailed] = useState(false);
  const [page, setPage] = useState<LedgerPage | null>(null);
  const [tableLoading, setTableLoading] = useState(true);
  const [offset, setOffset] = analysis.useField("offset");
  const [search, setSearch] = analysis.useField("search");
  const [debouncedSearch, setDebouncedSearch] = useState(() => search.trim());
  const [typeFilter, setTypeFilter] = analysis.useField("typeFilter");
  const [editing, setEditing] = useState<TradeEvent | null>(null);
  const [matching, setMatching] = useState<TradeEvent | null>(null);
  const [saving, setSaving] = useState(false);
  const [editError, setEditError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [preview, setPreview] = useState<LedgerImportPreview | null>(null);
  const [importBusy, setImportBusy] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [batches, setBatches] = useState<LedgerBatch[]>([]);
  const [reload, setReload] = useState(0);
  const [reloadComparison, setReloadComparison] = useState(0);
  const { catalog } = useWfmCatalog();

  // A newer load of the same kind (or unmount) retires the older one's result.
  const tokens = useRef({ analytics: 0, comparison: 0, table: 0, batches: 0 });
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true; // StrictMode re-runs effects after its simulated unmount
    return () => void (alive.current = false);
  }, []);
  const start = (kind: keyof typeof tokens.current) => {
    const token = (tokens.current[kind] += 1);
    return () => alive.current && token === tokens.current[kind];
  };

  const baseQuery = useMemo((): LedgerQuery => ({ from: range.from ?? null, to: range.to ?? null }), [range]);

  useEffect(() => {
    const current = start("analytics");
    setLoading(true);
    setFailed(false);
    pageLedgerRange(baseQuery, ANALYTICS_MAX_ROWS, current)
      .then((loaded) => {
        if (!loaded) return;
        setEvents(loaded.events);
        setTotal(loaded.total);
      })
      .catch(() => current() && (setEvents([]), setTotal(0), setFailed(true)))
      .finally(() => current() && setLoading(false));
  }, [baseQuery, reload]);

  // Year over year owns its query and each year its own bounds, or the newest-first ceiling would spend every row on this year.
  useEffect(() => {
    const current = start("comparison");
    const year = new Date().getFullYear();
    setComparisonFailed(false);
    (async () => {
      const previous = await pageLedgerRange({ from: `${year - 1}-01-01`, to: `${year - 1}-12-31` }, ANALYTICS_MAX_ROWS, current);
      if (!previous) return;
      const now = await pageLedgerRange({ from: `${year}-01-01`, to: `${year}-12-31` }, ANALYTICS_MAX_ROWS, current);
      if (!now) return;
      setComparison({ events: [...previous.events, ...now.events], total: previous.total + now.total });
    })().catch(() => current() && (setComparison(null), setComparisonFailed(true)));
  }, [reload, reloadComparison]);

  useEffect(() => {
    const t = setTimeout(() => setDebouncedSearch(search.trim()), LEDGER_INPUT_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [search]);

  useEffect(() => {
    const current = start("table");
    setTableLoading(true);
    ledgerGetRange({ ...baseQuery, offset, limit: TABLE_PAGE_SIZE, type: typeFilter === "all" ? null : typeFilter, text: debouncedSearch || null })
      .then((next) => {
        if (!current()) return;
        // a kept offset can be past the end after the ledger shrank
        if (next.total > 0 && next.events.length === 0 && offset > 0) return setOffset(Math.floor((next.total - 1) / TABLE_PAGE_SIZE) * TABLE_PAGE_SIZE);
        setPage(next);
      })
      .catch(() => current() && (setPage(null), setFailed(true)))
      .finally(() => current() && setTableLoading(false));
  }, [baseQuery, offset, typeFilter, debouncedSearch, reload]);

  useEffect(() => {
    const current = start("batches");
    ledgerListBatches()
      .then((b) => current() && setBatches(b))
      .catch(() => current() && setBatches([]));
  }, [reload]);

  const refresh = useCallback(() => {
    setReload((n) => n + 1);
  }, []);

  const wfmLookup = useMemo((): WfmItemsLookup => {
    const lookup: WfmItemsLookup = {};
    for (const it of catalog?.items ?? []) lookup[it.name.toLowerCase()] = { url_name: it.slug, item_name: it.name, thumb: it.thumb, icon: it.icon, maxRank: it.maxRank, gameRef: it.gameRef };
    return lookup;
  }, [catalog]);
  const resolveKind = useMemo(
    () => withCategoryOverrides(makeItemKindResolver(items as unknown as ItemDbLookup, wfmLookup), loadCategoryOverrides()),
    [items, wfmLookup],
  );

  const flow = useMemo(() => computeFlow(events), [events]);
  const basis = useMemo(() => fifoCostBasis(events), [events]);
  const best = useMemo(() => bestSeller(events), [events]);
  const types = useMemo(() => typeRollup(events, resolveKind), [events, resolveKind]);
  const partners = useMemo(() => partnerRollup(events), [events]);
  const months = useMemo(() => monthlyFlow(events, 24, { ...(range.from ? { from: range.from } : {}), ...(range.to ? { to: range.to } : {}) }), [events, range]);
  // "Today" and the last ten days must not follow the range picker, so they read the year-comparison load.
  const cmpEvents = comparison?.events ?? [];
  const recent = useMemo(() => recentDailyFlow(cmpEvents, 10), [cmpEvents]);
  const today = useMemo(() => todayFlow(cmpEvents), [cmpEvents]);
  const yearly = useMemo(() => yearComparison(cmpEvents), [cmpEvents]);

  const filtersActive = search.trim() !== "" || typeFilter !== "all";
  const hasData = events.length > 0 || (page?.total ?? 0) > 0;
  const capped = total > events.length;
  const comparisonCapped = comparison != null && comparison.total > comparison.events.length;

  const saveEdit = async (patch: LedgerPatch) => {
    if (!editing) return;
    if (Object.keys(patch).length === 0) return setEditing(null);
    setSaving(true);
    setEditError(null);
    try {
      await ledgerEdit(editing.id, patch);
      setEditing(null);
      refresh();
      setReloadComparison((n) => n + 1);
    } catch (e) {
      setEditError(tr(errorKey(e, "analysis.saveFailed")));
    } finally {
      setSaving(false);
    }
  };

  const previewImport = async (path: string) => {
    setImportBusy(true);
    setImportError(null);
    try {
      setPreview(await gdprPreviewFile(path));
    } catch (e) {
      setImportError(tr(errorKey(e, "analysis.importUnavailable")));
    } finally {
      setImportBusy(false);
    }
  };

  const applyImport = async () => {
    if (!preview) return;
    setImportBusy(true);
    setImportError(null);
    try {
      const result = await gdprApply(preview.batchId);
      // The preview already dropped its duplicates; both counts were skipped.
      const skipped = preview.counts.duplicates + result.skippedDuplicates;
      setStatus({ text: tr("analysis.importApplied", { applied: result.applied, skipped }), tone: "ok" });
      setPreview(null);
      setImporting(false);
      refresh();
    } catch (e) {
      setImportError(tr(errorKey(e, "analysis.importUnavailable")));
    } finally {
      setImportBusy(false);
    }
  };

  const undoImport = async (batchId: string) => {
    if (!(await askConfirm(tr("analysis.undoConfirm")))) return;
    try {
      const removed = await ledgerUndoBatch(batchId);
      setStatus({ text: tr("analysis.undoDone", { count: removed }), tone: "ok" });
      refresh();
    } catch (e) {
      setStatus({ text: tr(errorKey(e, "analysis.saveFailed")), tone: "error" });
    }
  };

  const day = (iso: string) => {
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleDateString(locale, { year: "numeric", month: "short", day: "numeric" });
  };

  const closeImport = () => {
    setImporting(false);
    setPreview(null);
    setImportError(null);
  };

  const gap = "grid grid-cols-1 gap-4 @5xl:grid-cols-2";
  return (
    <section className="@container flex flex-col gap-4" data-analysis-view>
      <PageHeader
        title={tr("analysis.title")}
        actions={
          <span data-analysis-import>
            <Button size="sm" onClick={() => setImporting(true)}>{tr("analysis.import")}</Button>
          </span>
        }
      />
      <div className="flex min-h-0 flex-1 flex-col gap-4">
        <DateRangeBar
          preset={preset}
          range={range}
          tr={tr}
          onChange={(p, r) => {
            setPreset(p);
            setRange(r);
            setOffset(0);
          }}
        />
        {status && (
          <p className={`m-0 flex items-center gap-2 text-xs ${status.tone === "error" ? "text-danger" : "text-success"}`} data-analysis-status>
            {status.text}
          </p>
        )}
        {batches.length > 0 && (
          <Card className="flex flex-col gap-1 p-3" data-analysis-batches>
            <span className="text-xs font-medium text-text-secondary">{tr("analysis.batches")}</span>
            {batches.map((b) => (
              <div key={b.batchId} className="flex items-center gap-3 text-sm" data-analysis-batch={b.batchId}>
                <span className="min-w-0 flex-1 truncate">
                  {tr("analysis.batchLine", { source: tr("analysis.source.gdpr"), rows: b.rows, from: day(b.from), to: day(b.to) })}
                </span>
                <Button size="sm" data-analysis-undo onClick={() => void undoImport(b.batchId)}>{tr("analysis.undo")}</Button>
              </div>
            ))}
          </Card>
        )}
        {(failed || comparisonFailed) && (
          <Card className="flex items-center justify-between gap-3 p-3">
            <span className="text-sm text-danger" data-analysis-load-error>{tr("analysis.loadFailed")}</span>
            <Button size="sm" onClick={() => { refresh(); setReloadComparison((n) => n + 1); }}>{tr("common.retry")}</Button>
          </Card>
        )}
        {loading ? (
          <div className="flex flex-col gap-4" aria-busy="true" aria-label={tr("common.loading")}>
            <Skeleton height="6rem" />
            <Skeleton height="10rem" />
          </div>
        ) : !hasData && !filtersActive ? (
          <Card data-analysis-empty>
            <EmptyState icon={<BarChart3 />} title={tr("common.noTradesYet")}>{tr("analysis.emptyHint")}</EmptyState>
          </Card>
        ) : (
          <>
            {capped && <p className="m-0 text-xs text-warning" data-analysis-scope>{tr("analysis.analyticsScope", { loaded: events.length.toLocaleString(locale), total: total.toLocaleString(locale) })}</p>}
            {comparisonCapped && <p className="m-0 text-xs text-warning" data-analysis-year-scope>{tr("analysis.yearScope", { loaded: cmpEvents.length.toLocaleString(locale), total: (comparison?.total ?? 0).toLocaleString(locale) })}</p>}
            <Summary flow={flow} basis={basis} best={best} tr={tr} locale={locale} />
            <div className={gap}>
              <MonthlyChart rows={months} tr={tr} locale={locale} />
              <RecentDays days={recent} today={today} loading={comparison == null && !comparisonFailed} failed={comparisonFailed} tr={tr} locale={locale} />
            </div>
            <div className={gap}>
              <TopItems titleKey="analysis.topSold" rows={topItems(events, "sold")} side="sold" tr={tr} locale={locale} />
              <TopItems titleKey="analysis.topBought" rows={topItems(events, "bought")} side="bought" tr={tr} locale={locale} />
            </div>
            <div className={gap}>
              <TypePanel rows={types} tr={tr} locale={locale} />
              <Partners rows={partners} tr={tr} locale={locale} />
            </div>
            {comparison && !comparisonFailed && <YearCompare comparison={yearly} tr={tr} locale={locale} />}
          </>
        )}
        {!loading && (hasData || filtersActive) && (
          <div className="flex min-h-[20rem] flex-col">
            <LedgerTable
              page={page}
              loading={tableLoading}
              search={search}
              typeFilter={typeFilter}
              offset={offset}
              limit={TABLE_PAGE_SIZE}
              tr={tr}
              locale={locale}
              onSearch={(v) => { setSearch(v); setOffset(0); }}
              onTypeFilter={(v) => { setTypeFilter(v); setOffset(0); }}
              onOffset={(v) => setOffset(Math.max(0, v))}
              onEdit={(event) => { setEditing(event); setEditError(null); }}
              onMatch={setMatching}
            />
          </div>
        )}
      </div>
      {matching && <MatchModal event={matching} tr={tr} onClose={() => setMatching(null)} onClosed={refresh} />}
      {editing && <RowEditor event={editing} saving={saving} error={editError} tr={tr} onSave={(p) => void saveEdit(p)} onClose={() => setEditing(null)} />}
      {importing && <ImportDialog preview={preview} busy={importBusy} error={importError} tr={tr} onPreview={(p) => void previewImport(p)} onApply={() => void applyImport()} onClose={closeImport} />}
    </section>
  );
}
