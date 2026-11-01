import { useState } from "react";
import { ARBI_SATURATION_THRESHOLD, formatRunDate, missionKindLabel } from "../../lib/arbi/arbiChartData";
import { arbiAveragePool, buildArbiComparison, formatArbiMetric, type ArbiAverageScope } from "../../lib/arbi/arbiCompare";
import { ARBI_MISSION_TYPE_KEYS } from "../../lib/arbi/arbiLabels";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { ArbiRun } from "../../lib/ipc";
import { Button, Card, Field, Select, Table } from "../ui";

const SCOPES: ArbiAverageScope[] = ["filtered", "missionType", "node", "squad"];
const SCOPE_KEYS: Record<ArbiAverageScope, MessageKey> = {
  filtered: "arbi.compare.scope.filtered",
  missionType: "arbi.compare.scope.missionType",
  node: "arbi.compare.scope.node",
  squad: "arbi.compare.scope.squad",
};

interface Props {
  runs: ArbiRun[];
  filteredRuns: ArbiRun[];
  allRuns: ArbiRun[];
  onClose: () => void;
  tr: Translator;
}

export default function ArbiCompare({ runs, filteredRuns, allRuns, onClose, tr }: Props) {
  const [scope, setScope] = useState<ArbiAverageScope>("filtered");
  const reference = runs[0] ?? null;
  // "Filtered" answers "how does this compare to what I am looking at"; the narrower
  // scopes answer "to every run like it", so they span the whole index.
  const pool = arbiAveragePool(scope === "filtered" ? filteredRuns : allRuns, reference, scope);
  const rows = buildArbiComparison(runs, pool);
  const na = tr("arbi.compare.na");

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-end gap-3">
        <h3 className="m-0 pb-1.5 text-md font-semibold text-text-primary">{tr("arbi.compare.title")}</h3>
        <Field label={tr("arbi.compare.average")}>
          <Select value={scope} onChange={(e) => setScope(e.target.value as ArbiAverageScope)} options={SCOPES.map((o) => [o, tr(SCOPE_KEYS[o])] as const)} />
        </Field>
        <span className="pb-1.5 text-xs text-text-muted">{tr("arbi.compare.poolSize", { count: String(pool.length) })}</span>
        <Button size="sm" className="ml-auto" onClick={onClose}>
          {tr("common.close")}
        </Button>
      </div>
      <div data-arbi-compare-panel>
        <Table label={tr("arbi.compare.title")}>
          <thead>
            <tr>
              <th>{tr("common.mission")}</th>
              {runs.map((run) => (
                <th key={run.id} className="ui-num">
                  <span className="block text-text-primary">{run.node}</span>
                  <span className="block font-normal">
                    {formatRunDate(run.startedAt)}, {missionKindLabel(run) ?? tr(ARBI_MISSION_TYPE_KEYS[run.missionType])}
                  </span>
                </th>
              ))}
              <th className="ui-num">{tr("arbi.compare.average")}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.metric.key}>
                <td className="text-text-secondary">{tr(row.metric.labelKey, { count: String(ARBI_SATURATION_THRESHOLD) })}</td>
                {row.cells.map((cell, i) => {
                  const text = formatArbiMetric(cell.value, row.metric.format);
                  return (
                    <td key={runs[i].id} className={`ui-num ${cell.best ? "font-bold text-success" : text === null ? "text-text-muted" : "text-text-primary"}`}>
                      {text ?? na}
                    </td>
                  );
                })}
                <td className="ui-num text-text-secondary">{formatArbiMetric(row.average, row.metric.format) ?? na}</td>
              </tr>
            ))}
          </tbody>
        </Table>
      </div>
      {runs.some((r) => r.stats === null) && <p className="m-0 text-xs text-text-muted">{tr("arbi.noStats")}</p>}
    </Card>
  );
}
