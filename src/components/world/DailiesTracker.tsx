import { formatInt } from "../../lib/format";
import { useEffect, useMemo, useState } from "react";
import type { MessageKey, Translator } from "../../lib/i18n";
import type { WorldState } from "../../lib/ipc";
import { itemLabel } from "../../lib/itemLabel";
import { nextDailyResetUtc, nextWeeklyResetUtc, parseIsoDate, timeTo } from "../../lib/format";
import { buildWikiUrl } from "../../lib/wikiUrl";
import {
  addCustomTask,
  expiryPeriodKey,
  loadTracker,
  pruneDynamicProgress,
  removeCustomTask,
  resetTrackerCustomization,
  saveTracker,
  setTrackerCount,
  setTrackerPeriod,
  setTrackerTarget,
  toggleTrackerHidden,
  trackerCount,
  trackerGroup,
  trackerList,
  trackerPeriodKey,
  type TrackerGroup,
  type TrackerState,
  type TrackerUserPeriod,
} from "../../lib/world/dailies";
import { autoTrackerState, nightwaveSeasonStanding } from "../../lib/world/dailiesAuto";
import { codaBatch, TENET_MELEE_STOCK, trackerExpiries, trackerLive } from "../../lib/world/dailiesLive";
import { buildBaroOwnedSet } from "../../lib/world/baroPlanner";
import { itemsByName } from "../../lib/world/itemsByName";
import { useInventory } from "../../stores/inventory";
import { useItemDatabase } from "../../stores/items";
import { openItemBy } from "../../stores/modals";
import { useClock, useCollapsedSections } from "../../stores/world";
import { worldView } from "../../stores/worldView";
import { ChevronDown, ExternalLink, Minus, Plus, X } from "lucide-react";
import { Badge, Button, Checkbox, IconButton, Input, Progress, SearchInput, Select } from "../ui";
import WorldCard from "./WorldCard";
import IconButtonCard from "./IconButtonCard";
import VendorRotations from "./VendorRotations";

const GROUP_TITLES: Record<TrackerGroup, MessageKey> = {
  daily: "dailies.groupDaily",
  nightwave: "dailies.groupNightwave",
  weekly: "dailies.groupWeekly",
  vendors: "dailies.groupVendors",
  alerts: "common.alerts",
};
const NIGHTWAVE_HEADER_KEYS: Record<string, MessageKey> = { nwDaily: "dailies.nwDaily", nwWeekly: "dailies.nwWeekly", nwElite: "dailies.nwElite" };
/** Under an hour reads as warning, under ten minutes as danger. */
const URGENT_MS = 60 * 60_000;
const CRITICAL_MS = 10 * 60_000;
const VENDOR_ROWS: Record<string, "coda" | "tenet" | undefined> = { codaWeapons: "coda", tenetMelee: "tenet" };

type CalendarDay = NonNullable<WorldState["calendarSeason"]>["days"][number];

interface Icon {
  key: string;
  label: string;
  imageUrl: string | null | undefined;
  owned: boolean;
  uniqueName: string | null;
}

interface Row {
  kind: "task" | "header";
  id: string;
  label: string;
  detail?: string | undefined;
  lines?: string[] | undefined;
  icons?: Icon[] | undefined;
  vendor?: "coda" | "tenet" | undefined;
  calendar?: CalendarDay[] | undefined;
  badge?: string | undefined;
  progress?: { current: number; required: number } | undefined;
  group: TrackerGroup;
  periodKey: string | null;
  count: number;
  target: number;
  done: boolean;
  /** Completion came from the inventory sync, so the checkbox is read-only. */
  auto: boolean;
  autoCount: number;
  hidden: boolean;
  custom: boolean;
  /** Nightwave acts and alerts rotate away, so they are not customisable. */
  dynamic: boolean;
  retimeable: boolean;
  period: string;
  wiki?: string | undefined;
  expiry?: string | null | undefined;
}

const rank = (isDaily: boolean, isElite: boolean) => (isDaily ? 0 : isElite ? 2 : 1);

