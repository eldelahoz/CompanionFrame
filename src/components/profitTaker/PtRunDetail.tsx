import { askConfirm } from "../../stores/confirm";
import { useEffect, useRef, useState } from "react";
import { formatRunDate } from "../../lib/arbi/arbiChartData";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { PtRunRecord } from "../../lib/ipc";
import {
  PT_EXCLUSION_KEYS,
  PT_METRICS,
  PT_METRIC_KEYS,
  formatPtSeconds,
  formatPtTime,
  ptComparison,
  ptComparisonExclusionReason,
  ptEligibleRuns,
  ptMetricValue,
  ptPersonalBest,
  ptPhaseRows,
  ptSquadSize,
  type PtMetric,
} from "../../lib/profitTakerStats";
import { usePersisted } from "../../lib/usePersisted";
import { ptRuns, updatePtNotes, updatePtTags } from "../../stores/ptRuns";
import { ChevronLeft, ChevronRight, Trash2, X } from "lucide-react";
import { Badge, Button, Field, IconButton, Input, Segmented, Select, Table } from "../ui";
import PtMetricCards from "./PtMetricCards";

const ELEMENT_KEYS: Record<string, MessageKey | undefined> = {
  impact: "pt.element.impact",
  puncture: "pt.element.puncture",
  slash: "pt.element.slash",
  cold: "pt.element.cold",
  heat: "pt.element.heat",
  toxin: "pt.element.toxin",
  electric: "pt.element.electric",
  gas: "pt.element.gas",
  viral: "pt.element.viral",
  magnetic: "pt.element.magnetic",
  radiation: "pt.element.radiation",
  corrosive: "pt.element.corrosive",
  blast: "pt.element.blast",
};

const LEG_KEYS: Record<string, MessageKey> = {
  frontLeft: "pt.leg.frontLeft",
  frontRight: "pt.leg.frontRight",
  backLeft: "pt.leg.backLeft",
  backRight: "pt.leg.backRight",
};

const PHASE_METRICS = ["shield", "leg", "body", "pylon"] as const;
const deltaLabel = (pct: number) => `${pct >= 0 ? "+" : ""}${pct.toFixed(1)}%`;

interface Props {
  run: PtRunRecord;
  onBack: () => void;
  /** Runs in the list's current filter order; drives previous/next. */
  orderedRuns: PtRunRecord[];
  /** Every known run, for the personal-best pool. */
  allRuns: PtRunRecord[];
  onNavigate: (id: string) => void;
  tr: Translator;
}

