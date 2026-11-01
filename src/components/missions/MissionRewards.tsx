import { formatNumber } from "../../lib/format";
import type { Translator } from "../../lib/i18n";
import type { MissionRewardSummary } from "../../lib/ipc";
import { endedAtLabel, missionTypeLabel, rewardRowDucats, type RewardRow } from "../../lib/missionRewardRows";
import ItemImage from "../ItemImage";
import { Badge, Readout, StatStrip, type StatItem } from "../ui";

export function MissionMeta({ mission, nodeLabel, locale }: { mission: MissionRewardSummary; nodeLabel: string | null; locale: string }) {
  const typeLabel = missionTypeLabel(mission.missionType);
  return (
    <span className="inline-flex flex-wrap items-baseline gap-x-3">
      <span className="text-xs tabular-nums text-text-muted">{endedAtLabel(mission.endedAt, locale)}</span>
      {typeLabel && (
        <span className="text-xs text-text-muted" data-missions-latest-type>
          {typeLabel}
        </span>
      )}
      {nodeLabel && (
        <span className="min-w-0 truncate text-xs text-text-muted" data-missions-latest-node>
          {nodeLabel}
        </span>
      )}
    </span>
  );
}

export function MissionRewardTotals({ tr, locale, missions, ducats, credits, endo }: { tr: Translator; locale: string; missions?: number; ducats: number; credits: number; endo: number }) {
  const items: StatItem[] = [
    ...(missions === undefined ? [] : [{ key: "missions", label: tr("enemy.missions"), value: formatNumber(missions, locale) }]),
    { key: "ducats", label: tr("missions.ducatValue"), value: <Readout currency="ducat" value={ducats} locale={locale} />, hint: tr("missions.ducatValueHint") },
    { key: "credits", label: tr("common.credits"), value: <Readout currency="credits" value={credits} locale={locale} /> },
    { key: "endo", label: tr("stats.endo"), value: <Readout currency="endo" value={endo} locale={locale} /> },
  ];
  return <StatStrip items={items} />;
}

export function MissionRewardList({ rows, tr, locale, className = "" }: { rows: RewardRow[]; tr: Translator; locale: string; className?: string }) {
  return (
    <>
      {rows.length > 0 && (
        <div className="flex justify-end gap-2 text-xs text-text-muted" data-reward-list-header>
          <span className="w-20 shrink-0 text-right">{tr("common.ducats")}</span>
        </div>
      )}
      <ul className={`m-0 flex list-none flex-col p-0 ${className}`}>
        {rows.map((row) => (
          <li key={row.uniqueName} className="flex min-w-0 items-center gap-2 border-b border-border/40 py-1 text-sm text-text-secondary last:border-b-0" data-reward-row={row.uniqueName}>
            <ItemImage src={row.imageUrl} alt="" cls="!h-8 !w-8 shrink-0" tr={tr} />
            <span className="min-w-0 truncate text-text-primary">{row.name}</span>
            {row.vaulted && (
              <Badge tone="warning" title={tr("common.vaulted")}>
                V
              </Badge>
            )}
            <span className="ml-auto shrink-0 tabular-nums" data-reward-row-count>
              {tr("analysis.unitsShort", { count: row.count })}
            </span>
            <span className="inline-flex w-20 shrink-0 justify-end" data-reward-row-ducats>
              {row.ducats !== null && <Readout currency="ducat" value={row.ducats} locale={locale} />}
            </span>
          </li>
        ))}
      </ul>
    </>
  );
}

/** entry: an expanded history row, whose header already shows the totals. */
export function MissionRewardBody({ mission, rows, tr, locale, variant = "panel" }: { mission: MissionRewardSummary; rows: RewardRow[]; tr: Translator; locale: string; variant?: "panel" | "entry" }) {
  const nothingNew = rows.length === 0 && mission.credits === 0 && mission.endo === 0;
  return (
    <>
      {mission.missionCount > 1 && <p className="m-0 text-xs text-text-muted">{tr("dashboard.lastMission.missionCount", { count: mission.missionCount })}</p>}
      {mission.baselineAt != null && (
        <p className="m-0 text-xs text-text-muted" title={tr("dashboard.lastMission.hint")} data-mission-baseline={mission.baselineAt}>
          {tr("missions.comparedWith", { time: endedAtLabel(mission.baselineAt, locale) })}
        </p>
      )}
      {variant === "entry" ? (
        rows.length > 0 && <MissionRewardList rows={rows} tr={tr} locale={locale} />
      ) : nothingNew ? (
        <p className="m-0 py-3 text-center text-sm text-text-muted" data-missions-latest-nothing>
          {tr("dashboard.lastMission.nothingNew")}
        </p>
      ) : (
        <>
          <MissionRewardTotals tr={tr} locale={locale} ducats={rewardRowDucats(rows)} credits={mission.credits} endo={mission.endo} />
          <MissionRewardList rows={rows} tr={tr} locale={locale} />
        </>
      )}
    </>
  );
}
