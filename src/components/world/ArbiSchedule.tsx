import { askConfirm } from "../../stores/confirm";
import { AlarmClock, Bell, BellRing, Check, Copy } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Badge, Button, Card, CardBody, CardHeader, Checkbox, EmptyState, IconButton, Input, SearchInput, Select, Skeleton, Table } from "../ui";
import type { Translator } from "../../lib/i18n";
import { arbiScheduleGet, arbiScheduleSetFavorite, arbiScheduleSetLead, arbiScheduleSetOccurrence, type ArbiScheduleAlerts, type ArbiScheduleEntry } from "../../lib/ipc";
import { renderArbiCard, timezoneLabel } from "../../lib/world/arbiCard";
import {
  buildNodeCatalog, buildSearchState, factionBadgeKey, filterScheduleEntries, formatEntryTime, formatScheduleCountdown, formatUpdatedAgo, groupEntriesByDay,
  loadDaysToShow, loadSelectedNodeIds, loadSelectionPresets, matchesSearch, saveDaysToShow, saveSelectedNodeIds, saveSelectionPresets, scheduleEntryKey, searchUnmatchedFeedback,
  type ArbiSelectionPreset,
} from "../../lib/world/arbiScheduleData";
import { useClock } from "../../stores/world";

const DAY_OPTIONS = [7, 14, 30, 60].map((d) => [d, String(d)] as const);
const MAX_COPY_ROWS = 80;
const FACTION_TONE = { grineer: "danger", corpus: "info", infested: "success", corrupted: "warning", other: "neutral" } as const;
const FACTION_DOT = { grineer: "var(--world-faction-grineer)", corpus: "var(--world-faction-corpus)", infested: "var(--world-faction-infested)", corrupted: "var(--warning)", other: "var(--text-muted)" } as const;
type CopyState = "idle" | "busy" | "done" | "error" | "max";

