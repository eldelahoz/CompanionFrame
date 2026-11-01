import { Card } from "../ui";
import type { Translator } from "../../lib/i18n";
import { archonShardColorKey, shardKindKey, type ArchonShardSummary } from "../../lib/inventory/archonShards";
import type { ItemDbEntry } from "../../types/inventory";
import ArchonShardPips from "./ArchonShardPips";

interface Props {
  summary: ArchonShardSummary;
  itemDb: Record<string, ItemDbEntry>;
  tr: Translator;
  locale: string;
  /** Resolves a Warframe uniqueName to the label the rest of the UI shows. */
  frameLabel: (itemType: string) => string;
  onOpenFrame?: (itemType: string) => void;
}

export default function ArchonShardSummaryPanel({ summary, itemDb, tr, locale, frameLabel, onOpenFrame }: Props) {
  return (
    <Card className="ms-archon" data-archon-summary>
      <div className="ms-archon-head">
        <span className="ms-eyebrow">{tr("archon.title")}</span>
        <span className="ms-sub ms-sub--muted">
          {tr("archon.installedCount", { count: summary.installed })}, {tr("archon.unsocketedCount", { count: summary.unsocketed })}
        </span>
        {summary.unknownInstalled > 0 && <span className="ms-sub ms-sub--muted">{tr("archon.unknownInstalled", { count: summary.unknownInstalled })}</span>}
      </div>
      {summary.stock.map((row) => (
        <div key={shardKindKey(row.color, row.tauforged)} className="ms-archon-row" data-archon-stock={shardKindKey(row.color, row.tauforged)}>
          <ArchonShardPips slots={[{ index: 0, color: row.color, tauforged: row.tauforged, filled: true, upgradeType: null }]} itemDb={itemDb} tr={tr} size="md" />
          {/* Stock is keyed by colour AND tauforged, so the label carries both. */}
          <span className="ms-archon-kind">
            {tr(archonShardColorKey(row.color))}
            {row.tauforged ? `, ${tr("archon.tauforged")}` : ""}
          </span>
          <b className="ms-archon-total">{row.total.toLocaleString(locale)}</b>
          <span className="ms-archon-holders">
            <span className="whitespace-nowrap">
              {tr("archon.installedCount", { count: row.installed })}, {tr("archon.unsocketedCount", { count: row.unsocketed })}
            </span>
            {row.holders.map((h, i) => (
              <button key={h.instanceId ?? `${h.itemType}-${i}`} type="button" className="ms-chip" data-archon-holder={h.itemType} onClick={() => onOpenFrame?.(h.itemType)}>
                {frameLabel(h.itemType)}
                {h.count > 1 ? ` x${h.count}` : ""}
              </button>
            ))}
          </span>
        </div>
      ))}
    </Card>
  );
}