interface Props {
  wd: WorldState | null;
  tr: Translator;
}

/** Daily/weekly checklist: built-in tasks plus live Nightwave acts and alerts; ticks persist locally, inventory sync auto-completes what it can. */
export default function DailiesTracker({ wd, tr }: Props) {
  const db = useItemDatabase();
  const { data: inventory, status } = useInventory();
  const nowMs = useClock(1000);
  const [collapsed, toggleSection] = useCollapsedSections();
  const [tracker, setTracker] = useState<TrackerState>(loadTracker);
  const [editing, setEditing] = useState(false);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [draftLabel, setDraftLabel] = useState("");
  const [draftPeriod, setDraftPeriod] = useState<TrackerUserPeriod>("daily");
  const [query, setQuery] = worldView.useField("dailiesQuery");

  useEffect(() => {
    const refresh = () => setTracker(loadTracker());
    window.addEventListener("storage", refresh);
    return () => window.removeEventListener("storage", refresh);
  }, []);

  const now = new Date(nowMs);
  const byName = useMemo(() => itemsByName(db), [db]);
  const owned = useMemo(() => buildBaroOwnedSet(inventory), [inventory]);
  const modifiedAt = status?.modifiedAt ?? null;
  const auto = useMemo(() => autoTrackerState(inventory, wd, nowMs, modifiedAt), [inventory, wd, nowMs, modifiedAt]);

  const iconsFromNames = (names: string[]): Icon[] =>
    names.map((name) => {
      const e = byName.get(name);
      return { key: e?.uniqueName ?? name, label: e ? itemLabel(e.uniqueName, e) : name, imageUrl: e?.imageUrl, owned: e ? owned.has(e.uniqueName) : false, uniqueName: e?.uniqueName ?? null };
    });
  const iconsFromUniqueNames = (uniqueNames: string[]): Icon[] =>
    uniqueNames.map((u) => ({ key: u, label: itemLabel(u, db[u]), imageUrl: db[u]?.imageUrl, owned: owned.has(u), uniqueName: db[u] ? u : null }));
  const circuit = (category: string) => wd?.duviriCycle?.choices.find((c) => c.category === category)?.choices ?? [];

  const rows = useMemo<Row[]>(() => {
    const expiries = trackerExpiries(wd);
    const taskRow = (base: Partial<Row> & Pick<Row, "id" | "label" | "group" | "periodKey">): Row => {
      const target = base.target ?? 1;
      const a = auto[base.id];
      const autoCount = a?.count ?? 0;
      const count = Math.max(trackerCount(tracker, base.id, base.periodKey, nowMs), autoCount);
      return { kind: "task", count, target, done: count >= target, auto: autoCount >= target, autoCount, progress: a?.progress, hidden: tracker.hidden.includes(base.id), custom: false, dynamic: false, retimeable: false, period: base.period ?? "", ...base };
    };
    const strips: Record<string, Icon[]> = {
      circuitNormal: iconsFromNames(circuit("normal")),
      circuitSteelPath: iconsFromNames(circuit("hard")),
      varzia: iconsFromUniqueNames((wd?.vaultTrader?.inventory ?? []).map((o) => o.uniqueName).filter(Boolean)),
      darvo: iconsFromUniqueNames((wd?.dailyDeals ?? []).map((d) => d.uniqueName).filter(Boolean)),
    };

    const builtin = trackerList(tracker).map((task): Row => {
      const live = task.label ? {} : trackerLive(task.id, wd, tr, nowMs);
      const autoDetail = auto[task.id]?.detail;
      const row = taskRow({
        id: task.id,
        label: task.label ?? (task.labelKey ? tr(task.labelKey) : task.id),
        group: trackerGroup(task.period, task.group),
        periodKey: trackerPeriodKey(task.period, now, expiries),
        target: task.target,
        wiki: task.wiki,
        period: task.period,
      });
      const icons = strips[task.id] ?? [];
      return {
        ...row,
        custom: Boolean(task.label),
        retimeable: task.period === "daily" || task.period === "weekly",
        detail: live.detail ?? (autoDetail ? tr(autoDetail.key, autoDetail.params) : undefined),
        lines: live.lines?.length ? live.lines : undefined,
        icons: icons.length > 0 ? icons : undefined,
        vendor: VENDOR_ROWS[task.id],
        calendar: live.calendar?.length ? live.calendar : undefined,
        expiry: live.expiry,
      };
    });

    const nightwave: Row[] = [];
    const season = wd?.nightwave;
    if (season && season.challenges.length > 0) {
      let lastHeader = "";
      for (const act of [...season.challenges].sort((a, b) => rank(a.isDaily, a.isElite) - rank(b.isDaily, b.isElite))) {
        const header = act.isDaily ? "nwDaily" : act.isElite ? "nwElite" : "nwWeekly";
        if (header !== lastHeader) {
          lastHeader = header;
          nightwave.push({ ...taskRow({ id: `nwhead:${header}`, label: "", group: "nightwave", periodKey: null }), kind: "header", label: tr(NIGHTWAVE_HEADER_KEYS[header]) });
        }
        nightwave.push({
          ...taskRow({ id: `nw:${act.id}`, label: act.title, group: "nightwave", periodKey: expiryPeriodKey("nw", act.expiry) }),
          dynamic: true,
          detail: act.description || tr("dailies.objectiveUnavailable"),
          badge: tr("dailies.standing", { amount: act.standing == null ? tr("common.unknown") : formatInt(act.standing) }),
          expiry: act.expiry,
        });
      }
    }

    const alerts = (wd?.alerts ?? []).map((alert): Row => {
      const rewards = [
        ...alert.items.map((i) => (i.count > 1 ? `${i.count}x ${i.name}` : i.name)),
        alert.credits > 0 ? tr("world.creditsAmount", { amount: formatInt(alert.credits) }) : "",
      ].filter(Boolean);
      return {
        ...taskRow({ id: `alert:${alert.id}`, label: `${alert.mission} - ${alert.node}`, group: "alerts", periodKey: expiryPeriodKey("alert", alert.expiry) }),
        dynamic: true,
        detail: rewards.join(" - "),
        badge: tr("dailies.levelRange", { min: String(alert.minLevel), max: String(alert.maxLevel) }),
        expiry: alert.expiry,
      };
    });
    return [...builtin, ...nightwave, ...alerts];
  }, [wd, tracker, auto, nowMs, tr, byName, owned, db]);

  const seasonStanding = nightwaveSeasonStanding(inventory, wd?.nightwave?.affiliationTag);

  const commit = (next: TrackerState) => {
    // No world state at all means the empty live lists prove nothing; a present one with none genuinely means those rows are gone.
    const liveIds = new Set(rows.filter((r) => r.kind === "task").map((r) => r.id));
    const pruned = wd ? pruneDynamicProgress(next, liveIds) : next;
    setTracker(pruned);
    saveTracker(pruned);
  };

  const matches = (row: Row, needle: string) =>
    !needle ||
    row.label.toLowerCase().includes(needle) ||
    (row.detail ?? "").toLowerCase().includes(needle) ||
    !!row.lines?.some((l) => l.toLowerCase().includes(needle)) ||
    !!row.icons?.some((i) => i.label.toLowerCase().includes(needle)) ||
    !!(row.vendor && (row.vendor === "coda" ? codaBatch(nowMs).weapons : TENET_MELEE_STOCK).some((n) => n.toLowerCase().includes(needle))) ||
    !!row.calendar?.some((d) => d.events.some((e) => e.label.toLowerCase().includes(needle)));

  const groupRows = (group: TrackerGroup): Row[] => {
    if (tracker.hidden.includes(`section:${group}`) && !editing) return [];
    const needle = query.trim().toLowerCase();
    const visible = rows.filter((r) => r.group === group && (editing || r.kind === "header" || !r.hidden) && (r.kind === "header" || matches(r, needle)));
    // A header left with no task under it would render as a stray label.
    return visible.filter((r, i) => r.kind === "task" || visible[i + 1]?.kind === "task");
  };

  const countdown = (expiry: string | null | undefined) => {
    const d = parseIsoDate(expiry ?? null);
    return d ? timeTo(d, nowMs) : "";
  };
  const remainingMs = (expiry: string | null | undefined) => {
    const d = parseIsoDate(expiry ?? null);
    return d ? d.getTime() - nowMs : Number.POSITIVE_INFINITY;
  };
  const groupReset = (group: TrackerGroup) => {
    if (group === "daily") return tr("dailies.resetsIn", { time: timeTo(nextDailyResetUtc(now), nowMs) });
    if (group === "weekly") return tr("dailies.resetsIn", { time: timeTo(nextWeeklyResetUtc(now), nowMs) });
    if (group === "nightwave" && wd?.nightwave) {
      const ends = tr("dailies.seasonEnds", { season: String(wd.nightwave.season), time: countdown(wd.nightwave.expiry) });
      return seasonStanding === null ? ends : `${ends}, ${tr("dailies.seasonStanding", { amount: formatInt(seasonStanding) })}`;
    }
    return "";
  };

  const toggleDone = (row: Row) => !row.auto && commit(setTrackerCount(tracker, row.id, row.periodKey, row.done ? 0 : row.target));
  const bump = (row: Row, delta: number) => commit(setTrackerCount(tracker, row.id, row.periodKey, Math.max(row.autoCount, Math.min(row.count + delta, row.target))));
  const addDraft = () => {
    const next = addCustomTask(tracker, draftLabel, draftPeriod);
    if (next === tracker) return;
    setDraftLabel("");
    commit(next);
  };
  const openItem = (uniqueName: string | null) => uniqueName && openItemBy({ uniqueName });

  const renderRow = (row: Row) => {
    if (row.kind === "header") return <p key={row.id} className="m-0 px-3 pb-1 pt-3 text-xs font-semibold text-text-muted">{row.label}</p>;
    const left = remainingMs(row.expiry);
    // No period key means the tick would never be read back, so the row waits.
    const locked = row.periodKey === null && !row.custom;
    const hasMore = !!(row.lines || row.icons || row.calendar || row.vendor);
    const hideLabel = row.hidden ? tr("dailies.showTask") : tr("dailies.hideTask");
    return (
      <div key={row.id}>
        <div className={`flex flex-wrap items-center gap-2 border-t border-border px-3 py-2 ${row.hidden ? "opacity-45" : ""}`}>
          <label className="flex min-w-0 flex-1 cursor-pointer items-start gap-2" title={row.auto ? tr("dailies.autoTracked") : undefined}>
            <Checkbox className="mt-0.5" checked={row.done} disabled={row.auto || locked} data-task={row.id} onChange={() => toggleDone(row)} />
            <span className="min-w-0">
              <span className={`block text-sm ${row.done ? "text-text-muted line-through" : "text-text-primary"}`}>{row.label}</span>
              {row.detail && <span className="block text-xs text-text-secondary">{row.detail}</span>}
            </span>
          </label>
          {row.progress && !row.done && <span className="shrink-0 text-xs tabular-nums text-text-secondary">{row.progress.current}/{row.progress.required}</span>}
          {row.badge && <Badge>{row.badge}</Badge>}
          {row.target > 1 && (
            <span className="flex shrink-0 items-center gap-1">
              <IconButton label={tr("dailies.decrement")} disabled={locked || row.count <= row.autoCount} data-task-dec={row.id} onClick={() => bump(row, -1)}><Minus aria-hidden="true" /></IconButton>
              <span className="w-10 text-center text-xs tabular-nums text-text-primary">{row.count}/{row.target}</span>
              <IconButton label={tr("dailies.increment")} disabled={locked || row.count >= row.target} data-task-inc={row.id} onClick={() => bump(row, 1)}><Plus aria-hidden="true" /></IconButton>
            </span>
          )}
          {row.expiry && <span className={`shrink-0 text-xs tabular-nums ${left < CRITICAL_MS ? "text-danger" : left < URGENT_MS ? "text-warning" : "text-text-secondary"}`}>{countdown(row.expiry)}</span>}
          {hasMore && (
            <IconButton label={expanded[row.id] ? tr("dailies.collapse") : tr("dailies.expand")} aria-expanded={!!expanded[row.id]} data-task-expand={row.id} onClick={() => setExpanded({ ...expanded, [row.id]: !expanded[row.id] })}>
              <ChevronDown className={expanded[row.id] ? "" : "-rotate-90"} aria-hidden="true" />
            </IconButton>
          )}
          {row.wiki && (
            <a className="ui-icon-btn" href={buildWikiUrl(row.wiki)} target="_blank" rel="noreferrer noopener" data-tip={tr("dailies.openWiki")} aria-label={tr("dailies.openWiki")}><ExternalLink aria-hidden="true" /></a>
          )}
          {editing && !row.dynamic && (
            <>
              {row.retimeable && (
                <Select title={tr("dailies.periodTitle")} aria-label={tr("dailies.periodTitle")} value={row.period} options={[["daily", tr("dailies.groupDaily")], ["weekly", tr("dailies.groupWeekly")]]} onChange={(e) => commit(setTrackerPeriod(tracker, row.id, e.target.value === "weekly" ? "weekly" : "daily"))} />
              )}
              <IconButton label={hideLabel} data-task-hide={row.id} onClick={() => commit(toggleTrackerHidden(tracker, row.id))}>{row.hidden ? <Plus aria-hidden="true" /> : <X aria-hidden="true" />}</IconButton>
              {row.custom && (
                <>
                  <Input className="!w-16" type="number" min={1} max={99} title={tr("dailies.targetTitle")} aria-label={tr("dailies.targetTitle")} defaultValue={row.target} onChange={(e) => commit(setTrackerTarget(tracker, row.id, Number(e.target.value)))} />
                  <IconButton danger label={tr("dailies.removeTask")} data-task-remove={row.id} onClick={() => commit(removeCustomTask(tracker, row.id))}><Minus aria-hidden="true" /></IconButton>
                </>
              )}
            </>
          )}
        </div>
        {expanded[row.id] && (
          <div className="px-3 pb-2">
            {row.lines && <ul className="m-0 pl-8 text-xs text-text-secondary">{row.lines.map((line) => <li key={line}>{line}</li>)}</ul>}
            {row.vendor && <VendorRotations vendors={[row.vendor]} tr={tr} />}
            {row.icons && (
              <div className="flex flex-wrap gap-2 py-1 pl-8">
                {row.icons.map((i) => <IconButtonCard key={i.key} name={i.label} imageUrl={i.imageUrl} owned={i.owned} onClick={() => openItem(i.uniqueName)} tr={tr} />)}
              </div>
            )}
            {row.calendar && (
              <div className="grid grid-cols-1 gap-2 pl-8 sm:grid-cols-2">
                {row.calendar.map((day) => {
                  const perks = day.events.filter((e) => e.kind === "upgrade");
                  return (
                    <div key={day.day} className="rounded-[var(--radius-md)] border border-border p-2 text-xs">
                      <p className="m-0 font-bold text-text-primary">{tr("dailies.calendarDay", { day: String(day.day) })}</p>
                      {day.events.map((e, i) =>
                        e.kind === "reward" ? (
                          <button key={i} type="button" className="ui-chip mr-1 mt-1 !h-auto py-0.5" onClick={() => openItem(e.uniqueName && db[e.uniqueName] ? e.uniqueName : null)}>{e.label}</button>
                        ) : e.kind === "challenge" ? (
                          <p key={i} className="m-0 mt-1 text-text-primary">{e.label}{e.description && <span className="block text-text-secondary">{e.description}</span>}</p>
                        ) : null,
                      )}
                      {perks.length > 0 && <p className="m-0 mt-1 text-text-secondary">{tr("dailies.calendarPerks")}: {perks.map((p) => p.label).join(", ")}</p>}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        )}
      </div>
    );
  };

  const renderSection = (group: TrackerGroup) => {
    const list = groupRows(group);
    if (list.length === 0) return null;
    const sectionHidden = tracker.hidden.includes(`section:${group}`);
    const shown = rows.filter((r) => r.kind === "task" && r.group === group && !r.hidden);
    const done = shown.filter((r) => r.done).length;
    const reset = groupReset(group);
    const hideLabel = sectionHidden ? tr("dailies.showTask") : tr("dailies.hideTask");
    return (
      <div key={group} className={sectionHidden ? "opacity-45" : ""}>
        <WorldCard title={tr(GROUP_TITLES[group])} collapsed={!!collapsed[`dailies-${group}`]} onToggle={() => toggleSection(`dailies-${group}`)} flush>
          <div className="flex items-center gap-3 px-4 py-2" data-task-meta={group}>
            <span className={`shrink-0 text-xs tabular-nums ${shown.length > 0 && done === shown.length ? "text-success" : "text-text-secondary"}`} title={tr("dailies.groupProgress", { done: String(done), total: String(shown.length) })}>{done}/{shown.length}</span>
            <div className="min-w-8 flex-1"><Progress value={(done / Math.max(1, shown.length)) * 100} label={tr("dailies.groupProgress", { done: String(done), total: String(shown.length) })} tone={shown.length > 0 && done === shown.length ? "success" : "accent"} /></div>
            {reset && <span className="text-xs text-text-secondary">{reset}</span>}
            {editing && (group === "nightwave" || group === "alerts") && (
              <IconButton label={hideLabel} onClick={() => commit(toggleTrackerHidden(tracker, `section:${group}`))}>{sectionHidden ? <Plus aria-hidden="true" /> : <X aria-hidden="true" />}</IconButton>
            )}
          </div>
          {list.map(renderRow)}
        </WorldCard>
      </div>
    );
  };

  return (
    <div className="@container flex flex-col" data-dailies-tracker>
      <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
        <p className="m-0 text-sm text-text-secondary">{!wd ? tr("dailies.awaitingWorldData") : inventory ? tr("dailies.autoHint") : tr("dailies.manualHint")}</p>
        <div className="flex items-center gap-3">
          {editing && tracker.hidden.length > 0 && <span className="text-xs text-text-muted">{tr("dailies.hiddenCount", { count: String(tracker.hidden.length) })}</span>}
          {editing && <Button size="sm" data-tracker-reset onClick={() => commit(resetTrackerCustomization(tracker))}>{tr("settings.resetDefaults")}</Button>}
          <SearchInput label={tr("common.searchPlaceholder")} className="w-48" placeholder={tr("common.searchPlaceholder")} data-tracker-search value={query} onChange={(e) => setQuery(e.target.value)} />
          <Button size="sm" data-tracker-edit aria-pressed={editing} onClick={() => setEditing(!editing)}>{editing ? tr("dailies.customizeDone") : tr("common.customize")}</Button>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-4 @4xl:grid-cols-2">
        <div className="flex flex-col gap-4">{(["daily", "nightwave", "alerts"] as const).map(renderSection)}</div>
        <div className="flex flex-col gap-4">{(["weekly", "vendors"] as const).map(renderSection)}</div>
      </div>
      {editing && (
        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Input className="!w-56" type="text" maxLength={60} placeholder={tr("dailies.addPlaceholder")} aria-label={tr("dailies.addPlaceholder")} data-task-name value={draftLabel} onChange={(e) => setDraftLabel(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addDraft()} />
          <Select title={tr("dailies.periodTitle")} aria-label={tr("dailies.periodTitle")} value={draftPeriod} options={[["daily", tr("dailies.groupDaily")], ["weekly", tr("dailies.groupWeekly")]]} onChange={(e) => setDraftPeriod(e.target.value === "weekly" ? "weekly" : "daily")} />
          <Button size="sm" disabled={!draftLabel.trim()} data-task-add onClick={addDraft}>{tr("dailies.addTask")}</Button>
        </div>
      )}
    </div>
  );
}
