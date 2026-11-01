import { askConfirm } from "../../stores/confirm";
import { formatInt } from "../../lib/format";
import { useEffect, useRef, useState } from "react";
import type { ArbiRunEndReason } from "../../../config/shared/arbiTypes";
import { hasCadenceData } from "../../lib/arbi/arbiCadence";
import { formatDuration, formatRunDate, missionKindLabel } from "../../lib/arbi/arbiChartData";
import { isIncompleteRun } from "../../lib/arbi/arbiCompare";
import { ARBI_MISSION_TYPE_KEYS } from "../../lib/arbi/arbiLabels";
import { arbiPersonalBest, type ArbiTrendMetric } from "../../lib/arbi/arbiTrends";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { ArbiRun } from "../../lib/ipc";
import { arbiRuns, updateArbiNotes, updateArbiTags } from "../../stores/arbiRuns";
import { ChevronLeft, ChevronRight, Trash2, X } from "lucide-react";
import { Badge, Button, IconButton, Input, Panel, SummaryGrid, type SummaryItem } from "../ui";
import ArbiClearMap from "./ArbiClearMap";
import ArbiDpmChart from "./ArbiDpmChart";
import ArbiDroneCadenceChart from "./ArbiDroneCadenceChart";
import ArbiRotationGrid from "./ArbiRotationGrid";
import ArbiSaturationChart from "./ArbiSaturationChart";
import ArbiSpawnMap from "./ArbiSpawnMap";
import ArbiTimeline from "./ArbiTimeline";
import ArbiVitusPanel from "./ArbiVitusPanel";

const END_REASON_KEYS: Record<ArbiRunEndReason, MessageKey> = {
  "mission-end": "arbi.end.mission-end",
  aborted: "arbi.end.aborted",
  "new-mission": "arbi.end.new-mission",
  "log-truncated": "arbi.end.log-truncated",
  "app-quit": "arbi.end.app-quit",
  inactivity: "arbi.end.inactivity",
  imported: "common.imported",
};

const PB_METRIC_KEYS: Record<ArbiTrendMetric, MessageKey> = {
  dronesPerMin: "arbi.metric.dronesPerMin",
  expectedVitusPerMin: "arbi.metric.expectedVitusPerMin",
};

const deltaLabel = (pct: number) => `${pct >= 0 ? "+" : ""}${pct.toFixed(1)}%`;

interface Props {
  run: ArbiRun;
  onBack: () => void;
  /** Runs in the list's current filter/sort order; drives previous/next. */
  orderedRuns: ArbiRun[];
  /** Every known run, for the personal-best pool. */
  allRuns: ArbiRun[];
  onNavigate: (id: string) => void;
  tr: Translator;
}

