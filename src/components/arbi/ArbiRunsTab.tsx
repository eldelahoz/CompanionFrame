import { askConfirm } from "../../stores/confirm";
import { useEffect, useMemo, useState } from "react";
import { ARBI_COMPARE_MAX } from "../../lib/arbi/arbiCompare";
import { formatBytes } from "../../lib/arbi/arbiChartData";
import type { Translator } from "../../lib/i18n";
import { arbiImportLog } from "../../lib/ipc";
import { arbiRuns, updateArbiTags, useArbiRuns } from "../../stores/arbiRuns";
import { Inbox, RefreshCw, Upload, X } from "lucide-react";
import { Button, Card, Checkbox, EmptyState, Field, Input, Select, Skeleton } from "../ui";
import ArbiCompare from "./ArbiCompare";
import ArbiRunDetail from "./ArbiRunDetail";
import ArbiRunList from "./ArbiRunList";

type TypeFilter = "all" | "defense" | "interception" | "disruption" | "other";
type SourceFilter = "all" | "live" | "imported";
interface Filters {
  minVitus: string;
  tag: string;
  type: TypeFilter;
  minRotations: string;
  minDurationMin: string;
  source: SourceFilter;
  showDuplicates: boolean;
}
const NO_FILTERS: Filters = { minVitus: "", tag: "", type: "all", minRotations: "", minDurationMin: "", source: "all", showDuplicates: false };

// Tags match case-insensitively: "sobek run a" and "Sobek run a" are one tag.
const tagKey = (tag: string) => tag.toLocaleLowerCase();
const num = (s: string) => (s.trim() === "" || !Number.isFinite(Number(s)) ? null : Number(s));

/** Counts, refresh and import controls both run tabs share. */
export function RunsHeader({ title, count, bytes, status, onRefresh, onImport, busy, tr }: { title: string; count: number; bytes: number; status: string; onRefresh: () => void; onImport: (path: string) => void; busy: boolean; tr: Translator }) {
  const [asking, setAsking] = useState(false);
  const [path, setPath] = useState("");
  return (
    <header className="flex flex-wrap items-center justify-between gap-3" aria-label={title}>
      <p className="m-0 text-sm text-text-secondary">
        {tr("arbi.runCount", { count: String(count) })}, {tr("arbi.diskUsage", { size: formatBytes(bytes) })}
      </p>
      <div className="flex flex-wrap items-center gap-2">
        {status && <span className="text-xs text-text-muted" role="status">{status}</span>}
        {asking && (
          // shortcut: the log path is typed, not picked - there is no JS file dialog; add a backend picker.
          <Input className="w-72" autoFocus aria-label="EE.log" placeholder="C:\\path\\to\\EE.log" value={path} onChange={(e) => setPath(e.target.value)} onKeyDown={(e) => e.key === "Enter" && path.trim() && (onImport(path.trim()), setAsking(false))} />
        )}
        <Button size="sm" icon={<RefreshCw />} onClick={onRefresh} disabled={busy} title={tr("arbi.refreshTitle")}>
          {tr("common.refresh")}
        </Button>
        <Button
          size="sm"
          icon={<Upload />}
          onClick={() => {
            if (asking && path.trim()) {
              onImport(path.trim());
              setAsking(false);
            } else setAsking(!asking);
          }}
          disabled={busy}
        >
          {tr("arbi.import")}
        </Button>
      </div>
    </header>
  );
}

interface Props {
  tr: Translator;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  refresh: () => Promise<void>;
  busy: boolean;
}

