import { useMemo } from "react";
import StatusIcon from "./icons/StatusIcon";
import { NavIcon } from "./icons/NavIcon";
import { componentNeeded, componentOwned, componentState, readyCount, sortByState, sourceHint, type ComponentState } from "../lib/componentStatus";
import { formatBuildTime } from "../lib/format";
import { itemLabel } from "../lib/itemLabel";
import type { DropsItemDb } from "../lib/relic/relicDropSources";
import type { RelicDatabase } from "../lib/relic/types";
import type { Translator } from "../lib/i18n";
import type { ComponentInfo, ItemDbEntry } from "../types/inventory";

const TONE: Record<ComponentState, string> = { ready: "text-success", build: "text-warning", partial: "text-warning", missing: "text-danger" };
const compKey = (c: ComponentInfo) => c.uniqueName || c.name;

interface Props {
  components: ComponentInfo[];
  selected: ComponentInfo | null;
  db: Record<string, ItemDbEntry>;
  relicDb: RelicDatabase | null;
  tr: Translator;
  onSelect: (comp: ComponentInfo | null) => void;
}

export default function ComponentStatusList({ components, selected, db, relicDb, tr, onSelect }: Props) {
  const sorted = useMemo(() => sortByState(components), [components]);
  const ready = readyCount(components);
  return (
    <>
      <div className="mb-2">
        <div className="mb-1 text-xs text-text-muted">{tr("detail.partsReady", { ready, total: components.length })}</div>
        <div role="progressbar" aria-valuemin={0} aria-valuenow={ready} aria-valuemax={components.length} aria-label={tr("detail.partsReady", { ready, total: components.length })} className="h-1 overflow-hidden rounded-full bg-surface-hover">
          <div className="h-full bg-success" style={{ width: `${(ready / components.length) * 100}%` }} />
        </div>
      </div>
      <div className="detail-components">
        {sorted.map((comp) => {
          const state = componentState(comp);
          const needed = componentNeeded(comp);
          const owned = componentOwned(comp);
          const on = selected ? compKey(selected) === compKey(comp) : false;
          const time = state === "build" ? (db[comp.uniqueName ?? ""]?.recipe?.buildTime ?? 0) : 0;
          const hint = state === "missing" ? sourceHint(comp, db as DropsItemDb, relicDb) : null;
          const label = {
            ready: tr("detail.partReady"),
            build: time > 0 ? `${tr("detail.partToBuild")} ${formatBuildTime(time)}` : tr("detail.partToBuild"),
            partial: tr("detail.partMissingN", { n: needed - owned }),
            missing: tr("common.missing"),
          }[state];
          return (
            <button
              key={compKey(comp)}
              type="button"
              className={`font-inherit -mx-1 flex w-full cursor-pointer appearance-none items-center justify-between gap-2 border-0 border-b border-dashed border-border-subtle bg-transparent px-1 py-1.5 text-left text-inherit last:border-b-0 hover:rounded-[var(--radius-sm)] hover:bg-surface-hover hover:text-text-primary ${on ? "rounded-[var(--radius-sm)] bg-surface-selected" : ""}`}
              aria-pressed={on}
              data-part-state={state}
              onClick={() => onSelect(on ? null : comp)}
            >
              <span className="comp-name">{itemLabel(comp.uniqueName ?? "", comp) || tr("common.unknown")}</span>
              <span className={`flex shrink-0 items-center gap-1.5 text-xs ${TONE[state]}`} title={comp.blueprintHeld ? tr("common.blueprintOwnedNotBuilt") : undefined}>
                {hint && (
                  <span className="flex items-center gap-1 text-text-muted">
                    {hint.kind !== "drop" && <NavIcon view={hint.kind === "relic" ? "relics" : "market"} className="h-3.5! w-3.5!" />}
                    {hint.kind === "market" ? tr("detail.partMarket") : hint.text}
                  </span>
                )}
                <StatusIcon state={state} />
                <span>{label}</span>
                <span className="comp-count text-text-muted">
                  {owned}/{needed}
                </span>
              </span>
            </button>
          );
        })}
      </div>
    </>
  );
}
