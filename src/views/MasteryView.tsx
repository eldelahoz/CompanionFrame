import { ChevronDown, ChevronRight, Pin, PinOff } from "lucide-react";
import ImportNote from "../components/inventory/ImportNote";
import { useEffect, useMemo, useState } from "react";
import { fallbackNameFromUniqueName, sanitizeDisplayName } from "../../config/shared/displayName";
import ArchonShardPips from "../components/archon/ArchonShardPips";
import ArchonShardSummaryPanel from "../components/archon/ArchonShardSummary";
import ItemImage from "../components/ItemImage";
import MasteryBreakdownRow from "../components/MasteryBreakdownRow";
import CodexPanel from "../components/mastery/CodexPanel";
import IncarnonTracker from "../components/mastery/IncarnonTracker";
import MasteryPlanner from "../components/mastery/MasteryPlanner";
import MasteryRoadmap from "../components/mastery/MasteryRoadmap";
import { Badge, Button, Card, EmptyState, Field, IconButton, PageHeader, Readout, SearchInput, Segmented, Select, ShowMore, Skeleton, StatStrip, Tabs, useBatch, type StatItem } from "../components/ui";
import { parseArchonShards, summarizeArchonShards } from "../lib/inventory/archonShards";
import { getRelicDatabase, wfmPricesBulk, type MasteryProgress } from "../lib/ipc";
import NoInventoryState from "../components/inventory/NoInventoryState";
import type { MessageKey, Translator } from "../lib/i18n";
import { buildMasteryPlan, type PlannerPin, type PlannerSort } from "../lib/masteryPlanner";
import { catalogLookup, hydrateMasteryItems, roadmapPriceSlugs, type PriceMap } from "../lib/masteryHydrate";
import { masteryProjectionSubtext } from "../lib/masteryProjection";
import { buildMasteryRoadmap } from "../lib/masteryRoadmap";
import { readStorage, writeStorage } from "../lib/persistence";
import { parseOwnedRelics } from "../lib/relic/relicInventory";
import type { RelicDatabase } from "../lib/relic/types";
import { usePersisted } from "../lib/usePersisted";
import { useWfmCatalog } from "../stores/market";
import { createViewState } from "../stores/viewState";
import { foundryFor, ownershipFor, useInventory } from "../stores/inventory";
import { useMasteryProgress } from "../stores/mastery";
import { useItemDatabase } from "../stores/items";
import { restoreMasteryPins, toggleMasteryPin, unpinMasteryItems, useMasteryPinKeep, useMasteryPins } from "../stores/masteryPins";
import { openItemBy, setActiveComponent } from "../stores/modals";
import type { ItemDbEntry } from "../types/inventory";

type Item = MasteryProgress["items"][number];
type Pair = { done: number; total: number };

const CAT_ORDER = ["Warframes", "Primary", "Secondary", "Melee", "Companions", "Archwing", "Amps", "Necramech", "Misc"];
const STATUS_TABS: Array<[string, MessageKey]> = [
  ["all", "common.all"],
  ["missing", "common.missing"],
  ["progress", "common.inProgress"],
  ["mastered", "common.mastered"],
];
const VIEW_TABS = ["collection", "roadmap", "planned", "codex", "incarnon"] as const;
type ViewTab = (typeof VIEW_TABS)[number];
const VIEW_TAB_LABEL: Record<ViewTab, MessageKey> = {
  collection: "mastery.viewCollection",
  roadmap: "mastery.viewRoadmap",
  planned: "mastery.viewPlanned",
  codex: "mastery.viewCodex",
  incarnon: "mastery.viewIncarnon",
};
const SORTS: Array<[string, MessageKey]> = [
  ["name", "common.name"],
  ["owned", "common.owned"],
  ["mastery_xp", "mastery.sort.masteryXp"],
];
const PLANNER_SORTS = ["mastery_xp", "completeness", "name"] as const;
const RING_R = 52;
const RING_C = 2 * Math.PI * RING_R;
const PRICE_CHUNK = 100;

const label = (i: Pick<Item, "name" | "displayName" | "uniqueName">) => sanitizeDisplayName(i.displayName || i.name);