export default function ArbiRunDetail({ run, onBack, orderedRuns, allRuns, onNavigate, tr }: Props) {
  const stats = run.stats;
  const [tagDraft, setTagDraft] = useState("");
  const [notes, setNotes] = useState(run.notes ?? "");
  const pending = useRef<{ id: string; text: string; timer: ReturnType<typeof setTimeout> } | null>(null);
  const tags = run.tags ?? [];

  const flushNotes = () => {
    const p = pending.current;
    if (!p) return;
    clearTimeout(p.timer);
    pending.current = null;
    void updateArbiNotes(p.id, p.text);
  };
  // Previous/next swaps the run in place: save the edit to the run it was typed on, then follow.
  useEffect(() => {
    setNotes(run.notes ?? "");
    return flushNotes;
  }, [run.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const onNotes = (text: string) => {
    setNotes(text);
    if (pending.current) clearTimeout(pending.current.timer);
    pending.current = { id: run.id, text, timer: setTimeout(flushNotes, 600) };
  };

  const index = orderedRuns.findIndex((r) => r.id === run.id);
  const prev = index > 0 ? orderedRuns[index - 1] : null;
  const next = index >= 0 && index < orderedRuns.length - 1 ? orderedRuns[index + 1] : null;
  const go = (target: ArbiRun | null) => target && onNavigate(target.id);

  const pbRows = arbiPersonalBest(run, allRuns);
  const typeLabel = missionKindLabel(run) ?? tr(ARBI_MISSION_TYPE_KEYS[run.missionType]);
  const vitusPerMin = run.vitusActual !== null && run.durationSec > 0 ? (run.vitusActual / (run.durationSec / 60)).toFixed(2) : null;

  const kpis: SummaryItem[] = [
    { key: "drones", label: tr("arbi.kpi.drones"), value: formatInt(run.drones) },
    { key: "enemies", label: tr("arbi.kpi.totalEnemies"), value: formatInt(run.totalEnemies) },
    { key: "duration", label: tr("common.duration"), value: formatDuration(run.durationSec), subtext: `${run.rotations} rot.` },
  ];
  if (stats) {
    kpis.splice(2, 0, { key: "kpd", label: tr("arbi.kpi.killsPerDrone"), value: run.drones > 0 ? stats.killsPerDrone.toFixed(2) : "–" });
    kpis.push({ key: "interval", label: tr("arbi.kpi.avgInterval"), value: stats.avgDroneIntervalSec !== null ? `${stats.avgDroneIntervalSec.toFixed(2)}s` : "–" });
    kpis.push({ key: "vpm", label: tr("arbi.kpi.vitusPerMin"), value: vitusPerMin ?? `≈${stats.vitusPerMin.toFixed(2)}`, tone: vitusPerMin ? "success" : "default" });
  }

  const addTag = () => {
    const value = tagDraft.trim();
    if (!value) return;
    setTagDraft("");
    // The backend dedupes case-insensitively and caps the list.
    void updateArbiTags(run.id, [...tags, value]);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            onClick={() => {
              flushNotes();
              onBack();
            }}
          >
            {tr("arbi.back")}
          </Button>
          <div className="flex items-center gap-1">
            <IconButton data-arbi-prev label={tr("arbi.nav.previous")} disabled={!prev} onClick={() => go(prev)}>
              <ChevronLeft />
            </IconButton>
            <IconButton data-arbi-next label={tr("arbi.nav.next")} disabled={!next} onClick={() => go(next)}>
              <ChevronRight />
            </IconButton>
          </div>
          <div className="flex flex-col">
            <span className="text-lg font-bold leading-tight text-text-primary">{run.node}</span>
            <span className="text-xs text-text-muted">
              {formatRunDate(run.startedAt)}, {typeLabel}, {tr(END_REASON_KEYS[run.endReason])}
              {run.source === "imported" && `, ${tr("common.imported")}`}
              {(run.players ?? []).length > 0 && `, ${(run.players ?? []).join(", ")}`}
            </span>
          </div>
        </div>
        <Button
          variant="danger"
          size="sm"
          icon={<Trash2 />}
          onClick={async () => {
            if (!(await askConfirm(tr("arbi.confirmDeleteRun")))) return;
            pending.current = null;
            void arbiRuns.remove(run.id);
            onBack();
          }}
        >
          {tr("arbi.deleteRun")}
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-text-secondary">{tr("common.tags")}</span>
        {tags.map((tag) => (
          <Badge key={tag} tone="info">
            {tag}
            <button type="button" className="ui-tag-x" data-tip={tr("arbi.tags.remove")} aria-label={tr("arbi.tags.remove")} onClick={() => void updateArbiTags(run.id, tags.filter((t) => t !== tag))}>
              <X aria-hidden="true" />
            </button>
          </Badge>
        ))}
        <Input
          className="w-40"
          aria-label={tr("arbi.tags.add")}
          type="text"
          maxLength={32}
          placeholder={tr("arbi.tags.add")}
          value={tagDraft}
          onChange={(e) => setTagDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addTag();
            }
          }}
          onBlur={addTag}
        />
      </div>

      {pbRows.some((row) => row.poolSize > 1) && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-[var(--radius-md)] border border-border bg-bg-raised px-3 py-2 text-xs">
          <span className="flex items-center gap-2">
            <span className=" text-text-muted">{tr("arbi.pb.title")}</span>
            <span className="text-text-muted">{tr("arbi.pb.pool", { node: run.node, type: typeLabel })}</span>
          </span>
          {pbRows
            .filter((row) => row.poolSize > 1)
            .map((row) => (
              <span key={row.metric} className="flex items-center gap-1.5">
                <span className="text-text-secondary">{tr(PB_METRIC_KEYS[row.metric])}</span>
                <span className="font-mono font-semibold text-text-primary">{row.value.toFixed(2)}</span>
                {row.isPb ? (
                  <>
                    <Badge tone="success">{tr("arbi.pb.badge")}</Badge>
                    {row.vsSecondPct !== null && <span className="text-success">{tr("arbi.pb.vsSecond", { delta: deltaLabel(row.vsSecondPct) })}</span>}
                  </>
                ) : (
                  row.vsBestPct !== null && <span className="text-warning">{tr("arbi.pb.vsBest", { delta: deltaLabel(row.vsBestPct) })}</span>
                )}
                <span className="text-text-muted">{tr("arbi.pb.rank", { rank: String(row.rank), count: String(row.poolSize) })}</span>
              </span>
            ))}
        </div>
      )}

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-text-secondary">{tr("arbi.notes.label")}</span>
        <textarea
          data-arbi-notes
          className="ui-input min-h-[4.5rem] w-full resize-y py-1.5"
          maxLength={2000}
          placeholder={tr("arbi.notes.placeholder")}
          value={notes}
          onChange={(e) => onNotes(e.target.value)}
          onBlur={flushNotes}
        />
      </label>

      <div className="flex flex-col gap-4">
        <SummaryGrid items={kpis} />
        <p className="m-0 flex flex-wrap items-center gap-2 px-1 text-xs text-text-muted">
          <Badge title={tr("arbi.confidence.hint")}>
            <span data-arbi-confidence>{tr("arbi.confidence.label")}</span>
          </Badge>
          <span>{tr("arbi.confidence.summary")}</span>
          {isIncompleteRun(run) && <span className="text-warning">{tr("arbi.incompleteHint")}</span>}
        </p>
        {stats ? (
          <>
            <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
              <ArbiVitusPanel run={run} tr={tr} />
              <ArbiSaturationChart stats={stats} tr={tr} />
              <ArbiDpmChart stats={stats} tr={tr} />
              <ArbiRotationGrid stats={stats} tr={tr} />
            </div>
            {hasCadenceData(stats) && <ArbiTimeline stats={stats} tr={tr} />}
            <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
              <ArbiDroneCadenceChart stats={stats} tr={tr} />
              <ArbiClearMap stats={stats} missionType={run.missionType} tr={tr} />
            </div>
            <ArbiSpawnMap stats={stats} run={run} tr={tr} />
          </>
        ) : (
          <Panel className="p-5">
            <p className="m-0 text-sm text-text-muted">{tr("arbi.noStats")}</p>
          </Panel>
        )}
      </div>
    </div>
  );
}
