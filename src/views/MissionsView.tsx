import { ChevronRight, Inbox, Info, SearchX, TriangleAlert } from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { MissionMeta, MissionRewardBody, MissionRewardList, MissionRewardTotals } from "../components/missions/MissionRewards";
import { Button, Card, CardBody, CardHeader, EmptyState, Field, IconButton, PageHeader, Readout, SearchInput, Select, Skeleton, Switch, Table, Tabs } from "../components/ui";
import { formatNumber } from "../lib/format";
import type { Translator } from "../lib/i18n";
import {
  missionRewardsPage,
  missionRewardsSetTracking,
  onMissionReward,
  spawnNodesGet,
  type ItemDatabase,
  type MissionRewardSummary,
  type MissionRewardsPage,
  type MissionRewardsQuery,
} from "../lib/ipc";
import {
  MISSION_PERIODS,
  buildRewardRows,
  createPageLoader,
  endedAtLabel,
  localMidnight,
  matchRewardItemTypes,
  missionName,
  missionPeriodStart,
  missionStatusText,
  missionTypeLabel,
  rewardRowDucats,
  type MissionPeriod,
  type PageLoadMode,
} from "../lib/missionRewardRows";
import { usePersisted } from "../lib/usePersisted";

interface Props {
  tr: Translator;
  locale: string;
  items: ItemDatabase;
}

const PAGE_SIZE = 50;
const SEARCH_DEBOUNCE_MS = 250;
const PERIOD_KEYS = {
  today: "missions.period.today",
  "7d": "missions.period.7d",
  "30d": "analysis.range.30d",
  all: "analysis.range.all",
} as const;

const count = (tr: Translator, one: "missions.todayCountOne" | "missions.recordedOne" | "missions.itemTypeCountOne", many: "missions.todayCount" | "missions.recorded" | "missions.itemTypeCount", n: number, locale: string) =>
  tr(n === 1 ? one : many, { count: formatNumber(n, locale) });

