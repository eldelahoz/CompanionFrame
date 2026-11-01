import { useEffect, useMemo, useState } from "react";
import { formatDuration } from "../../lib/arbi/arbiChartData";
import type { Translator } from "../../lib/i18n";
import { missionRewardsPage, onMissionReward, spawnNodesGet, type ItemDatabase, type MissionRewardsPayload } from "../../lib/ipc";
import { buildRewardRows, endedAtLabel, missionName, missionStatusText } from "../../lib/missionRewardRows";
import { arbiRuns, useArbiRuns } from "../../stores/arbiRuns";
import { settingNumber, useWidgetSettings } from "../../stores/dashboard";
import { openView } from "../../stores/nav";
import { MissionMeta, MissionRewardBody } from "../missions/MissionRewards";
import WidgetFrame, { WIDGET_BODY, WIDGET_SUBTITLE } from "./WidgetFrame";
import { INPUT } from "../ui";

export function RecentRunsWidget({ tr, locale }: { tr: Translator; locale: string }) {
  const { runs, loaded } = useArbiRuns();
  const limit = settingNumber(useWidgetSettings("widget.recentRuns"), "limit", 5);
  // The run index is the store's own one-shot read, shared with the Arbitrations tab.
  useEffect(() => {
    if (!arbiRuns.getState().loaded) void arbiRuns.load().catch(() => {});
  }, []);
  const rows = [...runs].sort((a, b) => b.startedAt - a.startedAt).slice(0, limit);
  const date = (ms: number) => new Date(ms).toLocaleDateString(locale, { month: "short", day: "numeric" });
  const grid = "grid grid-cols-[auto_1fr_auto_auto_auto] items-baseline gap-x-3 py-1";
  return (
    <WidgetFrame
      widgetId="widget.recentRuns"
      tr={tr}
      empty={rows.length === 0}
      emptyKey={loaded ? "arbi.empty" : "common.loading"}
      subtitle={<p className={WIDGET_SUBTITLE} data-widget-status>{tr("arbi.runCount", { count: String(runs.length) })}</p>}
    >
      <div className={`${WIDGET_BODY} max-h-[21rem] overflow-y-auto`}>
        <div className={`${grid} text-xs text-text-muted`}>
          <span>{tr("arbi.col.date")}</span>
          <span>{tr("common.node")}</span>
          <span>{tr("common.duration")}</span>
          <span className="text-right">{tr("arbi.col.rotations")}</span>
          <span className="text-right">{tr("arbi.col.vitus")}</span>
        </div>
        {rows.map((run) => (
          <div key={run.id} className={`${grid} text-sm text-text-secondary`} data-widget-run={run.id}>
            <span className="whitespace-nowrap tabular-nums">{date(run.startedAt)}</span>
            <span className="min-w-0 truncate">{run.node || run.missionName}</span>
            <span className="whitespace-nowrap tabular-nums">{formatDuration(run.durationSec)}</span>
            <span className="text-right tabular-nums">{run.rotations}</span>
            <span className="text-right tabular-nums text-text-primary">{run.vitusActual ?? "-"}</span>
          </div>
        ))}
      </div>
    </WidgetFrame>
  );
}

export function LastMissionWidget({ tr, locale, items }: { tr: Translator; locale: string; items: ItemDatabase }) {
  const [payload, setPayload] = useState<MissionRewardsPayload | null>(null);
  const [failed, setFailed] = useState(false);
  const [pickedId, setPickedId] = useState("");
  const [nodeLabels, setNodeLabels] = useState<Map<string, string>>(new Map());

  useEffect(() => {
    let live = true;
    const apply = (next: MissionRewardsPayload) => {
      if (!live) return;
      setPayload(next);
      setFailed(false);
      setPickedId("");
    };
    const off = onMissionReward(apply);
    missionRewardsPage({ offset: 0, limit: 20 }).then(apply, () => live && setFailed(true));
    spawnNodesGet().then((nodes) => live && setNodeLabels(new Map(nodes.map((n) => [n.nodeId, `${n.node} (${n.planet})`])))).catch(() => {});
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, []);

  const summaries = payload?.summaries ?? [];
  const status = payload?.status ?? null;
  const selected = summaries.find((s) => s.id === pickedId) ?? summaries[0] ?? null;
  const rows = useMemo(() => (selected ? buildRewardRows(selected.items, items) : []), [selected, items]);
  const label = (s: (typeof summaries)[number]) => (s.node ? (nodeLabels.get(s.node) ?? null) : null);
  const notice = summaries.length === 0 ? null : missionStatusText(status, tr);
  const emptyKey = status?.blocked === "tracking-off" ? "dashboard.lastMission.trackingOff" : "dashboard.lastMission.none";

  return (
    <WidgetFrame
      widgetId="widget.lastMission"
      tr={tr}
      loading={payload === null && !failed}
      errorKey={failed ? "dashboard.widgetError" : null}
      empty={selected === null}
      emptyKey={emptyKey}
      subtitle={
        <>
          {selected && (
            <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1" title={tr("dashboard.lastMission.hint")} data-widget-status>
              <MissionMeta mission={selected} nodeLabel={label(selected)} locale={locale} />
              {summaries.length > 1 && (
                <select className={`${INPUT} max-w-full`} data-last-mission-picker value={selected.id} onChange={(e) => setPickedId(e.currentTarget.value)}>
                  {summaries.map((s) => (
                    <option key={s.id} value={s.id}>
                      {tr("dashboard.lastMission.pickerOption", { time: endedAtLabel(s.endedAt, locale), mission: missionName(s, label(s), tr("common.unknown")) })}
                    </option>
                  ))}
                </select>
              )}
            </div>
          )}
          {notice && <p className="m-0 text-xs text-text-muted" data-last-mission-status={status?.phase}>{notice}</p>}
        </>
      }
    >
      {selected && (
        <div className={WIDGET_BODY}>
          <MissionRewardBody mission={selected} rows={rows} tr={tr} locale={locale} />
          <button type="button" className="cursor-pointer self-end border-0 bg-transparent p-0 text-xs text-text-muted hover:text-[var(--text-link)] hover:underline" data-last-mission-view-all onClick={() => openView("missions")}>
            {tr("dashboard.lastMission.viewAll")}
          </button>
        </div>
      )}
    </WidgetFrame>
  );
}