function summaryRows(data: MasteryProgress, tr: Translator, locale: string): StatItem[] {
  const { stats } = data;
  const pm = stats.profileMastery;
  const rows: StatItem[] = [
    { key: "mastered", value: stats.mastered, label: tr("common.mastered"), tone: "success" },
    { key: "progress", value: stats.inProgress, label: tr("common.inProgress"), tone: "warning" },
    { key: "missing", value: stats.missing, label: tr("common.missing"), tone: "danger" },
    { key: "total", value: stats.total, label: tr("common.total") },
  ];
  if (pm?.rank != null) {
    const rank = pm.rank + 1;
    let text = tr("mastery.progressUnavailable");
    if (pm.testReady && pm.xpIntoRank != null) {
      // Banked XP overflows past the bar; show the real figure, not a clamp.
      text = tr("mastery.mrXpReady", { xp: pm.xpIntoRank.toLocaleString(locale), needed: (pm.xpForNext ?? 0).toLocaleString(locale), rank });
    } else if (pm.testReady) {
      text = tr("mastery.mrTestReady", { rank });
    } else if (pm.xpIntoRank != null && pm.xpForNext != null) {
      // The game counts down to the next rank; match it.
      text = tr("mastery.mrRemainingXp", { rank, xp: Math.max(0, pm.xpForNext - pm.xpIntoRank).toLocaleString(locale) });
    } else if (pm.percentToNext != null) {
      text = tr("mastery.percentToNext", { pct: pm.percentToNext });
    }
    const subtext = pm.totalXp != null ? masteryProjectionSubtext(tr, pm.rank, pm.totalXp, 0, locale) : null;
    rows.push({ key: "mr", value: `MR ${pm.rank}`, label: tr("common.mastery"), hint: subtext ? `${text}, ${subtext}` : text });
  }
  return rows;
}

function MasteryCard({ item, tr, locale, shards, itemDb, pinned }: { item: Item; tr: Translator; locale: string; shards: ReturnType<typeof parseArchonShards>["suits"]; itemDb: Record<string, ItemDbEntry>; pinned: boolean }) {
  const name = label(item);
  const missing = item.status === "missing";
  const pct = missing ? 0 : Math.max(0, Math.min(100, Math.floor((item.rank / Math.max(item.maxRank, 1)) * 100)));
  const remaining = item.masteryXpRemaining ?? 0;
  return (
    <div className="ms-card" data-mastery-card={item.uniqueName} data-status={item.status}>
      <button type="button" className="ms-card-main" onClick={() => openItemBy({ uniqueName: item.uniqueName })} title={tr("common.openDetailsFor", { name })}>
        <span className="ms-card-img">
          <ItemImage src={item.imageUrl} fallback={item.fallbackImageUrl} alt="" tr={tr} />
          {item.vaulted && (
            <span className="ms-card-vault">
              <Badge>V</Badge>
            </span>
          )}
          {shards.length > 0 && (
            <span className="ms-card-shards">
              {shards.map((copy, i) => (
                <ArchonShardPips key={copy.instanceId ?? i} slots={copy.slots} itemDb={itemDb} tr={tr} size="lg" title={tr(copy.filled === 1 ? "archon.shardCountOne" : "archon.shardCount", { count: copy.filled })} />
              ))}
            </span>
          )}
          <span className={`ms-card-dot ms-card-dot--${item.status}`} aria-hidden="true" />
        </span>
        <span className="ms-card-body">
          <span className="ms-card-name">{name}</span>
          <span className="ms-sub ms-sub--muted">
            {item.category}
            {item.masteryReq ? `, ${tr("rivens.mr", { level: item.masteryReq })}` : ""}
          </span>
          {remaining > 0 && (
            <span title={tr("mastery.xpBadgeTitle")}>
              <Readout chip currency="mastery" value={remaining} locale={locale}>
                <span>+{remaining.toLocaleString(locale)} XP</span>
              </Readout>
            </span>
          )}
          {missing ? (
            <span className="ms-sub ms-sub--muted">{tr("mastery.notOwned")}</span>
          ) : (
            <>
              <span className={`ui-meter ui-meter--${item.status === "mastered" ? "success" : "accent"}`} aria-hidden="true">
                <span style={{ width: `${Math.min(100, (item.rank / Math.max(item.maxRank, 1)) * 100)}%` }} />
              </span>
              <span className="ms-sub ms-sub--muted">{tr("mastery.rankLine", { rank: item.rank, maxRank: item.maxRank, pct })}</span>
            </>
          )}
          {item.components.length > 0 && (
            <span className="ms-card-parts">
              {item.components.slice(0, 8).map((c, i) => (
                <i key={`${c.uniqueName}-${i}`} data-part-state={c.owned ? "owned" : "missing"} title={`${sanitizeDisplayName(c.displayName || c.name) || "?"}: ${tr(c.owned ? "common.owned" : "common.missing")}`} />
              ))}
            </span>
          )}
        </span>
      </button>
      {item.status !== "mastered" && (
        <IconButton className="ms-card-pin" label={tr(pinned ? "mastery.planner.unpin" : "mastery.planner.pin")} aria-pressed={pinned} data-mastery-pin={item.uniqueName} onClick={() => toggleMasteryPin(item.uniqueName)}>
          {pinned ? <PinOff /> : <Pin />}
        </IconButton>
      )}
    </div>
  );
}

