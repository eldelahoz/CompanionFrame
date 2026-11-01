import { FileJson, Upload } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { gdprApply, gdprPreviewFile, ledgerAddTrades, onTradeDetected, statsImport, type LedgerImportPreview } from "../lib/ipc";
import type { Translator } from "../lib/i18n";
import { pageLedgerRange } from "../lib/stats/ledgerPaging";
import { assertStatsImportFileSize, normalizeAlecaFrameStats, parseAlecaFrameTrades } from "../lib/stats/importAlecaFrame";
import { usePersisted } from "../lib/usePersisted";
import type { TradeEvent } from "../types/stats";
import HeaderTabs from "../components/HeaderTabs";
import DailyStatsPanel from "../components/stats/DailyStatsPanel";
import StatsTradePanel from "../components/stats/StatsTradePanel";
import { ImportDialog } from "../components/analysis/ledger";
import PersonalStatsPanel from "../components/stats/PersonalStatsPanel";
import { Button, Card, PageHeader, Skeleton } from "../components/ui";

// Older trades live in the paged ledger; the cap keeps a huge archive from stalling the panel.
const TRADE_LIST_MAX_ROWS = 50_000;
const TABS = ["tracking", "personal"] as const;
const byNewest = (a: TradeEvent, b: TradeEvent) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0);

