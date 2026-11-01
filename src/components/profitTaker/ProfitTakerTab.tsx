import { useEffect, useMemo, useState } from "react";
import type { Translator } from "../../lib/i18n";
import { ptImportLog } from "../../lib/ipc";
import { ptBestRunIds, ptFilterRuns, type PtFilters } from "../../lib/profitTakerStats";
import { usePersisted } from "../../lib/usePersisted";
import { ptRuns, usePtRuns } from "../../stores/ptRuns";
import { RunsHeader } from "../arbi/ArbiRunsTab";
import { Inbox, X } from "lucide-react";
import { Button, Card, Checkbox, EmptyState, Field, Input, Segmented, Select, Skeleton } from "../ui";
import PtAnalytics from "./PtAnalytics";
import PtRunDetail from "./PtRunDetail";
import PtRunList from "./PtRunList";

interface Filters {
  squad: PtFilters["squad"];
  status: PtFilters["status"];
  from: string;
  to: string;
  tag: string;
  showDuplicates: boolean;
}
const NO_FILTERS: Filters = { squad: "all", status: "all", from: "", to: "", tag: "", showDuplicates: false };
const tagKey = (tag: string) => tag.toLocaleLowerCase();

interface Props {
  tr: Translator;
  locale: string;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  refresh: () => Promise<void>;
  busy: boolean;
}

export default function ProfitTakerTab({ tr, locale, selectedId, onSelect, refresh, busy }: Props) {
  const { runs, diskUsageBytes, loaded } = usePtRuns();
  const [view, setView] = usePersisted("pt-subtab", ["runs", "analytics"] as const, "runs");
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  const [status, setStatus] = useState("");
  const [importing, setImporting] = useState(false);
  const set = <K extends keyof Filters>(k: K, v: Filters[K]) => setFilters((f) => ({ ...f, [k]: v }));

  useEffect(() => {
    if (!loaded) void ptRuns.load().catch(() => {});
  }, [loaded]);

  const allTags = useMemo(() => [...new Map(runs.flatMap((r) => r.tags ?? []).map((t) => [tagKey(t), t])).values()].sort((a, b) => a.localeCompare(b)), [runs]);
  const tag = filters.tag && allTags.some((t) => tagKey(t) === tagKey(filters.tag)) ? filters.tag : "";
  const filtered = ptFilterRuns(runs, { ...filters, tag });
  const hiddenDuplicates = filters.showDuplicates ? 0 : runs.filter((r) => r.duplicateOf).length;
  const filtersActive = JSON.stringify({ ...filters, tag }) !== JSON.stringify(NO_FILTERS);
  const bestIds = useMemo(() => ptBestRunIds(runs), [runs]);

  const importLog = async (path: string) => {
    if (importing) return;
    setImporting(true);
    setStatus("");
    try {
      const r = await ptImportLog(path);
      if (r.imported.length > 0 || r.skipped > 0) {
        setStatus(tr("pt.importResult", { imported: String(r.imported.length), skipped: String(r.skipped) }));
        await ptRuns.load();
      }
    } catch (e) {
      setStatus(String(e));
    } finally {
      setImporting(false);
    }
  };

  const selectedRun = selectedId ? (runs.find((r) => r.id === selectedId) ?? null) : null;
  if (selectedRun) return <PtRunDetail run={selectedRun} onBack={() => onSelect(null)} orderedRuns={filtered} allRuns={runs} onNavigate={onSelect} tr={tr} />;

  return (
    <>
      <RunsHeader title={tr("pt.title")} count={runs.length} bytes={diskUsageBytes} status={status} onRefresh={() => void refresh()} onImport={(p) => void importLog(p)} busy={busy || importing} tr={tr} />
      <div className="w-fit">
        <Segmented
          label={tr("pt.title")}
          items={[
            { key: "runs", label: tr("pt.runs") },
            { key: "analytics", label: tr("common.analytics") },
          ]}
          active={view}
          onSelect={setView}
        />
      </div>

      {loaded && runs.length === 0 && view === "analytics" ? (
        <PtAnalytics runs={[]} onSelect={onSelect} tr={tr} locale={locale} />
      ) : runs.length === 0 ? (
        // An empty store also means "first load still running", which "no runs yet" misreports.
        <Card>
          {loaded ? (
            <EmptyState icon={<Inbox />} title={tr("pt.empty")} />
          ) : (
            <div className="flex flex-col gap-3 p-4">
              <Skeleton width="40%" height="1.1rem" />
              <Skeleton />
              <Skeleton width="70%" />
            </div>
          )}
        </Card>
      ) : (
        <div className="flex flex-col gap-4">
          <Card className="flex flex-wrap items-end gap-3 p-3" data-pt-filters>
            <Field label={tr("pt.filter.mode")}>
              <Select
                data-pt-filter-squad
                value={filters.squad}
                onChange={(e) => set("squad", e.target.value as Filters["squad"])}
                options={[["all", tr("common.all")], ["solo", tr("pt.recordedPlayer")], ...["2", "3", "4"].map((size) => [size, tr("pt.squadSize", { count: size })] as const), ["unknown", tr("pt.unknownSquad")]]}
              />
            </Field>
            {allTags.length > 0 && (
              <Field label={tr("arbi.filter.tag")}>
                <Select data-pt-filter-tag value={tag} onChange={(e) => set("tag", e.target.value)} options={[["", tr("arbi.filter.allTags")], ...allTags.map((t) => [t, t] as const)]} />
              </Field>
            )}
            <Field label={tr("browse.status")}>
              <Select
                data-pt-filter-status
                value={filters.status}
                onChange={(e) => set("status", e.target.value as Filters["status"])}
                options={[["all", tr("common.all")], ["eligible", tr("pt.eligible")], ["bugged", tr("pt.badge.bugged")], ["incomplete", tr("arbi.incomplete")]]}
              />
            </Field>
            <Field label={tr("analysis.range.from")}>
              <Input type="date" data-pt-filter-from value={filters.from} onChange={(e) => set("from", e.target.value)} />
            </Field>
            <Field label={tr("analysis.range.to")}>
              <Input type="date" data-pt-filter-to value={filters.to} onChange={(e) => set("to", e.target.value)} />
            </Field>
            <Checkbox className="pb-1.5" data-pt-show-duplicates label={tr("arbi.filter.showDuplicates")} checked={filters.showDuplicates} onChange={(e) => set("showDuplicates", e.target.checked)} />
            <div className="ml-auto flex flex-wrap items-center gap-2 pb-1 text-xs text-text-muted">
              {hiddenDuplicates > 0 && <span>{tr("arbi.filter.duplicatesHidden", { count: String(hiddenDuplicates) })}</span>}
              <span>{tr("arbi.filter.showing", { shown: String(filtered.length), total: String(runs.length) })}</span>
              {filtersActive && (
                <Button variant="ghost" size="sm" icon={<X />} onClick={() => setFilters(NO_FILTERS)}>
                  {tr("arbi.filter.clear")}
                </Button>
              )}
            </div>
          </Card>
          {view === "analytics" ? (
            <PtAnalytics runs={filtered} onSelect={onSelect} tr={tr} locale={locale} />
          ) : (
            <Card className="flex flex-col gap-2 overflow-hidden">
              <PtRunList runs={filtered} onSelect={onSelect} bestRunIds={bestIds} tr={tr} />
            </Card>
          )}
        </div>
      )}
    </>
  );
}
