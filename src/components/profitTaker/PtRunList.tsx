import { PT_EXCLUSION_KEYS, formatPtTime, ptComparisonExclusionReason, ptSquadSize } from "../../lib/profitTakerStats";
import type { Translator } from "../../lib/i18n";
import type { PtRunRecord } from "../../lib/ipc";
import { ptRuns } from "../../stores/ptRuns";
import RunList from "../RunList";
import { Badge } from "../ui";

interface Props {
  runs: PtRunRecord[];
  onSelect: (id: string) => void;
  bestRunIds?: ReadonlySet<string>;
  tr: Translator;
}

export default function PtRunList({ runs, onSelect, bestRunIds = new Set(), tr }: Props) {
  return (
    <>
      <p data-pt-recorded-roster className="m-0 text-xs text-text-muted">
        {tr("pt.recordedRosterHint")}
      </p>
      <RunList
        runs={runs}
        onSelect={onSelect}
        tr={tr}
        onDelete={(id) => void ptRuns.remove(id)}
        onDeleteLog={(id) => void ptRuns.removeLog(id)}
        tableAttrs={{ "data-pt-runs": "" } as object}
        rowAttrs={(run) => ({ "data-pt-run": run.id })}
        headers={
          <>
            <th className="ui-num">{tr("common.total")}</th>
            <th className="ui-num">{tr("pt.stat.flight")}</th>
            <th />
          </>
        }
        cells={(run) => {
          const size = ptSquadSize(run);
          const exclusion = ptComparisonExclusionReason(run);
          return (
            <>
              <td className="ui-num whitespace-nowrap font-semibold text-text-primary" data-label={tr("common.total")}>{formatPtTime(run.durationSec)}</td>
              <td className="ui-num whitespace-nowrap text-text-secondary" data-label={tr("pt.stat.flight")}>{formatPtTime(run.flightSec)}</td>
              <td>
                <span className="flex flex-wrap items-center gap-1">
                  <Badge>{size === null ? tr("pt.unknownSquad") : size === 1 ? tr("pt.recordedPlayer") : tr("pt.squadSize", { count: size })}</Badge>
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
                      <span data-pt-bugged>{tr("pt.badge.bugged")}</span>
                    </Badge>
                  )}
                  {bestRunIds.has(run.id) && (
                    <Badge tone="success">
                      <span data-pt-pb>{tr("arbi.pb.badge")}</span>
                    </Badge>
                  )}
                  {(exclusion === "migrated" || exclusion === "flight" || exclusion === "telemetry" || exclusion === "squad") && (
                    <Badge tone="warning">
                      <span data-pt-exclusion={exclusion}>{tr(PT_EXCLUSION_KEYS[exclusion])}</span>
                    </Badge>
                  )}
                  {run.source === "imported" && <Badge>{tr("common.imported")}</Badge>}
                  {run.duplicateOf && (
                    <Badge title={tr("arbi.duplicateHint")}>
                      <span data-pt-duplicate>{tr("arbi.duplicate")}</span>
                    </Badge>
                  )}
                  {run.tags.map((tag) => (
                    <Badge key={tag} tone="info">
                      {tag}
                    </Badge>
                  ))}
                </span>
              </td>
            </>
          );
        }}
      />
    </>
  );
}