export default function ArbiRunsTab({ tr, selectedId, onSelect, refresh, busy }: Props) {
  const { runs, diskUsageBytes, loaded } = useArbiRuns();
  const [filters, setFilters] = useState<Filters>(NO_FILTERS);
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [massTag, setMassTag] = useState("");
  const [massBusy, setMassBusy] = useState(false);
  const [comparing, setComparing] = useState(false);
  const [status, setStatus] = useState("");
  const [importing, setImporting] = useState(false);
  const set = <K extends keyof Filters>(k: K, v: Filters[K]) => setFilters((f) => ({ ...f, [k]: v }));

  useEffect(() => {
    if (!loaded) void arbiRuns.load().catch(() => {});
  }, [loaded]);

  const allTags = useMemo(() => [...new Map(runs.flatMap((r) => r.tags ?? []).map((t) => [tagKey(t), t])).values()].sort((a, b) => a.localeCompare(b)), [runs]);
  // The selected tag can vanish (last run deleted/retagged) - don't strand the list.
  const tag = filters.tag && allTags.some((t) => tagKey(t) === tagKey(filters.tag)) ? filters.tag : "";
  const minVitus = num(filters.minVitus);
  const minRot = num(filters.minRotations);
  const minDur = num(filters.minDurationMin);
  const filtered = runs.filter((run) => {
    if (!filters.showDuplicates && run.duplicateOf) return false;
    if (filters.type !== "all" && run.missionType !== filters.type) return false;
    if (minVitus !== null && (run.vitusActual === null || run.vitusActual < minVitus)) return false;
    if (tag && !(run.tags ?? []).some((t) => tagKey(t) === tagKey(tag))) return false;
    if (minRot !== null && run.rotations < minRot) return false;
    if (minDur !== null && run.durationSec < minDur * 60) return false;
    if (filters.source !== "all" && run.source !== filters.source) return false;
    return true;
  });
  const hiddenDuplicates = filters.showDuplicates ? 0 : runs.filter((r) => r.duplicateOf).length;
  const filtersActive = JSON.stringify({ ...filters, tag }) !== JSON.stringify(NO_FILTERS);

  // Drop selections that no longer resolve to a run (deleted elsewhere).
  const alive = new Set(runs.map((r) => r.id));
  const picked = [...selected].filter((id) => alive.has(id));
  const compareRuns = runs.filter((r) => selected.has(r.id)).slice(0, ARBI_COMPARE_MAX);
  const canCompare = picked.length >= 2 && picked.length <= ARBI_COMPARE_MAX;

  const toggle = (id: string) => setSelected(new Set(selected.has(id) ? picked.filter((x) => x !== id) : [...picked, id]));
  const toggleAll = () => {
    const ids = filtered.map((r) => r.id);
    setSelected(new Set(ids.every((id) => selected.has(id)) ? picked.filter((id) => !ids.includes(id)) : [...picked, ...ids]));
  };

  const massDelete = async () => {
    if (massBusy || !picked.length || !(await askConfirm(tr("arbi.confirmDeleteRuns", { count: String(picked.length) })))) return;
    setMassBusy(true);
    try {
      for (const id of picked) await arbiRuns.remove(id);
      setSelected(new Set());
    } finally {
      setMassBusy(false);
    }
  };
  const massAddTag = async () => {
    const value = massTag.trim();
    if (massBusy || !value || !picked.length) return;
    setMassBusy(true);
    try {
      for (const id of picked) {
        const run = runs.find((r) => r.id === id);
        if (run) await updateArbiTags(id, [...(run.tags ?? []), value]);
      }
      setMassTag("");
    } finally {
      setMassBusy(false);
    }
  };
  const importLog = async (path: string) => {
    if (importing) return;
    setImporting(true);
    setStatus("");
    try {
      const r = await arbiImportLog(path);
      if (r.imported.length > 0 || r.skipped > 0) {
        setStatus(tr("arbi.importResult", { imported: String(r.imported.length), skipped: String(r.skipped) }));
        await arbiRuns.load();
      }
    } catch (e) {
      setStatus(String(e));
    } finally {
      setImporting(false);
    }
  };

  const selectedRun = selectedId ? (runs.find((r) => r.id === selectedId) ?? null) : null;
  if (selectedRun) return <ArbiRunDetail run={selectedRun} onBack={() => onSelect(null)} orderedRuns={filtered} allRuns={runs} onNavigate={onSelect} tr={tr} />;

  return (
    <>
      <span data-arbi-runs className="contents">
        <RunsHeader title={tr("arbi.title")} count={runs.length} bytes={diskUsageBytes} status={status} onRefresh={() => void refresh()} onImport={(p) => void importLog(p)} busy={busy || importing} tr={tr} />
      </span>
      {runs.length === 0 ? (
        // An empty store also means "first load still running", which "no runs yet" misreports.
        <Card>
          {loaded ? (
            <EmptyState icon={<Inbox />} title={tr("arbi.empty")} />
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
          <Card className="flex flex-wrap items-end gap-3 p-3" data-arbi-filters>
            <Field label={tr("arbi.filter.minVitus")}>
              <Input className="w-24" type="number" min="0" placeholder="0" value={filters.minVitus} onChange={(e) => set("minVitus", e.target.value)} />
            </Field>
            <Field label={tr("common.type")}>
              <Select
                value={filters.type}
                onChange={(e) => set("type", e.target.value as TypeFilter)}
                options={[["all", tr("arbi.filter.allTypes")], ["defense", tr("arbi.type.defense")], ["interception", tr("arbi.type.interception")], ["disruption", tr("arbi.type.disruption")], ["other", tr("arbi.type.other")]]}
              />
            </Field>
            <Field label={tr("arbi.filter.minRotations")}>
              <Input className="w-20" type="number" min="0" placeholder="0" value={filters.minRotations} onChange={(e) => set("minRotations", e.target.value)} />
            </Field>
            <Field label={tr("arbi.filter.minDuration")}>
              <Input className="w-20" type="number" min="0" placeholder="0" value={filters.minDurationMin} onChange={(e) => set("minDurationMin", e.target.value)} />
            </Field>
            <Field label={tr("common.source")}>
              <Select value={filters.source} onChange={(e) => set("source", e.target.value as SourceFilter)} options={[["all", tr("arbi.filter.allSources")], ["live", tr("common.live")], ["imported", tr("common.imported")]]} />
            </Field>
            {allTags.length > 0 && (
              <Field label={tr("arbi.filter.tag")}>
                <Select value={tag} onChange={(e) => set("tag", e.target.value)} options={[["", tr("arbi.filter.allTags")], ...allTags.map((t) => [t, t] as const)]} />
              </Field>
            )}
            <Checkbox className="pb-1.5" data-arbi-show-duplicates label={tr("arbi.filter.showDuplicates")} checked={filters.showDuplicates} onChange={(e) => set("showDuplicates", e.target.checked)} />
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

          {picked.length > 0 && (
            <Card className="flex flex-wrap items-center gap-2 border-accent p-3">
              <span className="text-sm font-semibold text-text-primary">{tr("common.selected", { count: String(picked.length) })}</span>
              <Input className="w-36" type="text" maxLength={32} aria-label={tr("arbi.tags.add")} placeholder={tr("arbi.tags.add")} value={massTag} onChange={(e) => setMassTag(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void massAddTag()} />
              <Button size="sm" disabled={massBusy || !massTag.trim()} onClick={() => void massAddTag()}>
                {tr("arbi.massTag")}
              </Button>
              <Button size="sm" data-arbi-compare disabled={!canCompare} title={tr("arbi.compare.max", { max: String(ARBI_COMPARE_MAX) })} onClick={() => setComparing(!comparing)}>
                {tr("arbi.compare.open")}
              </Button>
              <Button size="sm" variant="danger" disabled={massBusy} onClick={() => void massDelete()}>
                {tr("common.deleteSelected")}
              </Button>
              <Button size="sm" variant="ghost" className="ml-auto" onClick={() => setSelected(new Set())}>
                {tr("arbi.filter.clear")}
              </Button>
            </Card>
          )}
          {comparing && canCompare && compareRuns.length >= 2 && <ArbiCompare runs={compareRuns} filteredRuns={filtered} allRuns={runs} onClose={() => setComparing(false)} tr={tr} />}
          <Card className="overflow-hidden">
            <ArbiRunList runs={filtered} onSelect={onSelect} selected={new Set(picked)} onToggleSelect={toggle} onToggleSelectAll={toggleAll} tr={tr} />
          </Card>
        </div>
      )}
    </>
  );
}
