import type { Translator } from "../../lib/i18n";
import { STAT_RESOURCES, statResourceLabelKey } from "../../lib/stats/resources";
import { resetChartResources, setChartResources, toggleChartResource, useChartResources } from "../../stores/statsDisplay";
import { Button, Checkbox, Modal } from "../ui";

export default function StatResourcePicker({ onClose, tr }: { onClose: () => void; tr: Translator }) {
  const selected = new Set(useChartResources());
  return (
    <Modal
      title={tr("stats.chartResources")}
      sub={tr("stats.chartResourcesHint")}
      tr={tr}
      onClose={onClose}
      width="min(46rem, 90vw)"
      footer={
        <>
          <Button size="sm" onClick={() => setChartResources(STAT_RESOURCES.map((r) => r.id))}>{tr("common.selectAll")}</Button>
          <Button size="sm" onClick={resetChartResources}>{tr("common.reset")}</Button>
          <Button size="sm" variant="primary" onClick={onClose}>{tr("common.close")}</Button>
        </>
      }
    >
      <div className="grid gap-2 [grid-template-columns:repeat(auto-fill,minmax(11rem,1fr))]" data-stat-resource-picker>
        {STAT_RESOURCES.map((r) => (
          <Checkbox key={r.id} className="rounded-[var(--radius-md)] border border-border bg-bg-raised px-2.5 py-1.5" label={<span className="truncate">{tr(statResourceLabelKey(r.id))}</span>} checked={selected.has(r.id)} data-stat-resource={r.id} onChange={() => toggleChartResource(r.id)} />
        ))}
      </div>
    </Modal>
  );
}