export default function StatsView({ tr, locale }: { tr: Translator; locale: string }) {
  const [tab, setTab] = usePersisted("stats-tab", TABS, "tracking");
  const [trades, setTrades] = useState<TradeEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);
  const [importing, setImporting] = useState(false);
  const [statsRefresh, setStatsRefresh] = useState(0);
  const [preview, setPreview] = useState<LedgerImportPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const load = useRef(0);
  const alive = useRef(true);
  const liveDuringLoad = useRef<TradeEvent[] | null>(null);

  const refreshTrades = useCallback(async () => {
    const mine = ++load.current;
    const live: TradeEvent[] = [];
    liveDuringLoad.current = live;
    try {
      const page = await pageLedgerRange({}, TRADE_LIST_MAX_ROWS, () => alive.current && mine === load.current);
      if (!page) return;
      const byId = new Map([...page.events, ...live].map((t) => [t.id, t]));
      setTrades([...byId.values()].sort(byNewest).slice(0, TRADE_LIST_MAX_ROWS));
    } catch {
      // the ledger is optional until the tracker has data
    } finally {
      if (mine === load.current) liveDuringLoad.current = null;
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    let unlisten: (() => void) | null = null;
    void onTradeDetected(({ trade }) => {
      liveDuringLoad.current?.push(trade as TradeEvent);
      setTrades((cur) => [trade as TradeEvent, ...cur.filter((t) => t.id !== trade.id)].sort(byNewest));
    }).then((u) => (alive.current ? (unlisten = u) : u()));
    void refreshTrades().finally(() => alive.current && setLoading(false));
    return () => {
      alive.current = false;
      unlisten?.();
    };
  }, [refreshTrades]);

  const importAlecaFile = async (file: File) => {
    setStatus({ text: tr("stats.importReading"), error: false });
    try {
      assertStatsImportFileSize(file.size);
      let parsed: unknown;
      try {
        parsed = JSON.parse(await file.text());
      } catch {
        setStatus({ text: tr("stats.invalidJsonFile"), error: true });
        return;
      }
      const days = normalizeAlecaFrameStats(parsed);
      const imported = parseAlecaFrameTrades(parsed);
      if (days.length === 0 && imported.length === 0) {
        setStatus({ text: tr("stats.noValidEntries"), error: true });
        return;
      }
      let text = "";
      if (days.length > 0) {
        const result = await statsImport(days);
        if (!result.ok) {
          setStatus({ text: tr("stats.statsImportFailed"), error: true });
          return;
        }
        text = result.count === 0 ? tr("stats.noNewEntries") : tr(result.count === 1 ? "stats.importedDay" : "stats.importedDays", { count: result.count });
        if (result.count > 0) setStatsRefresh((n) => n + 1);
      }
      if (imported.length > 0) {
        await ledgerAddTrades(imported);
        await refreshTrades();
        text += `${text ? " " : ""}${tr(imported.length === 1 ? "stats.importedTradeSingular" : "stats.importedTradePlural", { count: imported.length })}`;
      }
      setStatus({ text, error: false });
    } catch (e) {
      setStatus({ text: e instanceof Error ? e.message : tr("stats.importFailedGeneric"), error: true });
    }
  };

  const previewImport = async (path: string) => {
    setBusy(true);
    setImportError(null);
    try {
      setPreview(await gdprPreviewFile(path));
    } catch {
      setImportError(tr("analysis.importUnavailable"));
    } finally {
      setBusy(false);
    }
  };
  const applyImport = async () => {
    if (!preview) return;
    setBusy(true);
    setImportError(null);
    try {
      const result = await gdprApply(preview.batchId);
      const skipped = preview.counts.duplicates + result.skippedDuplicates;
      setStatus({ text: tr("analysis.importApplied", { applied: result.applied, skipped }), error: false });
      closeImport();
      void refreshTrades();
    } catch {
      setImportError(tr("analysis.importUnavailable"));
    } finally {
      setBusy(false);
    }
  };
  const closeImport = () => {
    setImporting(false);
    setPreview(null);
    setImportError(null);
  };

  return (
    <section className="mx-auto flex w-full max-w-[1680px] flex-col gap-4" data-stats-view>
      <div data-stats-header>
        <PageHeader
          title={tr("common.stats")}
          actions={
            tab === "tracking" && (
              <div className="flex flex-wrap items-center gap-2" data-stats-controls>
                <Button size="sm" icon={<Upload />} onClick={() => setImporting(true)} title={tr("analysis.importTitle")}>
                  {tr("analysis.import")}
                </Button>
                <label className="ui-btn ui-btn--sm cursor-pointer" title={tr("stats.importAlecaTitle")}>
                  <FileJson aria-hidden="true" />
                  {tr("stats.importAlecaButton")}
                  <input
                    type="file"
                    accept=".json"
                    className="hidden"
                    data-stats-import-aleca
                    onChange={(e) => {
                      const file = e.target.files?.[0];
                      e.target.value = "";
                      if (file) void importAlecaFile(file);
                    }}
                  />
                </label>
              </div>
            )
          }
        />
        <div className="border-b border-border">
          <HeaderTabs
            label={tr("common.stats")}
            options={[
              { key: "tracking", label: tr("profile.tracking") },
              { key: "personal", label: tr("profile.personal") },
            ]}
            activeKey={tab}
            onSelect={(key) => setTab(key === "personal" ? "personal" : "tracking")}
          />
        </div>
      </div>

      {tab === "personal" && <PersonalStatsPanel tr={tr} locale={locale} />}

      {tab === "tracking" &&
        (loading ? (
          <Card className="flex flex-col gap-3 p-4">
            <Skeleton width="30%" height="1.1rem" />
            <Skeleton />
            <Skeleton width="70%" />
          </Card>
        ) : (
          <div className="grid items-start gap-4 min-[1100px]:grid-cols-[minmax(0,1fr)_24rem]" data-stats-tracking>
            <div className="flex min-w-0 flex-col gap-4">
              {status && (
                <p className={`m-0 text-sm ${status.error ? "text-danger" : "text-success"}`} role="status" data-stats-status>
                  {status.text}
                </p>
              )}
              <DailyStatsPanel tr={tr} locale={locale} refreshKey={statsRefresh} />
            </div>
            <StatsTradePanel trades={trades} locale={locale} tr={tr} />
          </div>
        ))}
      {importing && <ImportDialog preview={preview} busy={busy} error={importError} tr={tr} onPreview={(p) => void previewImport(p)} onApply={() => void applyImport()} onClose={closeImport} />}
    </section>
  );
}
