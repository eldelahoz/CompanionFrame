import { ArrowDown, ArrowUp, ChevronLeft, ChevronRight, RefreshCw, UserRound } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createViewState } from "../../stores/viewState";
import { getItemDatabase, getPersonalLoadouts, getPersonalProfile, onInventoryUpdated, type ItemDatabase, type PersonalProfileResult, type SavedLoadout } from "../../lib/ipc";
import { itemLabel } from "../../lib/itemLabel";
import type { MessageKey, Translator } from "../../lib/i18n";
import { Button, Card, EmptyState, Readout, SearchInput, Skeleton, StatStrip, Table, Tabs, ItemCell } from "../ui";
import ProfileAppearance from "./ProfileAppearance";

type Section = "career" | "equipment" | "enemies" | "abilities" | "missions" | "appearance";
type Metric = "equipTime" | "kills" | "headshots" | "assists" | "xp" | "finishers" | "deaths" | "scans" | "used" | "highScore";

const PAGE_SIZE = 50;
const SECTION_KEYS: Record<Section, MessageKey> = {
  career: "profile.career",
  equipment: "inventory.tab.equipment",
  enemies: "profile.enemies",
  abilities: "profile.abilities",
  missions: "enemy.missions",
  appearance: "profile.loadouts",
};
const CAREER: Array<[string, MessageKey]> = [
  ["TimePlayedSec", "profile.timePlayed"],
  ["Income", "profile.creditsEarned"],
  ["ReviveCount", "profile.revives"],
  ["HealCount", "profile.healed"],
  ["Deaths", "profile.deaths"],
  ["MeleeKills", "profile.meleeKills"],
  ["MissionsCompleted", "profile.missionsCompleted"],
  ["MissionsFailed", "profile.missionsFailed"],
  ["MissionsQuit", "profile.missionsQuit"],
  ["MissionsInterrupted", "profile.missionsInterrupted"],
  ["MissionsDumped", "profile.missionsDumped"],
  ["CiphersSolved", "profile.ciphersSolved"],
  ["CiphersFailed", "profile.ciphersFailed"],
  ["CipherTime", "profile.cipherTime"],
  ["PickupCount", "profile.pickups"],
  ["DestroyCount", "profile.objectsDestroyed"],
  ["FishCount", "profile.fishCaught"],
];
const METRIC_KEYS: Record<Metric, MessageKey> = {
  equipTime: "profile.equippedTime",
  kills: "profile.kills",
  headshots: "profile.headshots",
  assists: "profile.assists",
  xp: "profile.affinity",
  finishers: "profile.finishers",
  deaths: "profile.deaths",
  scans: "common.scans",
  used: "profile.timesUsed",
  highScore: "profile.highScore",
};
const SECTION_METRICS: Record<Section, Metric[]> = {
  career: [],
  equipment: ["equipTime", "kills", "headshots", "assists", "xp"],
  enemies: ["kills", "headshots", "assists", "finishers", "deaths", "scans"],
  abilities: ["used"],
  missions: ["highScore"],
  appearance: [],
};
const SECTIONS = Object.keys(SECTION_KEYS) as Section[];

interface Row {
  type: string;
  name: string;
  image: string | null;
  values: Record<string, number | null>;
}

const num = (v: number | null | undefined, locale: string) => (v == null ? null : v.toLocaleString(locale, { maximumFractionDigits: 1 }));

/** The panel comes back as the user left it (memory only). */
const personal = createViewState({ section: "career" as Section, search: "", sort: "name" as "name" | Metric, descending: false, page: 0 });

