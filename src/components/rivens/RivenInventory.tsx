import { LayoutGrid, RefreshCw, Rows3, SearchX, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { addToast } from "../../stores/toasts";
import { RIVEN_ATTR_GRADE_KEYS, RIVEN_ATTR_GRADE_ORDER, RIVEN_TYPE_KEYS } from "../../lib/rivenLabels";
import { rivenChatTag, rivenWtsLine } from "../../lib/rivenChatTag";
import { matchRivenListings } from "../../lib/marketContract";
import { groupVeiledRivens } from "../../lib/veiledRivenGroups";
import { onInventoryUpdated, rivensDecoded, wfmContractsMy, type DecodedRiven, type DecodedRivens, type ItemDatabase, type WfmContract } from "../../lib/ipc";
import type { Translator } from "../../lib/i18n";
import { usePersisted } from "../../lib/usePersisted";
import SortControl from "../SortControl";
import { Button, Card, EmptyState, Field, IconButton, SearchInput, Segmented, Select, Skeleton } from "../ui";
import RivenCard, { type RivenCardSize } from "./RivenCard";
import RivenDetailModal from "./RivenDetailModal";
import { createViewState } from "../../stores/viewState";

const TYPES = ["all", "Rifle", "Shotgun", "Pistol", "Melee", "Archgun", "Kitgun", "Zaw"] as const;
// Letter families: grading emits +/- variants (A+, B-, ...), S and F stand alone.
const GRADES = ["all", "S", "A", "B", "C", "F"] as const;
const ATTR_GRADES = ["all", "Great", "Good", "OK", "Bad"] as const;
const SORTS = ["name", "disposition", "rerolls", "grade", "attr_grade"] as const;
const DIRS = ["asc", "desc"] as const;
const SIZES = ["full", "compact"] as const;
type SortKey = (typeof SORTS)[number];

const GRADE_RANK: Record<string, number> = { S: 6, A: 5, B: 4, C: 3, D: 2, F: 1 };
const gradeRank = (g: string) => (GRADE_RANK[g.charAt(0)] ?? 0) * 3 + (g.endsWith("+") ? 1 : g.endsWith("-") ? -1 : 0);

const sortValue = (r: DecodedRiven, key: SortKey): string | number => {
  switch (key) {
    case "name":
      return r.weaponName.toLowerCase();
    case "disposition":
      return r.disposition;
    case "rerolls":
      return r.rerolls;
    case "grade":
      return gradeRank(r.overallGrade);
    case "attr_grade":
      return RIVEN_ATTR_GRADE_ORDER[r.attributeGrade] ?? -1;
  }
};

export const rivenCounts = (d: DecodedRivens | null) => ({
  unveiled: d?.unveiled.length ?? 0,
  veiled: d ? d.veiled.length + d.veiledUnseen.reduce((n, g) => n + g.count, 0) : 0,
});

function Veiled({ data, tr }: { data: DecodedRivens; tr: Translator }) {
  const groups = useMemo(() => groupVeiledRivens(data.veiled, tr("rivens.challengeNotAssigned")), [data.veiled, tr]);
  if (data.veiled.length === 0 && data.veiledUnseen.length === 0) return <Card><EmptyState title={tr("rivens.noVeiled")} /></Card>;
  return (
    <div className="flex flex-col gap-5">
      {groups.map((group) => (
        <Card key={group.label} className="riven-veiled-group" data-riven-veiled-group={group.label}>
          <header className="ui-card-head">
            <h2>{group.label}</h2>
            <span className="ui-badge" data-riven-veiled-group-count>
              x{group.entries.length}
            </span>
          </header>
          {group.entries.map((entry, i) => (
            <div key={i} className="riven-veiled-row" data-riven-veiled-row>
              <strong className="min-w-16 shrink-0 text-sm" data-riven-veiled-name>
                {tr("rivens.rivenMod", { label: entry.label })}
              </strong>
              <div className="flex min-w-0 flex-1 items-center gap-3" data-riven-veiled-challenge>
                {entry.challengeDesc ? (
                  <>
                    <span className="text-xs text-text-secondary">{entry.challengeDesc}</span>
                    {entry.challengeProgress != null && entry.challengeRequired != null && (
                      <>
                        <div className="riven-progress" role="progressbar" aria-valuemin={0} aria-valuemax={entry.challengeRequired} aria-valuenow={entry.challengeProgress}>
                          <i style={{ width: `${Math.min(100, (entry.challengeProgress / Math.max(entry.challengeRequired, 1)) * 100)}%` }} />
                        </div>
                        <span className="shrink-0 text-xs tabular-nums text-text-muted">
                          {entry.challengeProgress} / {entry.challengeRequired}
                        </span>
                      </>
                    )}
                  </>
                ) : (
                  <span className="text-xs italic text-text-muted">{tr("rivens.challengeNotAssigned")}</span>
                )}
              </div>
            </div>
          ))}
        </Card>
      ))}
      {data.veiledUnseen.length > 0 && (
        <section>
          <h3 className="m-0 mb-3 text-sm font-semibold text-text-secondary">{tr("rivens.unseenTitle")}</h3>
          <div className="riven-unseen">
            {data.veiledUnseen.map((group) => (
              <Card key={group.itemType} className="flex flex-col items-center gap-2 p-4 text-center">
                <strong>{group.label}</strong>
                <span className="text-xs text-text-muted">{tr("rivens.equipToReveal")}</span>
                <span className="ui-badge mt-auto">x{group.count}</span>
              </Card>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

interface Props {
  tab: "unveiled" | "veiled";
  tr: Translator;
  locale: string;
  items?: ItemDatabase;
  /** Persisted "similar auctions" toggle of the detail modal (app setting). */
  similarAuctions: boolean;
  onSimilarAuctions: (shown: boolean) => void;
  onData: (d: DecodedRivens | null) => void;
}

/** Inventory rivens as decoded from their fingerprints: the Unveiled and Veiled tabs of the Rivens view. */
/** The filters come back as the user left them (memory only). */
const rivenInv = createViewState({
  query: "",
  type: "all" as (typeof TYPES)[number],
  grade: "all" as (typeof GRADES)[number],
  attrGrade: "all" as (typeof ATTR_GRADES)[number],
});

export default function RivenInventory({ tab, tr, locale, items, similarAuctions, onSimilarAuctions, onData }: Props) {
  const [data, setData] = useState<DecodedRivens | null>(null);
  const [listings, setListings] = useState<WfmContract[]>([]);
  const [refreshing, setRefreshing] = useState(false);
  const [query, setQuery] = rivenInv.useField("query");
  const [type, setType] = rivenInv.useField("type");
  const [grade, setGrade] = rivenInv.useField("grade");
  const [attrGrade, setAttrGrade] = rivenInv.useField("attrGrade");
  const [sort, setSort] = usePersisted<SortKey>("cf_rivens_sort", SORTS, "name");
  const [dir, setDir] = usePersisted("cf_rivens_sort_dir", DIRS, "asc");
  const [size, setSize] = usePersisted<RivenCardSize>("cf_rivens_card_size", SIZES, "full");
  const [selected, setSelected] = useState<DecodedRiven | null>(null);
  const [menu, setMenu] = useState<{ riven: DecodedRiven; x: number; y: number } | null>(null);
  // Narrow windows fold the filters behind a Filters button (rivens.css container query).
  const [filtersOpen, setFiltersOpen] = useState(false);

  const load = () =>
    rivensDecoded().then(
      (d) => {
        setData(d);
        onData(d);
      },
      () => {
        setData({ unveiled: [], veiled: [], veiledUnseen: [] });
        onData(null);
      },
    );
  const loadListings = async () => {
    try {
      setListings((await wfmContractsMy()).contracts);
    } catch {
      setListings([]); // signed out: no listing badges
    }
  };

  useEffect(() => {
    void load();
    void loadListings();
    let off = () => {};
    let gone = false;
    void onInventoryUpdated(() => void load()).then((u) => (gone ? u() : (off = u)));
    return () => {
      gone = true;
      off();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("click", close);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", esc);
    };
  }, [menu]);

  const listingByRiven = useMemo(() => matchRivenListings(data?.unveiled ?? [], listings), [data, listings]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = (data?.unveiled ?? []).filter(
      (r) =>
        (!q || [r.weaponName, r.rivenName, ...r.stats.map((s) => s.name)].some((t) => t.toLowerCase().includes(q))) &&
        (type === "all" || r.rivenType === type) &&
        (grade === "all" || r.overallGrade.toUpperCase().startsWith(grade)) &&
        (attrGrade === "all" || r.attributeGrade === attrGrade),
    );
    const sign = dir === "asc" ? 1 : -1;
    return list.sort((a, b) => {
      const [x, y] = [sortValue(a, sort), sortValue(b, sort)];
      return sign * (x < y ? -1 : x > y ? 1 : 0) || a.weaponName.localeCompare(b.weaponName);
    });
  }, [data, query, type, grade, attrGrade, sort, dir]);

  const copy = async (text: string) => {
    setMenu(null);
    try {
      await navigator.clipboard.writeText(text);
      addToast({ level: "success", message: tr("common.copied"), durationMs: 2000 });
    } catch {
      addToast({ level: "error", message: tr("common.copyFailed"), durationMs: 2000 });
    }
  };

  if (!data)
    return (
      <div className="riven-grid" aria-busy="true" aria-label={tr("rivens.loading")}>
        {[0, 1, 2, 3].map((i) => (
          <Card key={i} className="flex flex-col gap-3 p-4">
            <Skeleton width="55%" height="1.1rem" />
            <Skeleton />
            <Skeleton width="70%" />
          </Card>
        ))}
      </div>
    );
  if (tab === "veiled") return <Veiled data={data} tr={tr} />;

  const menuListing = menu ? listingByRiven.get(menu.riven.itemId) : undefined;
  const activeFilters = [type !== "all", grade !== "all", attrGrade !== "all"].filter(Boolean).length;
  return (
    <div className="flex flex-col gap-4">
      <div className={`riven-toolbar ${filtersOpen ? "is-open" : ""}`}>
        <SearchInput label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
        <Button size="sm" className="riven-filter-toggle" aria-expanded={filtersOpen} onClick={() => setFiltersOpen(!filtersOpen)} icon={<SlidersHorizontal />}>
          {tr("common.filters")}
          {activeFilters > 0 && <span className="ui-badge ui-badge--accent">{activeFilters}</span>}
        </Button>
        <div className="riven-filters">
          <Field label={tr("common.type")}>
            <Select
              value={type}
              options={TYPES.map((v) => [v, v === "all" ? tr("common.all") : tr(RIVEN_TYPE_KEYS[v])] as const)}
              onChange={(e) => setType(e.target.value as (typeof TYPES)[number])}
            />
          </Field>
          <Field label={tr("rivens.sort.grade")} className="riven-grade-filter">
            <Select data-riven-grade-select value={grade} options={GRADES.map((v) => [v, v === "all" ? tr("common.all") : v] as const)} onChange={(e) => setGrade(e.target.value as (typeof GRADES)[number])} />
          </Field>
          <Field label={tr("rivens.sort.attributeGrade")}>
            <Select
              data-riven-attr-grade-select
              value={attrGrade}
              options={ATTR_GRADES.map((v) => [v, v === "all" ? tr("common.all") : tr(RIVEN_ATTR_GRADE_KEYS[v])] as const)}
              onChange={(e) => setAttrGrade(e.target.value as (typeof ATTR_GRADES)[number])}
            />
          </Field>
          <SortControl
            value={sort}
            options={[
              ["name", tr("common.name")],
              ["disposition", tr("rivens.sort.disposition")],
              ["rerolls", tr("common.rerolls")],
              ["grade", tr("rivens.sort.grade")],
              ["attr_grade", tr("rivens.sort.attributeGrade")],
            ]}
            direction={dir}
            onSelect={(v) => setSort(v as SortKey)}
            onToggleDirection={() => setDir(dir === "asc" ? "desc" : "asc")}
            tr={tr}
          />
        </div>
        <div className="ml-auto flex items-center gap-2">
          <Segmented
            label={tr("rivens.cardSize.label")}
            active={size}
            onSelect={setSize}
            items={[
              { key: "full", label: <span className="sr-only">{tr("rivens.cardSize.full")}</span>, icon: <LayoutGrid /> },
              { key: "compact", label: <span className="sr-only">{tr("rivens.cardSize.compact")}</span>, icon: <Rows3 /> },
            ]}
          />
          <IconButton
            label={tr("rivens.refreshListings")}
            data-riven-listings-refresh
            disabled={refreshing}
            onClick={async () => {
              setRefreshing(true);
              await loadListings();
              setRefreshing(false);
            }}
          >
            <RefreshCw />
          </IconButton>
        </div>
      </div>
      {shown.length === 0 ? (
        <Card>
          <EmptyState icon={<SearchX />} title={data.unveiled.length > 0 ? tr("rivens.noResults") : tr("rivens.noUnveiled")} />
        </Card>
      ) : (
        <div className={`riven-grid ${size === "full" ? "riven-grid--full" : ""}`}>
          {shown.map((r) => (
            <RivenCard
              key={r.itemId}
              riven={r}
              listing={listingByRiven.get(r.itemId)}
              size={size}
              tr={tr}
              onOpen={() => setSelected(r)}
              onMenu={(e) => {
                e.preventDefault();
                setMenu({ riven: r, x: Math.max(8, Math.min(e.clientX, window.innerWidth - 224)), y: Math.max(8, Math.min(e.clientY, window.innerHeight - 96)) });
              }}
              onCopy={copy}
            />
          ))}
        </div>
      )}
      {menu && (
        <div className="ui-card fixed z-[60] flex min-w-[13rem] flex-col p-1" style={{ left: menu.x, top: menu.y, boxShadow: "var(--shadow-2)" }} role="menu" data-riven-card-menu>
          <Button variant="ghost" size="sm" role="menuitem" className="justify-start" onClick={() => copy(rivenChatTag(menu.riven))}>
            {tr("rivens.copyChatTag")}
          </Button>
          {menuListing && (
            <Button variant="ghost" size="sm" role="menuitem" className="justify-start" onClick={() => copy(rivenWtsLine(menu.riven, menuListing.buyoutPlatinum ?? menuListing.platinum))}>
              {tr("rivens.copyWtsLine")}
            </Button>
          )}
        </div>
      )}
      {selected && (
        <RivenDetailModal
          riven={selected}
          contract={listingByRiven.get(selected.itemId) ?? null}
          tr={tr}
          locale={locale}
          vaulted={items?.[selected.weaponUniqueName]?.vaulted}
          similarAuctions={similarAuctions}
          onSimilarAuctions={onSimilarAuctions}
          onContractUpdated={() => void loadListings()}
          onClose={() => setSelected(null)}
        />
      )}
    </div>
  );
}