export default function PtRunDetail({ run, onBack, orderedRuns, allRuns, onNavigate, tr }: Props) {
  const [baselineId, setBaselineId] = useState("");
  const [phaseView, setPhaseView] = usePersisted("pt-phase-view", ["cards", "table"] as const, "cards");
  const [tagDraft, setTagDraft] = useState("");
  const [notes, setNotes] = useState(run.notes ?? "");
  const pending = useRef<{ id: string; text: string; timer: ReturnType<typeof setTimeout> } | null>(null);

  const flushNotes = () => {
    const p = pending.current;
    if (!p) return;
    clearTimeout(p.timer);
    pending.current = null;
    void updatePtNotes(p.id, p.text);
  };
  // Previous/next swaps the run in place: save the edit to the run it was typed on, then follow.
  useEffect(() => {
    setNotes(run.notes ?? "");
    return flushNotes;
  }, [run.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const index = orderedRuns.findIndex((r) => r.id === run.id);
  const prev = index > 0 ? orderedRuns[index - 1] : null;
  const next = index >= 0 && index < orderedRuns.length - 1 ? orderedRuns[index + 1] : null;
  const go = (target: PtRunRecord | null) => target && onNavigate(target.id);

  const pbRows = ptPersonalBest(run, allRuns);
  const totalPb = pbRows.find((r) => r.metric === "total") ?? null;
  const tags = run.tags ?? [];
  const players = (Array.isArray(run.players) ? run.players : []).filter((n): n is string => typeof n === "string" && !!n.trim());
  const squadSize = ptSquadSize(run);
  const exclusion = ptComparisonExclusionReason(run);
  const phases = ptPhaseRows(run);
  const baselines = ptEligibleRuns(allRuns).filter((r) => r.id !== run.id && ptSquadSize(r) === squadSize);
  const baseline = baselines.find((r) => r.id === baselineId) ?? null;
  const comparison = baseline ? ptComparison(run, baseline) : [];

  /** PB chip under a stat card: better-than-second when it leads, else the gap. */
  const pbSubtext = (metric: PtMetric): { text: string; good: boolean } | null => {
    const row = pbRows.find((r) => r.metric === metric);
    if (!row || row.poolSize < 2) return null;
    if (row.isPb) return { text: row.vsSecondPct !== null ? tr("arbi.pb.vsSecond", { delta: deltaLabel(row.vsSecondPct) }) : tr("arbi.pb.badge"), good: true };
    return row.vsBestPct === null ? null : { text: tr("arbi.pb.vsBest", { delta: deltaLabel(row.vsBestPct) }), good: false };
  };
  // An unmapped DT_ token has no key; show the raw name rather than the key.
  const elementLabel = (element: string) => (ELEMENT_KEYS[element] ? tr(ELEMENT_KEYS[element]!) : element);
  const pylonOf = (phaseIndex: number, seconds: number | null) => (run.bugged && phaseIndex === 3 ? null : seconds);

  const onNotes = (text: string) => {
    setNotes(text);
    if (pending.current) clearTimeout(pending.current.timer);
    pending.current = { id: run.id, text, timer: setTimeout(flushNotes, 600) };
  };
  const addTag = () => {
    const value = tagDraft.trim();
    if (!value) return;
    setTagDraft("");
    void updatePtTags(run.id, [...tags, value]);
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex items-center gap-3">
          <span data-pt-detail-back>
            <Button
              size="sm"
              onClick={() => {
                flushNotes();
                onBack();
              }}
            >
              {tr("arbi.back")}
            </Button>
          </span>
          <div className="flex items-center gap-1">
            <IconButton data-pt-prev label={tr("arbi.nav.previous")} disabled={!prev} onClick={() => go(prev)}>
              <ChevronLeft />
            </IconButton>
            <IconButton data-pt-next label={tr("arbi.nav.next")} disabled={!next} onClick={() => go(next)}>
              <ChevronRight />
            </IconButton>
          </div>
          <div className="flex flex-col">
            <span className="font-mono text-lg font-bold leading-tight text-text-primary">{formatPtTime(run.durationSec)}</span>
            <span className="text-xs text-text-muted">
              {formatRunDate(run.startedAt)}
              {run.source === "imported" && `, ${tr("common.imported")}`}
              {players.length > 0 && `, ${players.join(", ")}`}
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
            void ptRuns.remove(run.id);
            onBack();
          }}
        >
          {tr("arbi.deleteRun")}
        </Button>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-xs">
        <Badge>{squadSize === null ? tr("pt.unknownSquad") : squadSize === 1 ? tr("pt.recordedPlayer") : tr("pt.squadSize", { count: squadSize })}</Badge>
        {run.aborted ? (
          <Badge tone="danger">{tr("arbi.end.aborted")}</Badge>
        ) : run.complete ? (
          <Badge tone="success">{tr("pt.badge.complete")}</Badge>
        ) : (
          <Badge tone="warning" title={tr("arbi.incompleteHint")}>
            {tr("arbi.incomplete")}
          </Badge>
        )}
        {run.bugged && (
          <Badge tone="warning" title={tr("pt.badge.buggedHint")}>
            {tr("pt.badge.bugged")}
          </Badge>
        )}
        {run.hostMigration && (
          <Badge tone="warning" title={tr("pt.badge.migrationHint")}>
            {tr("pt.badge.migration")}
          </Badge>
        )}
        {run.flightUnreliable && (
          <Badge title={tr("pt.badge.flightEstimateHint")}>
            {tr("pt.badge.flightEstimate")}
          </Badge>
        )}
      </div>

      <p data-pt-recorded-roster className="m-0 text-xs text-text-muted">
        {tr("pt.recordedRosterHint")}
      </p>
      {exclusion ? (
        <p data-pt-exclusion={exclusion} className="m-0 rounded-lg border border-warning/30 bg-warning/10 px-3 py-2 text-sm text-warning">
          {tr("pt.excluded")}: {tr(PT_EXCLUSION_KEYS[exclusion])}
        </p>
      ) : (
        <div className="flex flex-wrap items-end gap-3">
          <Field label={tr("pt.baseline")}>
            <Select
              data-pt-baseline
              value={baselineId}
              onChange={(e) => setBaselineId(e.target.value)}
              options={[["", tr("common.none")], ...baselines.map((c) => [c.id, `${formatRunDate(c.startedAt)} | ${formatPtSeconds(c.durationSec)}s`] as const)]}
            />
          </Field>
          <span className="pb-1.5 text-xs text-text-muted">{tr("pt.baseline.hint")}</span>
        </div>
      )}

      <PtMetricCards
        tr={tr}
        rows={PT_METRICS.map((metric) => ({
          metric,
          value: metric === "pylon" && run.bugged ? null : ptMetricValue(run, metric),
          detail: pbSubtext(metric),
          delta: comparison.find((row) => row.metric === metric) ?? null,
        }))}
      />

      <div className="flex items-center justify-between gap-3">
        <h3 className="m-0 text-base font-semibold text-text-primary">{tr("pt.phases")}</h3>
        <Segmented
          label={tr("pt.phases")}
          items={[
            { key: "cards", label: tr("pt.phase.cards") },
            { key: "table", label: tr("pt.phase.table") },
          ]}
          active={phaseView}
          onSelect={setPhaseView}
        />
      </div>
      {phaseView === "cards" ? (
        <div className="grid gap-3 xl:grid-cols-2" data-pt-phases>
          {phases.map((row) => (
            <section key={row.index} data-pt-phase={row.index} className="ui-card min-w-0 p-4">
              <header className="mb-4 flex flex-wrap items-start justify-between gap-3">
                <h4 className="m-0 text-md font-semibold text-text-primary">
                  {tr("pt.col.phase")} {row.index}
                </h4>
                <div className="flex gap-6 text-right">
                  <div>
                    <span className="block text-xs text-text-muted">{tr("pt.phase.duration")}</span>
                    <strong className="font-mono text-lg text-text-primary">
                      <span data-pt-phase-duration>{formatPtSeconds(row.phase?.totalSec ?? null)}</span>s
                    </strong>
                  </div>
                  <div>
                    <span className="block text-xs text-text-muted">{tr("pt.phase.elapsed")}</span>
                    <strong className="font-mono text-lg text-text-primary">
                      <span data-pt-phase-elapsed>{formatPtSeconds(row.elapsed)}</span>s
                    </strong>
                  </div>
                </div>
              </header>
              {row.phase ? (
                <div className="grid gap-4 sm:grid-cols-[120px_minmax(0,1fr)]">
                  <dl className="m-0 space-y-2 border-r border-border pr-4 text-sm">
                    {PHASE_METRICS.map((metric) => (
                      <div key={metric} className="flex justify-between gap-2">
                        <dt className="text-text-secondary">{tr(PT_METRIC_KEYS[metric])}</dt>
                        <dd data-pt-phase-pylon={metric === "pylon" ? "" : undefined} className="m-0 font-mono text-text-primary">
                          {formatPtSeconds(metric === "pylon" ? pylonOf(row.index, row.phase!.pylonSec) : row.phase![`${metric}Sec`])}
                        </dd>
                      </div>
                    ))}
                  </dl>
                  <div className="min-w-0 space-y-3">
                    {row.phase.shields.length > 0 && (
                      <div>
                        <h5 className="m-0 mb-1.5 text-xs font-semibold text-text-muted">{tr("pt.phase.shieldOrder")}</h5>
                        <ol className="m-0 flex list-none flex-wrap gap-1.5 p-0">
                          {row.phase.shields.map((s, i) => (
                            <li key={i} className="flex items-center gap-1.5 rounded-[var(--radius-sm)] bg-bg-raised px-2 py-1 text-xs">
                              <span className="text-text-muted">{i + 1}</span>
                              <span className="text-text-secondary">{elementLabel(s.element)}</span>
                              <span className="font-mono text-text-primary">{formatPtSeconds(s.seconds)}s</span>
                            </li>
                          ))}
                        </ol>
                      </div>
                    )}
                    <div>
                      <h5 className="m-0 mb-1.5 text-xs font-semibold text-text-muted">{tr("pt.phase.legOrder")}</h5>
                      <ol className="m-0 grid list-none gap-1.5 p-0 sm:grid-cols-2">
                        {row.phase.legs.map((leg, i) => (
                          <li key={i} className="flex items-center gap-2 rounded-[var(--radius-sm)] bg-bg-raised px-2 py-1 text-xs">
                            <span className="text-text-muted">{i + 1}</span>
                            <span className="flex-1 text-text-secondary">{LEG_KEYS[leg.leg] ? tr(LEG_KEYS[leg.leg]) : leg.leg}</span>
                            <span className="font-mono text-text-primary">{formatPtSeconds(leg.seconds)}s</span>
                          </li>
                        ))}
                      </ol>
                    </div>
                  </div>
                </div>
              ) : (
                <p className="m-0 py-5 text-sm text-text-muted">{tr("pt.phase.missing")}</p>
              )}
            </section>
          ))}
        </div>
      ) : (
        <div data-pt-phases>
          <Table label={tr("pt.phases")}>
            <thead>
              <tr>
                <th>{tr("pt.col.phase")}</th>
                <th className="ui-num">{tr("pt.phase.duration")}</th>
                <th className="ui-num">{tr("pt.phase.elapsed")}</th>
                {PHASE_METRICS.map((m) => (
                  <th key={m} className="ui-num">
                    {tr(PT_METRIC_KEYS[m])}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {phases.map((row) => (
                <tr key={row.index} data-pt-phase={row.index}>
                  <th>{row.index}</th>
                  <td className="ui-num" data-pt-phase-duration>
                    {formatPtSeconds(row.phase?.totalSec ?? null)}
                  </td>
                  <td className="ui-num" data-pt-phase-elapsed>
                    {formatPtSeconds(row.elapsed)}
                  </td>
                  {PHASE_METRICS.map((m) => (
                    <td key={m} className="ui-num">
                      {formatPtSeconds(m === "pylon" ? pylonOf(row.index, row.phase?.pylonSec ?? null) : (row.phase?.[`${m}Sec`] ?? null))}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </Table>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-text-secondary">{tr("common.tags")}</span>
        {tags.map((tag) => (
          <Badge key={tag} tone="info">
            {tag}
            <button type="button" className="ui-tag-x" data-tip={tr("arbi.tags.remove")} aria-label={tr("arbi.tags.remove")} onClick={() => void updatePtTags(run.id, tags.filter((t) => t !== tag))}>
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

      {totalPb && totalPb.poolSize > 1 && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-[var(--radius-md)] border border-border bg-bg-raised px-3 py-2 text-xs">
          <span className=" text-text-muted">{tr("arbi.pb.title")}</span>
          <span className="text-text-muted">{tr("pt.pb.pool")}</span>
          <span className="text-text-secondary">{tr("arbi.pb.rank", { rank: String(totalPb.rank), count: String(totalPb.poolSize) })}</span>
        </div>
      )}

      <label className="flex flex-col gap-1">
        <span className="text-xs font-medium text-text-secondary">{tr("arbi.notes.label")}</span>
        <textarea
          data-pt-notes
          className="ui-input min-h-[4.5rem] w-full resize-y py-1.5"
          maxLength={2000}
          placeholder={tr("arbi.notes.placeholder")}
          value={notes}
          onChange={(e) => onNotes(e.target.value)}
          onBlur={flushNotes}
        />
      </label>
    </div>
  );
}