/** Stats "personal" tab: career totals, per-item tables and the saved loadouts, from the public profile and the inventory. */
export default function PersonalStatsPanel({ tr, locale }: { tr: Translator; locale: string }) {
  const [result, setResult] = useState<PersonalProfileResult | null>(null);
  const [loadouts, setLoadouts] = useState<SavedLoadout[]>([]);
  const [db, setDb] = useState<ItemDatabase>({});
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const [section, setSection] = personal.useField("section");
  const [search, setSearch] = personal.useField("search");
  const [sort, setSort] = personal.useField("sort");
  const [descending, setDescending] = personal.useField("descending");
  const [page, setPage] = personal.useField("page");
  const [now, setNow] = useState(Date.now());
  const alive = useRef(true);
  const generation = useRef(0);

  const load = useCallback(async (force = false) => {
    const mine = ++generation.current;
    setBusy(true);
    setFailed(false);
    try {
      const [next, saved] = await Promise.all([getPersonalProfile(force), getPersonalLoadouts().catch(() => [])]);
      if (alive.current && mine === generation.current) {
        setResult(next);
        setLoadouts(saved);
      }
    } catch {
      if (alive.current && mine === generation.current) setFailed(true);
    } finally {
      if (alive.current && mine === generation.current) setBusy(false);
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void load();
    void getItemDatabase().then((d) => alive.current && setDb(d)).catch(() => {});
    // A new inventory snapshot can change both the account and the saved loadouts; the cached profile is cheap to reload.
    const off = onInventoryUpdated(() => void load());
    return () => {
      alive.current = false;
      void off.then((f) => f());
    };
  }, [load]);

  const deadline = result?.nextRefreshAt ?? 0;
  useEffect(() => {
    setNow(Date.now());
    if (deadline <= Date.now()) return;
    const timer = setInterval(() => {
      setNow(Date.now());
      if (Date.now() >= deadline) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  }, [deadline]);

  const pick = (next: Section) => {
    setSection(next);
    setSearch("");
    setSort("name");
    setDescending(false);
    setPage(0);
  };
  const sortBy = (next: "name" | Metric) => {
    setDescending(sort === next ? !descending : next !== "name");
    setSort(next);
    setPage(0);
  };

  const profile = result?.profile ?? null;
  const waitSeconds = Math.max(0, Math.ceil((deadline - now) / 1000));
  const unavailableKey: MessageKey = result?.status === "no-account" ? "profile.noAccount" : "profile.noData";
  const hasError = failed || result?.status === "fetch-failed";
  const hours = (v: number | null | undefined) => (v == null ? tr("profile.unavailable") : tr("profile.hours", { count: (v / 3600).toLocaleString(locale, { maximumFractionDigits: 1 }) }));
  const plain = (v: number | null | undefined) => num(v, locale) ?? tr("profile.unavailable");
  const metrics = SECTION_METRICS[section];
  const source = section === "equipment" ? profile?.equipment : section === "enemies" ? profile?.enemies : section === "abilities" ? profile?.abilities : section === "missions" ? profile?.missions : null;

  const rows = useMemo<Row[]>(() => {
    const q = search.trim().toLocaleLowerCase(locale);
    const mapped = (source ?? [])
      .map((e): Row => ({ type: e.type, name: e.name || itemLabel(e.type, db[e.type]), image: db[e.type]?.imageUrl ?? null, values: e.stats }))
      .filter((r) => !q || r.name.toLocaleLowerCase(locale).includes(q));
    return mapped.sort((a, b) => {
      if (sort === "name") return (descending ? -1 : 1) * a.name.localeCompare(b.name, locale);
      const av = a.values[sort];
      const bv = b.values[sort];
      if (av == null) return bv == null ? a.name.localeCompare(b.name, locale) : 1;
      if (bv == null) return -1;
      return (descending ? -1 : 1) * (av - bv) || a.name.localeCompare(b.name, locale);
    });
  }, [source, search, sort, descending, db, locale]);
  const lastPage = Math.max(0, Math.ceil(rows.length / PAGE_SIZE) - 1);
  const current = Math.min(page, lastPage);
  const shown = rows.slice(current * PAGE_SIZE, (current + 1) * PAGE_SIZE);

  const cell = (m: Metric, v: number | null | undefined) => (m === "equipTime" ? hours(v) : plain(v));
  const sortHead = (key: "name" | Metric, label: string, numeric = false) => (
    <th className={numeric ? "ui-num" : undefined} aria-sort={sort === key ? (descending ? "descending" : "ascending") : "none"}>
      <button type="button" onClick={() => sortBy(key)}>
        {label}
        {sort === key && (descending ? <ArrowDown aria-hidden="true" /> : <ArrowUp aria-hidden="true" />)}
      </button>
    </th>
  );

  const registered = profile?.registeredAt ? new Date(profile.registeredAt).toLocaleDateString(locale) : null;
  const updated = result?.fetchedAt ? new Date(result.fetchedAt).toLocaleString(locale) : null;
  const cipherAvg = profile && (profile.career.CipherTime ?? 0) > 0 && (profile.career.CiphersSolved ?? 0) > 0 ? profile.career.CipherTime! / profile.career.CiphersSolved! : null;

  const summary = [
    { key: "time", label: tr("profile.timePlayed"), value: hours(profile?.career.TimePlayedSec) },
    { key: "missions", label: tr("profile.missionsCompleted"), value: plain(profile?.career.MissionsCompleted) },
    { key: "credits", label: tr("profile.creditsEarned"), value: profile?.career.Income == null ? tr("profile.unavailable") : <Readout currency="credits" value={profile.career.Income} locale={locale} /> },
    { key: "revives", label: tr("profile.revives"), value: plain(profile?.career.ReviveCount) },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto" data-personal-profile>
      <Card className="flex flex-wrap items-start justify-between gap-3 p-4">
        <div className="min-w-0">
          <h2 className="m-0 text-lg font-semibold text-text-heading" data-profile-name>
            {profile?.displayName || tr("profile.personal")}
          </h2>
          <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-sm text-text-secondary">
            {profile?.masteryRank != null && (
              <span>
                {tr("marketAlerts.masteryRank")}: {profile.masteryRank.toLocaleString(locale)}
              </span>
            )}
            {registered && <span>{tr("profile.registered", { date: registered })}</span>}
          </div>
          <p className="mb-0 mt-2 text-xs text-text-muted">{tr("profile.source")}</p>
        </div>
        <div className="flex flex-col items-end gap-1">
          <Button size="sm" icon={<RefreshCw />} data-profile-refresh loading={busy} disabled={busy || waitSeconds > 0} onClick={() => void load(true)}>
            {tr("common.refresh")}
          </Button>
          {updated && <span className="text-xs text-text-muted">{tr("profile.updated", { date: updated })}</span>}
          {waitSeconds > 0 && <span className="text-xs text-text-muted">{tr("profile.refreshWait", { seconds: waitSeconds })}</span>}
        </div>
      </Card>

      {hasError && (
        <Card className="p-3">
          <p className="m-0 text-sm text-warning" role="status" data-profile-status>
            {profile ? tr("profile.stale") : tr("profile.fetchFailed")}
          </p>
        </Card>
      )}

      {busy && !profile && loadouts.length === 0 ? (
        <Card className="flex flex-col gap-3 p-4">
          <Skeleton width="40%" height="1.1rem" />
          <Skeleton />
          <Skeleton width="70%" />
        </Card>
      ) : !profile ? (
        loadouts.length > 0 ? (
          <>
            {!hasError && <p className="m-0 text-sm text-text-muted">{tr(unavailableKey)}</p>}
            <ProfileAppearance items={[]} loadouts={loadouts} db={db} tr={tr} />
          </>
        ) : (
          !hasError && (
            <Card>
              <EmptyState icon={<UserRound />} title={tr(unavailableKey)} />
            </Card>
          )
        )
      ) : (
        <>
          <StatStrip items={summary} />
          <Tabs label={tr("profile.personal")} items={SECTIONS.map((key) => ({ key, label: tr(SECTION_KEYS[key]) }))} active={section} onSelect={pick} />

          {section === "career" ? (
            <Card className="ui-cq px-4 py-1">
              <dl className="m-0 grid grid-cols-1 gap-x-8 @2xl:grid-cols-2">
                {CAREER.map(([key, label]) => {
                  const v = profile.career[key];
                  return (
                    <div key={key} className="flex items-center justify-between gap-4 border-b border-border py-2.5" data-profile-career={key}>
                      <dt className="text-sm text-text-secondary">{tr(label)}</dt>
                      <dd className="m-0 text-right font-medium tabular-nums text-text-primary">
                        {key === "Income" && v != null ? (
                          <Readout currency="credits" value={v} locale={locale} />
                        ) : key === "CipherTime" ? (
                          v == null ? tr("profile.unavailable") : tr("profile.seconds", { count: num(v, locale)! })
                        ) : key === "TimePlayedSec" ? (
                          hours(v)
                        ) : (
                          plain(v)
                        )}
                      </dd>
                    </div>
                  );
                })}
                {cipherAvg !== null && (
                  <div className="flex items-center justify-between gap-4 py-2.5">
                    <dt className="text-sm text-text-secondary">{tr("profile.averageCipherTime")}</dt>
                    <dd className="m-0 font-medium tabular-nums text-text-primary">{tr("profile.seconds", { count: num(cipherAvg, locale)! })}</dd>
                  </div>
                )}
              </dl>
            </Card>
          ) : section === "appearance" ? (
            <ProfileAppearance items={profile.appearance} loadouts={loadouts} db={db} tr={tr} />
          ) : (
            <Card className="flex min-w-0 flex-col gap-3 p-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <SearchInput
                  className="w-72 max-w-full"
                  label={tr("common.searchPlaceholder")}
                  placeholder={tr("common.searchPlaceholder")}
                  value={search}
                  onChange={(e) => {
                    setSearch(e.target.value);
                    setPage(0);
                  }}
                />
                <span className="text-xs text-text-muted">{tr("profile.rowCount", { count: rows.length.toLocaleString(locale) })}</span>
              </div>
              {source == null ? (
                <EmptyState title={tr("profile.tableUnavailable")} />
              ) : rows.length === 0 ? (
                <EmptyState title={search ? tr("profile.noMatches") : tr("profile.noEntries")} />
              ) : (
                <>
                  <Table mode="stack" label={tr(SECTION_KEYS[section])}>
                    <thead>
                      <tr>
                        {sortHead("name", tr("common.name"))}
                        {metrics.map((m) => sortHead(m, tr(METRIC_KEYS[m]), true))}
                      </tr>
                    </thead>
                    <tbody data-profile-table={section}>
                      {shown.map((r) => (
                        <tr key={r.type}>
                          <td data-primary>
                            <ItemCell name={r.name} image={r.image} />
                          </td>
                          {metrics.map((m) => (
                            <td key={m} className="ui-num" data-label={tr(METRIC_KEYS[m])}>
                              {cell(m, r.values[m])}
                            </td>
                          ))}
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                  {lastPage > 0 && (
                    <div className="flex items-center justify-center gap-3">
                      <Button size="sm" icon={<ChevronLeft />} disabled={current === 0} onClick={() => setPage(current - 1)}>
                        {tr("profile.previousPage")}
                      </Button>
                      <span className="text-xs text-text-muted">{tr("profile.page", { current: current + 1, total: lastPage + 1 })}</span>
                      <Button size="sm" disabled={current >= lastPage} onClick={() => setPage(current + 1)}>
                        {tr("profile.nextPage")}
                        <ChevronRight />
                      </Button>
                    </div>
                  )}
                </>
              )}
            </Card>
          )}
        </>
      )}
    </div>
  );
}