/** Arbitration schedule: pick nodes, see the upcoming rotations, bell an occurrence, star a node, copy rows as an image. */
export default function ArbiSchedule({ tr, locale }: { tr: Translator; locale: string }) {
  const nowMs = useClock(1000);
  const [entries, setEntries] = useState<ArbiScheduleEntry[]>([]);
  const [alerts, setAlerts] = useState<ArbiScheduleAlerts>({ occurrences: [], favoriteNodes: [], minutesBefore: 5 });
  const [fetchedAt, setFetchedAt] = useState<number | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [loadFailed, setLoadFailed] = useState(false);
  const [selected, setSelected] = useState(loadSelectedNodeIds);
  const [presets, setPresets] = useState<ArbiSelectionPreset[]>(loadSelectionPresets);
  const [presetName, setPresetName] = useState("");
  const [presetSelected, setPresetSelected] = useState("");
  const [presetStatus, setPresetStatus] = useState("");
  const [searchRaw, setSearchRaw] = useState("");
  const [days, setDays] = useState(loadDaysToShow);
  const [copySelection, setCopySelection] = useState<ReadonlySet<string>>(new Set());
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const flash = useRef<ReturnType<typeof setTimeout>>(undefined);
  useEffect(() => () => clearTimeout(flash.current), []);

  const refresh = useCallback(async () => {
    try {
      const p = await arbiScheduleGet();
      setEntries(p.entries);
      setAlerts(p.alerts);
      setFetchedAt(p.fetchedAt);
      setLoadFailed(p.entries.length === 0);
    } catch {
      setLoadFailed(true);
    } finally {
      setLoaded(true);
    }
  }, []);
  useEffect(() => void refresh(), [refresh]);

  const catalog = useMemo(() => buildNodeCatalog(entries), [entries]);
  const search = useMemo(() => buildSearchState(searchRaw, catalog), [searchRaw, catalog]);
  const unmatched = searchUnmatchedFeedback(search);
  const sidebarNodes = catalog
    .filter((n) => matchesSearch(search, n.node, n.mission, n.faction))
    .sort((a, b) => Number(!selected.has(a.id)) - Number(!selected.has(b.id)) || a.node.localeCompare(b.node));
  const visible = filterScheduleEntries(entries, selected, search, days, nowMs);
  const dayGroups = groupEntriesByDay(visible, locale);
  const occurrences = new Set(alerts.occurrences);
  const favorites = new Set(alerts.favoriteNodes);
  const updatedAgo = formatUpdatedAgo(fetchedAt, nowMs);
  const checked = visible.filter((e) => copySelection.has(scheduleEntryKey(e)));

  const select = (next: Set<string>) => {
    setSelected(next);
    saveSelectedNodeIds(next);
  };
  const toggleNode = (id: string) => select(selected.has(id) ? new Set([...selected].filter((v) => v !== id)) : new Set([...selected, id]));
  const apply = async (call: Promise<ArbiScheduleAlerts | null>) => {
    const next = await call;
    if (next) setAlerts(next);
  };
  const toggleCopyRow = (key: string) => setCopySelection((cur) => new Set(cur.has(key) ? [...cur].filter((k) => k !== key) : [...cur, key]));

  const flashCopy = (state: CopyState) => {
    setCopyState(state);
    clearTimeout(flash.current);
    flash.current = setTimeout(() => setCopyState("idle"), 2000);
  };
  const copyChecked = async () => {
    if (copyState === "busy" || checked.length === 0) return;
    if (checked.length > MAX_COPY_ROWS) return flashCopy("max");
    setCopyState("busy");
    try {
      const blob = await renderArbiCard(groupEntriesByDay([...checked].sort((a, b) => a.epochMs - b.epochMs), locale), {
        header: tr("arbisched.cardHeader", { count: checked.length }),
        zone: timezoneLabel(),
        brand: tr("arbisched.brand"),
        time: (ms) => formatEntryTime(ms, locale),
      });
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      flashCopy("done");
    } catch (err) {
      console.warn("[ArbiSched] copy checked rows failed", err);
      flashCopy("error");
    }
  };

  const savePreset = async () => {
    const name = presetName.trim();
    if (!name) return setPresetStatus(tr("arbisched.presetNameMissing"));
    const at = presets.findIndex((p) => p.name.toLowerCase() === name.toLowerCase());
    if (at >= 0 && !(await askConfirm(tr("arbisched.presetOverwrite", { name: presets[at].name })))) return;
    const preset = { name, nodeIds: [...selected], updatedAt: Date.now() };
    const next = (at >= 0 ? presets.map((p, i) => (i === at ? preset : p)) : [...presets, preset]).sort((a, b) => a.name.localeCompare(b.name));
    setPresets(next);
    saveSelectionPresets(next);
    setPresetSelected(name);
    setPresetName("");
    setPresetStatus(tr("arbisched.presetSaved", { name, count: String(preset.nodeIds.length) }));
  };
  const chosenPreset = () => presets.find((p) => p.name === presetSelected);
  const loadPreset = () => {
    const p = chosenPreset();
    if (!p) return setPresetStatus(tr("arbisched.presetSelectFirst"));
    select(new Set(p.nodeIds.filter((id) => catalog.some((n) => n.id === id))));
    setPresetStatus(tr("arbisched.presetLoaded", { name: p.name }));
  };
  const deletePreset = async () => {
    const p = chosenPreset();
    if (!p) return setPresetStatus(tr("arbisched.presetSelectFirst"));
    if (!(await askConfirm(tr("arbisched.presetDeleteConfirm", { name: p.name })))) return;
    const next = presets.filter((x) => x.name !== p.name);
    setPresets(next);
    saveSelectionPresets(next);
    setPresetSelected("");
    setPresetStatus(tr("arbisched.presetDeleted", { name: p.name }));
  };

  const activeCount = [...selected].filter((id) => catalog.some((n) => n.id === id)).length;
  const copyLabel =
    copyState === "done" ? tr("common.copied") : copyState === "error" ? tr("common.copyFailed") : copyState === "max" ? tr("arbisched.copyMax", { max: String(MAX_COPY_ROWS) }) : tr("arbisched.copySelected", { n: String(checked.length) });

  return (
    <div className="@container">
      <div className="grid grid-cols-1 gap-4 @6xl:grid-cols-[19rem_minmax(0,1fr)]" data-arbi-schedule>
        <aside className="grid min-w-0 grid-cols-1 gap-4 self-start @2xl:grid-cols-2 @6xl:sticky @6xl:top-0 @6xl:grid-cols-1">
          <Card>
            <CardHeader
              title={tr("arbisched.nodes")}
              meta={selected.size > 0 ? tr("arbisched.nodeCountSelected", { active: String(activeCount), total: String(catalog.length) }) : tr("arbisched.nodeCount", { total: String(catalog.length) })}
            />
            <CardBody className="flex flex-col gap-2">
              <SearchInput label={tr("arbisched.searchPlaceholder")} placeholder={tr("arbisched.searchPlaceholder")} value={searchRaw} onChange={(e) => setSearchRaw(e.target.value)} />
              {unmatched && <span className="text-xs text-warning">{tr("arbisched.noNodeMatch", { tokens: unmatched })}</span>}
              <ul className="m-0 max-h-[26rem] list-none overflow-y-auto rounded-[var(--radius-md)] border border-border p-0" data-arbi-node-list>
                {sidebarNodes.length === 0 && <li className="px-3 py-4 text-center text-sm text-text-muted">{tr("arbisched.noNodes")}</li>}
                {sidebarNodes.map((n) => {
                  const active = selected.has(n.id);
                  const key = factionBadgeKey(n.faction);
                  return (
                    <li key={n.id} data-arbi-node={n.id} className={`flex items-center gap-2 border-b border-border px-2 py-1.5 last:border-b-0 ${active ? "bg-accent-glow" : "hover:bg-bg-hover"}`}>
                      <Checkbox aria-label={n.node} checked={active} onChange={() => toggleNode(n.id)} />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm text-text-primary">{n.node}</span>
                        <span className="block truncate text-xs text-text-muted">{n.mission}</span>
                      </span>
                      <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: FACTION_DOT[key] }} title={n.faction} />
                      <IconButton label={tr("arbisched.starTitle")} aria-pressed={favorites.has(n.id)} className={favorites.has(n.id) ? "!text-warning" : ""} onClick={() => void apply(arbiScheduleSetFavorite(n.id, !favorites.has(n.id)))}>
                        <AlarmClock aria-hidden="true" />
                      </IconButton>
                    </li>
                  );
                })}
              </ul>
              <div className="flex gap-2">
                <Button size="sm" className="flex-1" onClick={() => select(new Set(catalog.map((n) => n.id)))}>{tr("common.all")}</Button>
                <Button size="sm" className="flex-1" onClick={() => select(new Set())}>{tr("common.none")}</Button>
              </div>
            </CardBody>
          </Card>
          <Card>
            <CardHeader title={tr("arbisched.presets")} />
            <CardBody className="flex flex-col gap-2">
              <Select value={presetSelected} onChange={(e) => setPresetSelected(e.target.value)} aria-label={tr("arbisched.presets")} options={[["", tr("arbisched.presetSelect")], ...presets.map((p) => [p.name, p.name] as const)]} />
              <div className="flex gap-2">
                <Button size="sm" className="flex-1" disabled={presets.length === 0} onClick={loadPreset}>{tr("arbisched.presetLoad")}</Button>
                <Button size="sm" variant="danger" className="flex-1" disabled={presets.length === 0} onClick={deletePreset}>{tr("common.delete")}</Button>
              </div>
              <div className="flex gap-2">
                <Input className="min-w-0 flex-1" placeholder={tr("arbisched.presetName")} aria-label={tr("arbisched.presetName")} value={presetName} onChange={(e) => setPresetName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && savePreset()} />
                <Button size="sm" onClick={savePreset}>{tr("common.save")}</Button>
              </div>
              {presetStatus && <span className="text-xs text-text-muted">{presetStatus}</span>}
            </CardBody>
          </Card>
        </aside>

        <Card className="min-w-0">
          <CardHeader
            title={tr("common.arbitrations")}
            meta={[tr("arbisched.entries", { count: String(visible.length) }), updatedAgo && tr("arbisched.updated", { ago: updatedAgo })].filter(Boolean).join(", ")}
            tools={
              <>
                <Select
                  aria-label={tr("arbisched.days", { n: String(days) })}
                  value={days}
                  options={DAY_OPTIONS.map(([d]) => [d, tr("arbisched.days", { n: String(d) })] as const)}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    const n = Number.isFinite(v) && v > 0 ? v : 30;
                    setDays(n);
                    saveDaysToShow(n);
                  }}
                />
                <label className="flex items-center gap-1.5 text-xs text-text-secondary">
                  <Bell className="h-3.5 w-3.5" aria-hidden="true" />
                  {tr("arbisched.leadLabel")}
                  <Input
                    key={alerts.minutesBefore}
                    type="number"
                    min={1}
                    max={120}
                    className="!w-14 text-center"
                    defaultValue={alerts.minutesBefore}
                    onBlur={(e) => {
                      const v = Number(e.target.value);
                      if (Number.isFinite(v)) void apply(arbiScheduleSetLead(Math.min(120, Math.max(1, Math.floor(v)))));
                    }}
                  />
                  {tr("arbisched.leadSuffix")}
                </label>
                <Button size="sm" disabled={checked.length === 0 || copyState === "busy"} title={tr("arbisched.copySelectedTitle")} onClick={() => void copyChecked()} icon={copyState === "done" ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />} data-arbi-copy>
                  {copyLabel}
                </Button>
              </>
            }
          />
          {!loaded ? (
            <CardBody className="flex flex-col gap-2">
              <Skeleton height="2.25rem" />
              <Skeleton height="2.25rem" />
              <Skeleton height="2.25rem" />
            </CardBody>
          ) : loadFailed && visible.length === 0 ? (
            <EmptyState title={tr("arbisched.unavailable")} />
          ) : visible.length === 0 ? (
            <EmptyState title={tr("arbisched.empty")} />
          ) : (
            <div data-arbi-table>
              <Table mode="stack" label={tr("common.arbitrations")}>
                <thead>
                  <tr>
                    <th aria-label={tr("arbisched.copySelectedTitle")} />
                    <th>{tr("foundry.sort.time")}</th>
                    <th>{tr("common.node")}</th>
                    <th>{tr("common.mission")}</th>
                    <th>{tr("arbisched.col.faction")}</th>
                    <th className="ui-num">{tr("arbisched.col.startsIn")}</th>
                    <th aria-hidden="true" />
                  </tr>
                </thead>
                <tbody>
                  {dayGroups.map((g) => [
                    <tr key={g.dayKey}>
                      <th colSpan={7} scope="colgroup" className="bg-bg-raised text-left text-xs font-semibold text-text-secondary">{g.dayLabel}</th>
                    </tr>,
                    ...g.entries.map((en) => {
                      const countdown = formatScheduleCountdown(en.epochMs, nowMs);
                      const key = scheduleEntryKey(en);
                      const belled = occurrences.has(key);
                      return (
                        <tr key={key} data-arbi-row>
                          <td data-actions>
                            <Checkbox aria-label={tr("arbisched.copyRowLabel", { node: en.node })} title={tr("arbisched.copyRowLabel", { node: en.node })} checked={copySelection.has(key)} onChange={() => toggleCopyRow(key)} />
                          </td>
                          <td data-label={tr("foundry.sort.time")} className="whitespace-nowrap tabular-nums text-text-secondary">{formatEntryTime(en.epochMs, locale)}</td>
                          <td data-primary>
                            <span className="font-semibold text-text-primary">{en.node}</span>
                            {favorites.has(en.nodeId) && <AlarmClock className="ml-1 inline h-3.5 w-3.5 align-[-2px] text-warning" aria-label={tr("arbisched.starTitle")} />}
                          </td>
                          <td data-label={tr("common.mission")} className="text-text-secondary">{en.mission}</td>
                          <td data-label={tr("arbisched.col.faction")}><Badge tone={FACTION_TONE[factionBadgeKey(en.faction)]}>{en.faction}</Badge></td>
                          <td data-label={tr("arbisched.col.startsIn")} className={`ui-num whitespace-nowrap ${countdown === "NOW" ? "font-semibold text-success" : "text-text-primary"}`}>{countdown}</td>
                          <td data-actions>
                            {countdown !== "NOW" && (
                              <IconButton label={tr("arbisched.bellTitle")} aria-pressed={belled} className={belled ? "!text-accent" : ""} onClick={() => void apply(arbiScheduleSetOccurrence(key, !belled))}>
                                {belled ? <BellRing aria-hidden="true" /> : <Bell aria-hidden="true" />}
                              </IconButton>
                            )}
                          </td>
                        </tr>
                      );
                    }),
                  ])}
                </tbody>
              </Table>
            </div>
          )}
        </Card>
      </div>
    </div>
  );
}
