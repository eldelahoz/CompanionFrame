import { useEffect, useMemo, useState } from "react";
import type { Translator } from "../../lib/i18n";
import type { WorldState } from "../../lib/ipc";
import { nextDailyResetUtc, nextWeeklyResetUtc, parseIsoDate, timeTo } from "../../lib/format";
import { loadTracker, trackerCount, trackerList, trackerPeriodKey } from "../../lib/world/dailies";
import { autoTrackerState } from "../../lib/world/dailiesAuto";
import { trackerExpiries, trackerLive } from "../../lib/world/dailiesLive";
import { useInventory } from "../../stores/inventory";
import { Button, Card, CardBody, CardHeader } from "../ui";

interface Props {
  wd: WorldState;
  nowMs: number;
  tr: Translator;
  onOpenDailies: () => void;
}

const HIGHLIGHTED = new Set(["archonHunt", "circuitNormal", "circuitSteelPath", "netracells", "steelPathHonors"]);

/** The week's long-running tasks at a glance; ticks are the Dailies tracker's state. */
export default function WorldWeekOverview({ wd, nowMs, tr, onOpenDailies }: Props) {
  const { data: inventory, status } = useInventory();
  const [tracker, setTracker] = useState(loadTracker);
  useEffect(() => {
    const refresh = () => setTracker(loadTracker());
    window.addEventListener("storage", refresh);
    return () => window.removeEventListener("storage", refresh);
  }, []);

  const now = new Date(nowMs);
  const modifiedAt = status?.modifiedAt ?? null;
  const rows = useMemo(() => {
    const at = new Date(nowMs);
    const expiries = trackerExpiries(wd);
    const auto = autoTrackerState(inventory, wd, nowMs, modifiedAt);
    return trackerList(tracker)
      .filter((task) => HIGHLIGHTED.has(task.id) && !tracker.hidden.includes(task.id))
      .map((task) => {
        const live = trackerLive(task.id, wd, tr, nowMs);
        const count = Math.max(trackerCount(tracker, task.id, trackerPeriodKey(task.period, at, expiries), nowMs), auto[task.id]?.count ?? 0);
        const detail = auto[task.id]?.detail;
        return {
          task,
          count: Math.min(task.target, count),
          done: count >= task.target,
          detail: live.detail ?? (detail ? tr(detail.key, detail.params) : ""),
          expiry: parseIsoDate(live.expiry) ?? (task.period === "daily" ? nextDailyResetUtc(at) : task.period === "weekly" ? nextWeeklyResetUtc(at) : null),
        };
      });
  }, [tracker, wd, inventory, modifiedAt, nowMs, tr]);

  return (
    <Card data-world-week-overview>
      <CardHeader
        title={tr("world.thisWeek")}
        meta={`${tr("world.dailyReset")}: ${timeTo(nextDailyResetUtc(now), nowMs)}, ${tr("world.weeklyResets")}: ${timeTo(nextWeeklyResetUtc(now), nowMs)}`}
        tools={<Button size="sm" data-world-open-dailies onClick={onOpenDailies}>{tr("world.openDailies")}</Button>}
      />
      <CardBody className="grid grid-cols-1 gap-3 @xl:grid-cols-2 @4xl:grid-cols-5">
        {rows.map(({ task, count, done, detail, expiry }) => (
          <button
            key={task.id}
            type="button"
            className="flex min-w-0 cursor-pointer flex-col rounded-[var(--radius-md)] border border-border bg-bg-raised p-3 text-left hover:border-border-strong"
            data-world-week-task={task.id}
            data-world-task-done={done}
            onClick={onOpenDailies}
          >
            <span className="flex items-start justify-between gap-2">
              <span className="text-sm font-semibold text-text-primary">{task.labelKey ? tr(task.labelKey) : task.label}</span>
              <span className={`shrink-0 text-xs tabular-nums ${done ? "text-success" : "text-text-secondary"}`} data-world-task-progress>{count}/{task.target}</span>
            </span>
            {detail && <span className="mt-1.5 text-xs text-text-secondary">{detail}</span>}
            {expiry && <span className="mt-2 text-xs text-text-muted">{tr("dailies.resetsIn", { time: timeTo(expiry, nowMs) })}</span>}
          </button>
        ))}
      </CardBody>
    </Card>
  );
}
