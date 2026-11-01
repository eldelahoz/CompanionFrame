import { formatInt } from "../../lib/format";
import { formatDuration, missionKindLabel } from "../../lib/arbi/arbiChartData";
import { isIncompleteRun } from "../../lib/arbi/arbiCompare";
import { ARBI_MISSION_TYPE_KEYS } from "../../lib/arbi/arbiLabels";
import type { Translator } from "../../lib/i18n";
import type { ArbiRun } from "../../lib/ipc";
import { arbiRuns } from "../../stores/arbiRuns";
import RunList from "../RunList";
import { Badge, type Tone } from "../ui";

interface Props {
  runs: ArbiRun[];
  onSelect: (id: string) => void;
  selected: ReadonlySet<string>;
  onToggleSelect: (id: string) => void;
  onToggleSelectAll: () => void;
  tr: Translator;
}

const TYPE_TONE: Record<ArbiRun["missionType"], Tone> = { defense: "warning", interception: "accent", disruption: "accent", other: "neutral" };

export default function ArbiRunList({ runs, onSelect, selected, onToggleSelect, onToggleSelectAll, tr }: Props) {
  return (
    <RunList
      runs={runs}
      onSelect={onSelect}
      tr={tr}
      selected={selected}
      onToggleSelect={onToggleSelect}
      onToggleSelectAll={onToggleSelectAll}
      onDelete={(id) => void arbiRuns.remove(id)}
      onDeleteLog={(id) => void arbiRuns.removeLog(id)}
      tableAttrs={{ "data-arbi-run-table": "" } as object}
      headers={
        <>
          <th>{tr("common.node")}</th>
          <th>{tr("common.type")}</th>
          <th className="ui-num">{tr("common.duration")}</th>
          <th className="ui-num">{tr("arbi.col.rotations")}</th>
          <th className="ui-num">{tr("arbi.col.drones")}</th>
          <th className="ui-num">{tr("arbi.col.vitus")}</th>
        </>
      }
      cells={(run) => (
        <>
          <td data-label={tr("common.node")}>
            <span className="flex flex-wrap items-center gap-1.5">
              <span className="font-semibold text-text-primary">{run.node}</span>
              {run.source === "imported" && <Badge>{tr("common.imported")}</Badge>}
              {isIncompleteRun(run) && (
                <Badge tone="warning" title={tr("arbi.incompleteHint")}>
                  <span data-arbi-incomplete>{tr("arbi.incomplete")}</span>
                </Badge>
              )}
              {run.duplicateOf && (
                <Badge title={tr("arbi.duplicateHint")}>
                  <span data-arbi-duplicate>{tr("arbi.duplicate")}</span>
                </Badge>
              )}
              {run.tags?.map((tag) => (
                <Badge key={tag} tone="info">
                  {tag}
                </Badge>
              ))}
            </span>
          </td>
          <td data-label={tr("common.type")}>
            {/* Named mission kinds are game data; only the fallback words are translated. */}
            <Badge tone={TYPE_TONE[run.missionType]}>{missionKindLabel(run) ?? tr(ARBI_MISSION_TYPE_KEYS[run.missionType])}</Badge>
          </td>
          <td className="ui-num whitespace-nowrap text-text-secondary" data-label={tr("common.duration")}>{formatDuration(run.durationSec)}</td>
          <td className="ui-num text-text-secondary" data-label={tr("arbi.col.rotations")}>{run.rotations}</td>
          <td className="ui-num text-text-secondary" data-label={tr("arbi.col.drones")}>{formatInt(run.drones)}</td>
          <td className={`ui-num ${run.vitusActual !== null ? "font-semibold text-text-primary" : "text-text-muted"}`} data-label={tr("arbi.col.vitus")}>
            {run.vitusActual !== null ? formatInt(run.vitusActual) : "–"}
          </td>
        </>
      )}
    />
  );
}
