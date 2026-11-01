import { askConfirm } from "../stores/confirm";
import { FileX, Trash2 } from "lucide-react";
import type { HTMLAttributes, ReactNode } from "react";
import { formatBytes, formatRunDate } from "../lib/arbi/arbiChartData";
import type { Translator } from "../lib/i18n";
import { Checkbox, IconButton, Table } from "./ui";

interface BaseRun {
  id: string;
  startedAt: number;
  hasLog: boolean;
  logSizeBytes: number;
}

interface Props<T extends BaseRun> {
  runs: T[];
  onSelect: (id: string) => void;
  headers: ReactNode;
  cells: (run: T) => ReactNode;
  onDelete: (id: string) => void;
  onDeleteLog: (id: string) => void;
  tr: Translator;
  tableAttrs?: HTMLAttributes<HTMLDivElement>;
  rowAttrs?: (run: T) => Record<string, string>;
  /** Present: a checkbox column driven by these. */
  selected?: ReadonlySet<string>;
  onToggleSelect?: (id: string) => void;
  onToggleSelectAll?: () => void;
}

/** The table shell both run lists share: date, mode-specific cells, log size and delete actions. */
export default function RunList<T extends BaseRun>({ runs, onSelect, headers, cells, onDelete, onDeleteLog, tr, tableAttrs, rowAttrs, selected, onToggleSelect, onToggleSelectAll }: Props<T>) {
  const allSelected = !!selected && runs.length > 0 && runs.every((r) => selected.has(r.id));
  return (
    <div {...tableAttrs}>
      <Table mode="stack" label={tr("arbi.col.date")}>
        <thead>
          <tr>
            {selected && (
              <th className="w-8">
                <Checkbox checked={allSelected} title={tr("common.selectAll")} aria-label={tr("common.selectAll")} onChange={onToggleSelectAll} />
              </th>
            )}
            <th>{tr("arbi.col.date")}</th>
            {headers}
            <th className="ui-num">{tr("arbi.col.log")}</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {runs.map((run) => (
            <tr key={run.id} className="cursor-pointer" onClick={() => onSelect(run.id)} {...rowAttrs?.(run)}>
              {selected && (
                <td>
                  <Checkbox checked={selected.has(run.id)} aria-label={tr("arbi.selectRun")} onClick={(e) => e.stopPropagation()} onChange={() => onToggleSelect?.(run.id)} />
                </td>
              )}
              <td data-primary className="whitespace-nowrap text-text-secondary">{formatRunDate(run.startedAt)}</td>
              {cells(run)}
              <td className="ui-num whitespace-nowrap text-text-muted" data-label={tr("arbi.col.log")}>{run.hasLog ? formatBytes(run.logSizeBytes) : "–"}</td>
              <td data-actions className="whitespace-nowrap text-right">
                {run.hasLog && (
                  <IconButton
                    label={tr("arbi.deleteLog")}
                    onClick={(e) => {
                      e.stopPropagation();
                      onDeleteLog(run.id);
                    }}
                  >
                    <FileX />
                  </IconButton>
                )}
                <IconButton
                  danger
                  label={tr("arbi.deleteRun")}
                  onClick={async (e) => {
                    e.stopPropagation();
                    if (await askConfirm(tr("arbi.confirmDeleteRun"))) onDelete(run.id);
                  }}
                >
                  <Trash2 />
                </IconButton>
              </td>
            </tr>
          ))}
        </tbody>
      </Table>
    </div>
  );
}