export default function MissionsView({ tr, locale, items }: Props) {
  const [period, setPeriod] = usePersisted<MissionPeriod>("wf_missions_period", MISSION_PERIODS, "7d");
  const [page, setPage] = useState<MissionRewardsPage | null>(null);
  const [summaries, setSummaries] = useState<MissionRewardSummary[]>([]);
  const [failed, setFailed] = useState(false);
  const [failedFilterChange, setFailedFilterChange] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [missionType, setMissionType] = useState("");
  const [search, setSearch] = useState("");
  const [appliedSearch, setAppliedSearch] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [showPeriodItems, setShowPeriodItems] = useState(false);
  const [nodeLabels, setNodeLabels] = useState<Map<string, string>>(new Map());
  const rowsRef = useRef<MissionRewardSummary[]>([]);

  const loader = useRef(
    createPageLoader<MissionRewardSummary, MissionRewardsPage>({
      rows: () => rowsRef.current,
      show: (rows, next) => {
        rowsRef.current = rows;
        setSummaries(rows);
        if (next) setPage(next);
        setFailed(false);
        setFailedFilterChange(false);
      },
      fail: (_error, rowsOutdated) => {
        setFailed(true);
        if (rowsOutdated) setFailedFilterChange(true);
      },
    }),
  ).current;

  // A string, so a reloaded page with the same recorded items does not refire the query.
  const searchKey = useMemo(
    () => (appliedSearch.trim() ? matchRewardItemTypes(page?.itemTypes ?? [], appliedSearch, items).join("\n") : null),
    [appliedSearch, page?.itemTypes, items],
  );

  const buildQuery = (offset: number, limit: number): MissionRewardsQuery => {
    const now = Date.now();
    const since = missionPeriodStart(period, now);
    return {
      offset,
      limit,
      ...(since === null ? {} : { since }),
      todaySince: localMidnight(now),
      ...(missionType ? { missionType } : {}),
      ...(searchKey === null ? {} : { uniqueNames: searchKey ? searchKey.split("\n") : [] }),
    };
  };
  const fetchPage = (query: MissionRewardsQuery, mode: PageLoadMode) => loader(mode, () => missionRewardsPage(query));
  const refresh = () => void fetchPage(buildQuery(0, PAGE_SIZE), "merge");
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;

  useEffect(() => {
    void fetchPage(buildQuery(0, PAGE_SIZE), "replace");
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the filters are the only inputs
  }, [period, missionType, searchKey]);

  useEffect(() => {
    const off = onMissionReward(() => refreshRef.current());
    return () => void off.then((f) => f());
  }, []);

  useEffect(() => {
    let live = true;
    void spawnNodesGet()
      .then((nodes) => live && setNodeLabels(new Map(nodes.map((n) => [n.nodeId, `${n.node} (${n.planet})`]))))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => setAppliedSearch(search), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [search]);

  const status = page?.status ?? null;
  const trackingOff = status?.blocked === "tracking-off";
  const notice = missionStatusText(status, tr);
  const latest = page?.latest ?? null;
  const labelOf = (s: MissionRewardSummary) => (s.node ? (nodeLabels.get(s.node) ?? null) : null);
  const periodRows = useMemo(() => (page ? buildRewardRows(page.totals.items, items) : []), [page, items]);
  const latestRows = useMemo(() => (latest ? buildRewardRows(latest.items, items) : []), [latest, items]);

  const loadMore = async () => {
    setLoadingMore(true);
    await fetchPage(buildQuery(summaries.length, PAGE_SIZE), "append");
    setLoadingMore(false);
  };
  const setTracking = async (enabled: boolean) => {
    await missionRewardsSetTracking(enabled);
    refresh();
  };

  const trackingToggle = (
    <Switch data-missions-tracking label={tr("settings.trackMissions")} title={tr("settings.missionsDesc")} checked={status !== null && !trackingOff} disabled={status === null} onChange={(e) => void setTracking(e.target.checked)} />
  );
  const subtitle = page && (
    <span data-missions-recorded={page.recorded}>
      <span data-missions-today={page.today}>{count(tr, "missions.todayCountOne", "missions.todayCount", page.today, locale)}</span>, {count(tr, "missions.recordedOne", "missions.recorded", page.recorded, locale)}
    </span>
  );
  const empty = (marker: Record<string, string>, text: string, icon: ReactNode = <Inbox />) => (
    <Card>
      <EmptyState icon={icon} title={<span {...marker}>{text}</span>} />
    </Card>
  );

  return (
    <div className="mx-auto flex w-full max-w-[1280px] flex-col gap-4" data-missions-view>
      <PageHeader title={tr("enemy.missions")} sub={subtitle} actions={trackingToggle} />

      {notice && (
        <Card className="p-3">
          <p className="m-0 flex items-center gap-2 text-sm text-text-secondary" data-missions-status={trackingOff ? "tracking-off" : status?.phase}>
            <Info className="shrink-0" aria-hidden="true" />
            {notice}
          </p>
        </Card>
      )}

      {failed && (!page || failedFilterChange) ? (
        empty({ "data-missions-error": "" }, tr("dashboard.widgetError"), <TriangleAlert />)
      ) : !page ? (
        <Card className="flex flex-col gap-3 p-4">
          <Skeleton width="30%" height="1.1rem" />
          <Skeleton />
          <Skeleton width="70%" />
        </Card>
      ) : !latest ? (
        !trackingOff && empty({ "data-missions-empty": "" }, tr("dashboard.lastMission.none"))
      ) : (
        <>
          <Card as="article" data-missions-latest={latest.id}>
            <CardHeader title={tr("dashboard.lastMission")} meta={<MissionMeta mission={latest} nodeLabel={labelOf(latest)} locale={locale} />} />
            <CardBody className="flex flex-col gap-3">
              <MissionRewardBody mission={latest} rows={latestRows} tr={tr} locale={locale} />
            </CardBody>
          </Card>

          <div className="flex flex-wrap items-end justify-between gap-3 border-b border-border" data-missions-periods={period}>
            <Tabs label={tr("enemy.missions")} items={MISSION_PERIODS.map((key) => ({ key, label: tr(PERIOD_KEYS[key]) }))} active={period} onSelect={setPeriod} className="border-b-0" />
            <div className="flex flex-wrap items-end gap-3 pb-2" data-missions-filters>
              <Field label={tr("common.type")}>
                <Select
                  value={missionType}
                  onChange={(e) => setMissionType(e.target.value)}
                  data-missions-type-filter
                  options={[["", tr("arbi.filter.allTypes")] as const, ...page.missionTypes.map((type) => [type, missionTypeLabel(type) ?? type] as const)]}
                />
              </Field>
              <div className="min-w-[14rem]" data-missions-search>
                <SearchInput label={tr("orderModal.searchItemsPlaceholder")} value={search} placeholder={tr("orderModal.searchItemsPlaceholder")} onChange={(e) => setSearch(e.target.value)} />
              </div>
            </div>
          </div>

          <div data-missions-totals>
            <Card className="flex flex-col gap-3 p-4">
              <MissionRewardTotals tr={tr} locale={locale} missions={page.totals.missions} ducats={rewardRowDucats(periodRows)} credits={page.totals.credits} endo={page.totals.endo} />
              {periodRows.length > 0 && (
                <>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="self-start"
                    aria-expanded={showPeriodItems}
                    data-missions-items-toggle
                    icon={<ChevronRight className={`transition-transform duration-150 ${showPeriodItems ? "rotate-90" : ""}`} />}
                    onClick={() => setShowPeriodItems(!showPeriodItems)}
                  >
                    {tr("missions.itemsReceived", { count: formatNumber(periodRows.length, locale) })}
                  </Button>
                  {showPeriodItems && <MissionRewardList rows={periodRows} tr={tr} locale={locale} className="max-h-[420px] overflow-y-auto" />}
                </>
              )}
            </Card>
          </div>

          {summaries.length === 0 ? (
            empty({ "data-missions-no-match": "" }, tr("missions.noMatch"), <SearchX />)
          ) : (
            <>
              <Card className="overflow-hidden">
                <Table mode="stack" label={tr("enemy.missions")}>
                  <thead>
                    <tr>
                      <th>{tr("arbi.col.date")}</th>
                      <th>{tr("common.mission")}</th>
                      <th className="ui-num">{tr("common.ducats")}</th>
                      <th className="ui-num">{tr("common.credits")}</th>
                      <th className="ui-num">{tr("stats.endo")}</th>
                    </tr>
                  </thead>
                  <tbody data-missions-list>
                    {summaries.map((s) => {
                      const rows = buildRewardRows(s.items, items);
                      const open = expanded[s.id] === true;
                      const expandable = rows.length > 0 || s.missionCount > 1;
                      return (
                        <Fragment key={s.id}>
                          <tr data-mission-entry={s.id} aria-selected={open || undefined}>
                            <td data-primary>
                              <div className="flex items-center gap-2">
                                {expandable ? (
                                  <IconButton label={tr(open ? "layout.collapseSection" : "layout.expandSection")} aria-expanded={open} data-mission-toggle={s.id} onClick={() => setExpanded({ ...expanded, [s.id]: !open })}>
                                    <ChevronRight className={`transition-transform duration-150 ${open ? "rotate-90" : ""}`} />
                                  </IconButton>
                                ) : (
                                  <span className="w-[1.875rem] shrink-0" aria-hidden="true" />
                                )}
                                <span className="tabular-nums text-text-secondary">{endedAtLabel(s.endedAt, locale)}</span>
                              </div>
                            </td>
                            <td data-label={tr("common.mission")}>
                              <span data-mission-name>{missionName(s, labelOf(s), tr("common.unknown"))}</span>
                              {labelOf(s) && missionTypeLabel(s.missionType) && <span className="ml-2 text-xs text-text-muted">{missionTypeLabel(s.missionType)}</span>}
                              <span className="ml-2 text-xs text-text-muted" data-mission-item-count>
                                {count(tr, "missions.itemTypeCountOne", "missions.itemTypeCount", s.items.length, locale)}
                              </span>
                            </td>
                            <td className="ui-num" data-label={tr("common.ducats")} data-mission-ducats>
                              <Readout currency="ducat" value={rewardRowDucats(rows)} locale={locale} />
                            </td>
                            <td className="ui-num" data-label={tr("common.credits")} data-mission-credits>
                              <Readout currency="credits" value={s.credits} locale={locale} />
                            </td>
                            <td className="ui-num" data-label={tr("stats.endo")} data-mission-endo>
                              <Readout currency="endo" value={s.endo} locale={locale} />
                            </td>
                          </tr>
                          {open && (
                            <tr>
                              <td colSpan={5} data-mission-detail={s.id}>
                                <div className="flex flex-col gap-2 py-1">
                                  <MissionRewardBody mission={s} rows={rows} tr={tr} locale={locale} variant="entry" />
                                </div>
                              </td>
                            </tr>
                          )}
                        </Fragment>
                      );
                    })}
                  </tbody>
                </Table>
              </Card>
              {summaries.length < page.matched && (
                <div className="flex justify-center" data-missions-more>
                  <Button onClick={() => void loadMore()} loading={loadingMore} disabled={loadingMore}>
                    {tr("market.loadMore")}
                  </Button>
                </div>
              )}
            </>
          )}
        </>
      )}
    </div>
  );
}