function Collapse({ open, onToggle, title, children, tools }: { open: boolean; onToggle: () => void; title: string; children: React.ReactNode; tools?: React.ReactNode }) {
  return (
    <div className="grid gap-2">
      <div className="ms-collapse">
        <button type="button" className="ms-collapse-toggle" aria-expanded={open} onClick={onToggle}>
          {open ? <ChevronDown aria-hidden="true" /> : <ChevronRight aria-hidden="true" />}
          {title}
        </button>
        {tools}
      </div>
      {open && children}
    </div>
  );
}

interface Props {
  tr: Translator;
  locale: string;
}

/** The search comes back as the user left it (memory only). */
const masteryUi = createViewState({ query: "" });

export default function MasteryView({ tr, locale }: Props) {
  const shared = useMasteryProgress();
  const data: MasteryProgress | null | undefined = shared.loaded ? shared.data : undefined;
  const [viewTab, setViewTab] = usePersisted<ViewTab>("wf_mastery_view_tab", VIEW_TABS, "collection");
  const [archonOpen, setArchonOpen] = useState(() => readStorage("mastery-archon-expanded") === "true");
  const inventory = useInventory();
  const itemDbRaw = useItemDatabase();
  const itemDb = itemDbRaw as unknown as Record<string, ItemDbEntry>; // same cast as stores/inventory.ts
  const archon = useMemo(() => parseArchonShards(inventory.data), [inventory.data]);
  const archonSummary = useMemo(() => summarizeArchonShards(archon), [archon]);
  const [cat, setCat] = usePersisted("wf_mastery_cat_tab", ["all", ...CAT_ORDER], "all");
  const [status, setStatus] = usePersisted("wf_mastery_status_tab", STATUS_TABS.map(([k]) => k), "all");
  const [expanded, setExpanded] = useState(() => readStorage("mastery-breakdown-expanded") === "true");
  const [display, setDisplay] = usePersisted("mastery-breakdown-display", ["bars", "rings"] as const, "bars");
  const [query, setQuery] = masteryUi.useField("query");
  const [sort, setSort] = usePersisted("wf_mastery_sort", SORTS.map(([k]) => k), "name");
  const [plannerSort, setPlannerSort] = usePersisted<PlannerSort>("wf_mastery_planner_sort", PLANNER_SORTS, "mastery_xp");
  const pins = useMasteryPins();
  const keepPinned = useMasteryPinKeep();
  const [notices, setNotices] = useState<Array<{ id: number; dropped: string[]; text: string }>>([]);
  const { catalog } = useWfmCatalog();
  const [prices, setPrices] = useState<{ map: PriceMap; done: boolean }>({ map: new Map(), done: false });
  const [relicDb, setRelicDb] = useState<RelicDatabase | null>(null);

  const lookup = useMemo(() => catalogLookup(catalog), [catalog]);
  const ownership = ownershipFor(inventory.data, itemDbRaw);

  // Platinum estimates need market prices; fetch them only once the roadmap is opened.
  const priceSlugs = useMemo(() => (viewTab === "roadmap" && data ? roadmapPriceSlugs(data.items, lookup) : []), [viewTab, data, lookup]);
  useEffect(() => {
    if (viewTab !== "roadmap" || !catalog) return;
    let live = true;
    void getRelicDatabase().then((db) => live && setRelicDb(db)).catch(() => undefined);
    void (async () => {
      const map = new Map<string, number>();
      for (let i = 0; i < priceSlugs.length && live; i += PRICE_CHUNK) {
        const entries = await wfmPricesBulk(priceSlugs.slice(i, i + PRICE_CHUNK)).catch(() => []);
        for (const e of entries) if (e.status === "ok" && e.price != null && e.price > 0) map.set(e.slug, e.price);
        if (live) setPrices({ map: new Map(map), done: false });
      }
      if (live) setPrices({ map, done: true });
    })();
    return () => {
      live = false;
    };
  }, [viewTab, catalog, priceSlugs]);

  const roadmap = useMemo(() => {
    if (viewTab !== "roadmap" || !data) return null;
    const building = inventory.data && Object.keys(itemDb).length ? foundryFor(inventory.data, itemDbRaw).building : [];
    const items = hydrateMasteryItems(data.items, { building, ownership, db: itemDb, lookup, prices: prices.map, now: Date.now() });
    return buildMasteryRoadmap(items, relicDb, parseOwnedRelics(inventory.data, relicDb));
  }, [viewTab, data, inventory.data, itemDb, ownership, lookup, prices.map, relicDb]);

  const plan = useMemo(() => {
    if (viewTab !== "planned" || pins.length === 0) return null;
    const byName = new Map((data?.items ?? []).map((i) => [i.uniqueName, i]));
    const planned: PlannerPin[] = pins.map((uniqueName) => {
      const match = byName.get(uniqueName);
      const entry = itemDb[uniqueName];
      const displayName = match?.displayName || entry?.displayName;
      return { uniqueName, name: match?.name || entry?.name || fallbackNameFromUniqueName(uniqueName), ...(displayName ? { displayName } : {}), imageUrl: match?.imageUrl ?? entry?.imageUrl ?? null, masteryXpRemaining: match?.masteryXpRemaining ?? 0 };
    });
    return buildMasteryPlan(planned, itemDb, ownership);
  }, [viewTab, pins, data, itemDb, ownership]);

  // A pinned item the account has since mastered drops off, with an undo.
  useEffect(() => {
    if (!data || pins.length === 0) return;
    const mastered = new Map(data.items.filter((i) => i.status === "mastered").map((i) => [i.uniqueName, label(i)]));
    const drop = pins.filter((u) => mastered.has(u) && !keepPinned.includes(u));
    if (drop.length === 0) return;
    unpinMasteryItems(drop);
    const text = drop.length === 1 ? tr("mastery.planner.autoUnpinnedOne", { name: mastered.get(drop[0]) ?? "" }) : tr("mastery.planner.autoUnpinnedMany", { count: drop.length });
    setNotices((n) => [...n, { id: Date.now() + n.length, dropped: drop, text }]);
  }, [data, pins, keepPinned, tr]);

  const categories = useMemo(() => {
    const keys = Object.keys(data?.stats.byCategory ?? {});
    return [...CAT_ORDER.filter((c) => keys.includes(c)), ...keys.filter((c) => !CAT_ORDER.includes(c)).sort((a, b) => a.localeCompare(b))];
  }, [data]);

  const shownAll = useMemo(() => {
    const q = query.trim().toLowerCase();
    const rows = (data?.items ?? []).filter(
      (i) =>
        (cat === "all" || i.category === cat) &&
        (status === "all" || i.status === status) &&
        (!q || label(i).toLowerCase().includes(q) || i.name.toLowerCase().includes(q) || i.keywords.some((k) => k.includes(q))),
    );
    const by: Record<string, (a: Item, b: Item) => number> = {
      name: () => 0,
      owned: (a, b) => Number(b.currentlyOwned) - Number(a.currentlyOwned),
      mastery_xp: (a, b) => (b.masteryXpRemaining ?? 0) - (a.masteryXpRemaining ?? 0),
    };
    return [...rows].sort((a, b) => by[sort](a, b) || label(a).localeCompare(label(b)));
  }, [data, cat, status, query, sort]);
  const { limit, more } = useBatch(`${cat}|${status}|${query}|${sort}`);
  const shown = shownAll.slice(0, limit);

  const header = (
    <>
      <PageHeader title={tr("mastery.title")} />
      {data?.fromProfile ? (
        <p className="ms-notice" data-mastery-from-profile>
          <Badge tone="info">{tr("profile.sourceBadge")}</Badge> {tr("mastery.fromProfileHint")}
        </p>
      ) : (
        <ImportNote tr={tr} />
      )}
      {notices.map((n) => (
        <div key={n.id} className="ms-notice" data-mastery-auto-unpin>
          <span>{n.text}</span>
          <span className="flex gap-2">
            <Button size="sm" onClick={() => (restoreMasteryPins(n.dropped), setNotices((l) => l.filter((x) => x.id !== n.id)))}>
              {tr("mastery.planner.undo")}
            </Button>
            <Button size="sm" variant="ghost" aria-label={tr("common.dismissNotification")} onClick={() => setNotices((l) => l.filter((x) => x.id !== n.id))}>
              {tr("common.dismiss")}
            </Button>
          </span>
        </div>
      ))}
      <div data-tour="mastery-view-tabs" className="mb-3">
        <Tabs label={tr("mastery.title")} items={VIEW_TABS.map((k) => ({ key: k, label: tr(VIEW_TAB_LABEL[k]), count: k === "planned" && pins.length > 0 ? pins.length : undefined }))} active={viewTab} onSelect={setViewTab} />
      </div>
    </>
  );

  if (viewTab === "codex") return <>{header}<CodexPanel tr={tr} locale={locale} /></>;
  if (viewTab === "incarnon") return <>{header}<IncarnonTracker tr={tr} itemDb={itemDb} inventory={inventory.data} /></>;

  if (!data) {
    return (
      <>
        {header}
        {data === null ? (
          <Card>
            <NoInventoryState tr={tr} why="noInventory.mastery" />
          </Card>
        ) : (
          <div className="ms-grid" aria-busy="true" aria-label={tr("mastery.loadingData")}>
            {[0, 1, 2, 3, 4, 5].map((i) => (
              <Card key={i} className="grid gap-3 p-4">
                <Skeleton width="45%" height="1.1rem" />
                <Skeleton width="70%" />
                <Skeleton />
              </Card>
            ))}
          </div>
        )}
      </>
    );
  }

  const openItem = (uniqueName: string) => openItemBy({ uniqueName });

  if (viewTab === "roadmap") {
    return (
      <>
        {header}
        {roadmap && (
          <MasteryRoadmap
            tr={tr}
            locale={locale}
            roadmap={roadmap}
            db={itemDb}
            totalXp={data.stats.profileMastery?.totalXp ?? null}
            currentRank={data.stats.profileMastery?.rank ?? null}
            pins={pins}
            pricing={!prices.done}
            onOpen={openItem}
            onTogglePin={toggleMasteryPin}
          />
        )}
      </>
    );
  }

  if (viewTab === "planned") {
    return (
      <>
        {header}
        <MasteryPlanner
          tr={tr}
          locale={locale}
          plan={plan ?? { items: [], totals: [], totalCredits: 0, craftableCount: 0 }}
          db={itemDb}
          inventory={inventory.data}
          sort={plannerSort}
          onSort={setPlannerSort}
          onUnpin={toggleMasteryPin}
          onOpenItem={openItem}
          onOpenComponent={setActiveComponent}
        />
      </>
    );
  }

  const { stats } = data;
  const pct = stats.total > 0 ? ((stats.mastered / stats.total) * 100).toFixed(1) : "0.0";
  const bars = display === "bars";
  const starRows: Array<[string, Pair]> = [
    [tr("common.normal"), stats.completion.starChart.normal],
    [tr("mastery.junctions"), stats.completion.starChart.junctions],
    [tr("common.steelPath"), stats.completion.starChart.steelPath],
    [tr("mastery.steelPathJunctions"), stats.completion.starChart.steelPathJunctions],
  ];
  const intrinsicRows: Array<[string, Pair]> = [
    [tr("world.railjack"), stats.completion.intrinsics.railjack],
    [tr("mastery.duviri"), stats.completion.intrinsics.drifter],
  ];
  const catTabs = [{ key: "all", label: tr("common.all"), count: stats.total }, ...categories.map((c) => ({ key: c, label: c, count: stats.byCategory[c].total }))];

  return (
    <>
      {header}

      <div className="ms-summary" data-mastery-summary>
        <Card className="ms-ring">
          <svg viewBox="0 0 120 120" role="img" aria-label={`${pct}% ${tr("common.mastered")}`}>
            <circle cx="60" cy="60" r={RING_R} fill="none" className="stroke-surface-hover" strokeWidth="8" />
            <circle cx="60" cy="60" r={RING_R} fill="none" className="stroke-accent" strokeWidth="8" strokeDasharray={RING_C} strokeDashoffset={RING_C * (1 - stats.mastered / Math.max(stats.total, 1))} strokeLinecap="round" transform="rotate(-90 60 60)" />
            <text x="60" y="58" textAnchor="middle" className="fill-text-primary" fontSize="22" fontWeight="600">
              {pct}%
            </text>
            <text x="60" y="76" textAnchor="middle" className="fill-text-muted" fontSize="9">
              {tr("common.mastered")}
            </text>
          </svg>
        </Card>
        <StatStrip items={summaryRows(data, tr, locale)} />
      </div>

      <div className="mb-3 grid gap-3">
        <Collapse
          open={expanded}
          onToggle={() => (writeStorage("mastery-breakdown-expanded", String(!expanded)), setExpanded(!expanded))}
          title={tr("mastery.detailedBreakdown")}
          tools={
            <span data-mastery-breakdown-display>
              <Segmented
                label={tr("mastery.breakdownStyle")}
                active={display}
                onSelect={setDisplay}
                items={(["bars", "rings"] as const).map((d) => ({ key: d, label: tr(d === "bars" ? "mastery.breakdownBars" : "mastery.breakdownRings") }))}
              />
            </span>
          }
        >
          <Card className={bars ? "ms-bars" : "ms-rings"}>
            {categories.map((c) => {
              const cs = stats.byCategory[c];
              return <MasteryBreakdownRow key={c} label={c} done={cs.mastered} total={cs.total} inProgress={cs.inProgress} display={display} />;
            })}
          </Card>
          <div className="ms-grid-2">
            {(
              [
                [tr("mastery.starChart"), starRows, "info"],
                [tr("mastery.intrinsics"), intrinsicRows, "accent"],
              ] as const
            ).map(([title, rows, tone]) => (
              <Card key={title} className={bars ? "ms-bars" : "ms-rings"}>
                <span className="ms-eyebrow col-span-full">{title}</span>
                {rows.map(([l, p]) => (
                  <MasteryBreakdownRow key={l} label={l} done={p.done} total={p.total} display={display} tone={tone} />
                ))}
              </Card>
            ))}
          </div>
        </Collapse>

        {archonSummary.stock.length > 0 && (
          <Collapse open={archonOpen} onToggle={() => (writeStorage("mastery-archon-expanded", String(!archonOpen)), setArchonOpen(!archonOpen))} title={tr("archon.title")}>
            <ArchonShardSummaryPanel
              summary={archonSummary}
              itemDb={itemDb}
              tr={tr}
              locale={locale}
              frameLabel={(u) => {
                const i = data.items.find((x) => x.uniqueName === u);
                return i ? label(i) : (u.split("/").pop() ?? u);
              }}
              onOpenFrame={openItem}
            />
          </Collapse>
        )}
      </div>

      <div className="ms-sticky">
        <div className="ms-filters ms-filters--start">
          <SearchInput className="ms-search" label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
          <Field label={tr("common.sort")}>
            <Select value={sort} options={SORTS.map(([k, key]) => [k, tr(key)] as const)} onChange={(e) => setSort(e.target.value)} />
          </Field>
          <Segmented label={tr("common.mastery")} active={status} onSelect={setStatus} items={STATUS_TABS.map(([k, key]) => ({ key: k, label: tr(key) }))} />
        </div>
        <Tabs label={tr("common.category")} items={catTabs} active={cat} onSelect={setCat} />
      </div>

      <div className="ms-grid" data-mastery-grid data-item-count={shownAll.length}>
        {shownAll.length === 0 ? (
          <Card className="col-span-full">
            <EmptyState title={tr("mastery.noItemsMatch")} />
          </Card>
        ) : (
          shown.map((item) => <MasteryCard key={item.uniqueName} item={item} tr={tr} locale={locale} shards={archon.bySuitType.get(item.uniqueName) ?? []} itemDb={itemDb} pinned={pins.includes(item.uniqueName)} />)
        )}
      </div>
      {shownAll.length > 0 && <ShowMore shown={shown.length} total={shownAll.length} onMore={more} tr={tr} />}
    </>
  );
}
