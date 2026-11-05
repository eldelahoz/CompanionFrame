import { ArrowDownWideNarrow, ArrowUpNarrowWide, CheckSquare, LayoutGrid, List, PackageSearch } from "lucide-react";
import ImportNote from "../components/inventory/ImportNote";
import { useCallback, useEffect, useMemo, useState, type ChangeEvent } from "react";
import InventorySourceBar from "../components/inventory/InventorySourceBar";
import InventoryCard from "../components/inventory/InventoryCard";
import InventoryList from "../components/inventory/InventoryList";
import InventoryOrderBookPanel from "../components/inventory/InventoryOrderBookPanel";
import InventorySelectionBar from "../components/inventory/InventorySelectionBar";
import InventoryValueStrip from "../components/inventory/InventoryValueStrip";
import PetsInventory from "../components/inventory/PetsInventory";
import SetExtra, { useRelicDb } from "../components/inventory/SetsPanel";
import BulkSellModal from "../components/workbench/BulkSellModal";
import { Button, Chip, EmptyState, IconButton, Input, PageHeader, Segmented, SearchInput, Select, ShowMore, Skeleton, Tabs, useBatch } from "../components/ui";
import type { MessageKey, Translator } from "../lib/i18n";
import type { ItemDatabase, Language } from "../lib/ipc";
import { useItemPriceProgress, useItemPrices, type PriceInfo } from "../lib/inventory/itemPrices";
import { buildIncompleteSets, buildRelicRewardIndex, partSlug } from "../lib/inventory/incompleteSets";
import { buildMasteryIndex, isNotMastered, masteryOfItem } from "../lib/inventory/masteryMark";
import { parseOwnedRelics } from "../lib/relic/relicInventory";
import { SORT_KEYS, buildRows, hasDucats, sortRows, valueEntries, type InvRow, type SortKey } from "../lib/inventory/inventoryRows";
import { GOALS, GOAL_FILTERS, PLAT_PRESETS, parseBound, passesPlatRange, presetKey, usePersistedRange, type Goal } from "../lib/inventory/invFilters";
import { computeInventoryValueTotals } from "../lib/inventory/valueTotals";
import { NO_MOD_FILTERS, modCopies, modFiltersOn, passesModFilters, type ModFilters } from "../lib/inventory/modFilters";
import { isUpgrade, toSellable, verdictResolver } from "../lib/inventory/sellable";
import { useInventorySafety } from "../stores/inventorySafety";
import { usePersisted } from "../lib/usePersisted";
import { useInventory, useParsedInventory, useResources } from "../stores/inventory";
import NoInventoryState from "../components/inventory/NoInventoryState";
import ProfileGear from "../components/inventory/ProfileGear";
import { clearSelection, selectKeys, setSelectionMode, toggleSelected, useSelection, useSelectionMode } from "../stores/inventorySelection";
import { useWfmCatalog } from "../stores/market";
import { useMasteryProgress } from "../stores/mastery";
import { setActiveItem } from "../stores/modals";
import { useWorkbenchState } from "../stores/workbench";
import type { InventoryGroup, ItemDbEntry } from "../types/inventory";
import ResourcesView from "./ResourcesView";

type Tab = InventoryGroup | "everything" | "resources" | "pets";

const TABS: ReadonlyArray<readonly [Tab, MessageKey]> = [
  ["everything", "inventory.tab.everything"],
  ["all_parts", "inventory.tab.allParts"],
  ["relics", "common.relics"],
  ["mods", "inventory.tab.mods"],
  ["arcanes", "inventory.tab.arcanes"],
  ["full_sets", "inventory.tab.fullSets"],
  ["equipment", "inventory.tab.equipment"],
  ["resources", "nav.resources"],
  ["pets", "inventory.tab.pets"],
  ["misc", "inventory.tab.misc"],
];

const SORT_LABEL: Record<SortKey, MessageKey> = {
  name: "inventory.sort.name",
  platinum: "inventory.sort.platinum",
  ducats: "inventory.sort.ducats",
  average: "inventory.sort.average",
  amount: "inventory.sort.amount",
};
const MODES = ["cards", "list"] as const;
const FLAG = ["0", "1"] as const;
/** Rows whose weekly median is fetched; the history endpoint is one request per item. */
const AVERAGE_CAP = 120;
/** Descending reads naturally for the figures, ascending for the name. */
const NO_PRICES: ReadonlyMap<string, PriceInfo> = new Map();
const defaultDesc = (key: SortKey) => key !== "name";

interface Props {
  items: ItemDatabase;
  tr: Translator;
  locale?: Language;
}

export default function InventoryView({ items, tr, locale = "en" }: Props) {
  const { data, status, loaded } = useInventory();
  const parsed = useParsedInventory(data, items);
  const resources = useResources(data, items);
  const { catalog } = useWfmCatalog();
  const { data: masteryData } = useMasteryProgress();
  const [tab, setTab] = useState<Tab>("everything");
  const [query, setQuery] = useState("");
  const [mode, setMode] = usePersisted("wf_inventory_view_mode", MODES, "cards");
  const [sortKey, setSortKey] = usePersisted("wf_inventory_sort", SORT_KEYS, "name");
  const [desc, setDesc] = useState(false);
  const [range, setRange] = usePersistedRange();
  const [customRange, setCustomRange] = useState(() => presetKey(range) === null);
  const [goal, setGoal] = useState<Goal>("all");
  const [tradableOnly, setTradableOnly] = useState(false);
  const [ducatsOnly, setDucatsOnly] = useState(false);
  const [primeWant, setPrimeWant] = useState<"all" | "prime" | "non">("all");
  const [masteryWant, setMasteryWant] = useState<"all" | "mastered" | "missing">("all");
  const [modWant, setModWant] = useState<ModFilters>(NO_MOD_FILTERS);
  const [surplusOnly, setSurplusOnly] = useState(false);
  const toggleMod = (key: keyof ModFilters) => setModWant((cur) => ({ ...cur, [key]: !cur[key] }));
  // A goal is a shortcut: it sets exactly its filters, and touching any filter by hand releases it.
  const pickGoal = (g: Goal) => {
    const f = GOAL_FILTERS[g];
    setGoal(g);
    setTradableOnly(f.tradableOnly);
    setDucatsOnly(f.ducatsOnly);
    setMasteryWant(f.masteryWant);
    setPrimeWant("all");
    setModWant(NO_MOD_FILTERS);
    setSurplusOnly(false);
    setRange(f.range);
    setCustomRange(false);
  };
  const manual =
    <A extends unknown[]>(fn: (...a: A) => void) =>
    (...a: A) => {
      setGoal("all");
      fn(...a);
    };
  const [incomplete, setIncomplete] = usePersisted("wf_inventory_incomplete_sets", FLAG, "0");
  const [bulkOpen, setBulkOpen] = useState(false);
  const [book, setBook] = useState<InvRow | null>(null);
  const selecting = useSelectionMode();
  const selection = useSelection();
  const [workbench] = useWorkbenchState();
  const bulkNeedsAttention = workbench?.reviewRequired === true || workbench?.phase === "running" || workbench?.phase === "cancelling";

  const listed = tab !== "resources" && tab !== "pets";
  const rows = useMemo(() => buildRows(parsed, catalog), [parsed, catalog]);
  const showIncomplete = tab === "full_sets" && incomplete === "1";
  const inTab = useMemo(() => {
    const q = query.trim().toLowerCase();
    return rows.filter(({ item, label }) => {
      const group = item.inventoryGroup;
      if (tab === "everything" ? group === "full_sets" || group === "incomplete_sets" : group !== tab && !(showIncomplete && group === "incomplete_sets")) return false;
      return !q || label.toLowerCase().includes(q) || item.name.toLowerCase().includes(q);
    });
  }, [rows, tab, query, showIncomplete]);
  // Counts follow the search box, so a tab never promises rows the search already hid.
  const counts = useMemo(() => {
    const out: Partial<Record<Tab, number>> = {};
    const q = query.trim().toLowerCase();
    for (const { item, label } of rows) {
      if (q && !label.toLowerCase().includes(q) && !item.name.toLowerCase().includes(q)) continue;
      const g = item.inventoryGroup;
      if (g) out[g] = (out[g] ?? 0) + 1;
      if (g !== "full_sets" && g !== "incomplete_sets") out.everything = (out.everything ?? 0) + 1;
    }
    return out;
  }, [rows, query]);
  const masteryIndex = useMemo(() => buildMasteryIndex(masteryData?.items ?? []), [masteryData]);
  const masteryOf = (row: InvRow) => masteryOfItem(row.item, masteryIndex, items as unknown as Record<string, ItemDbEntry>);
  // Groups holding anything masterable: derived, so the toggle follows the data. Only there does it show or filter.
  const masterableGroups = useMemo(() => {
    const out = new Set<string>();
    for (const r of rows) if (r.item.inventoryGroup && !out.has(r.item.inventoryGroup) && masteryOf(r) !== null) out.add(r.item.inventoryGroup);
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, masteryIndex, items]);
  const canFilterMastery = tab === "everything" ? [...masterableGroups].some((g) => g !== "full_sets" && g !== "incomplete_sets") : masterableGroups.has(tab) || (tab === "full_sets" && masterableGroups.has("incomplete_sets"));
  const masteryFilter = canFilterMastery ? masteryWant : "all";
  // Ducat rows are unbuilt Prime parts, all in one group; the toggle shows only where such rows exist.
  const canFilterDucats = useMemo(() => rows.some((r) => hasDucats(r) && (tab === "everything" || r.item.inventoryGroup === tab)), [rows, tab]);
  // Prime and non-Prime rows mixed: the chips show only where they would split the tab.
  const canFilterPrime = useMemo(() => {
    const seen = new Set<boolean>();
    for (const r of rows) if (tab === "everything" ? r.item.inventoryGroup !== "full_sets" && r.item.inventoryGroup !== "incomplete_sets" : r.item.inventoryGroup === tab) seen.add(!!r.item.isPrime);
    return seen.size === 2;
  }, [rows, tab]);
  const primeFilter = canFilterPrime ? primeWant : "all";
  const ducatsFilter = ducatsOnly && canFilterDucats;
  // The mod chips show only where mods or arcanes are listed; a copy count spans every rank row, whatever the search hides.
  const canFilterMods = useMemo(() => (tab === "everything" || tab === "mods" || tab === "arcanes") && rows.some((r) => isUpgrade(r.item) && (tab === "everything" || r.item.inventoryGroup === tab)), [rows, tab]);
  const modFilter = canFilterMods ? modWant : NO_MOD_FILTERS;
  const copies = useMemo(() => modCopies(rows), [rows]);
  // The safety context (mastery, foundry walk) is only worth building while a chip asks for the sellable count.
  const showSellable = surplusOnly || modFiltersOn(modFilter);
  const safety = useInventorySafety(showSellable ? data : null, items as unknown as Record<string, ItemDbEntry>);
  const sellable = useMemo(() => {
    if (!showSellable) return null;
    // Mods and arcanes are judged over every rank row, so the resolver reads the whole inventory, not the tab.
    const verdictOf = verdictResolver(parsed, safety.context);
    return new Map(inTab.filter((r) => r.item.inventoryGroup !== "incomplete_sets").map((r) => [r.key, toSellable(verdictOf(r.item))] as const));
  }, [showSellable, parsed, inTab, safety.context]);
  const sellableOf = (row: InvRow) => sellable?.get(row.key);
  const filtered = useMemo(
    () => inTab.filter((r) => (!tradableOnly || r.canTrade) && (!ducatsFilter || hasDucats(r)) && passesModFilters(r, copies, modFilter) && (!surplusOnly || (sellable?.get(r.key)?.count ?? 0) > 0) && (primeFilter === "all" || !!r.item.isPrime === (primeFilter === "prime")) && (masteryFilter === "all" || (masteryFilter === "mastered" ? masteryOf(r)?.status === "mastered" : isNotMastered(masteryOf(r))))),
    // masteryOf only reads masteryIndex and items.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [inTab, tradableOnly, ducatsFilter, copies, modFilter, surplusOnly, sellable, primeFilter, masteryFilter, masteryIndex, items],
  );
  const { limit, more } = useBatch(`${tab}|${query}|${sortKey}|${desc}|${tradableOnly}|${ducatsFilter}|${modFilter.duplicates}|${modFilter.notMax}|${modFilter.notEquipped}|${surplusOnly}|${primeFilter}|${masteryFilter}|${range.min}|${range.max}|${mode}`);

  // Prices: the rows on screen first, then the rest of the tab and every row the value strip counts, in the background.
  // Missing parts of incomplete sets are priced like any row: through the same queue.
  const partSlugs = useCallback(
    (list: readonly InvRow[]) =>
      buildIncompleteSets(list.filter((r) => r.item.inventoryGroup === "incomplete_sets").map((r) => r.item), null, {}, undefined, (part, set) => partSlug(part, set, (n) => catalog?.byName(n) ?? null)).flatMap((e) =>
        e.missing.flatMap((m) => (m.slug ? [m.slug] : [])),
      ),
    [catalog],
  );
  const allSlugs = useMemo(() => {
    const wanted = new Set<string>();
    for (const r of filtered) if (r.slug) wanted.add(r.slug);
    for (const slug of partSlugs(filtered)) wanted.add(slug);
    for (const r of rows) if (r.slug && r.canTrade) wanted.add(r.slug);
    return [...wanted];
  }, [filtered, rows, partSlugs]);
  // Price order is unknown before the prices are, so the first rows asked are the head of the unpriced order.
  const head = useMemo(() => sortRows(filtered, NO_PRICES, sortKey, desc).slice(0, limit), [filtered, sortKey, desc, limit]);
  const headSlugs = useMemo(() => [...head.flatMap((r) => (r.slug ? [r.slug] : [])), ...partSlugs(head)], [head, partSlugs]);
  // The weekly median costs one request per item, so only the first rows on screen and the picked ones get it.
  const [loadedAvg, setLoadedAvg] = useState<ReadonlySet<string>>(new Set());
  const loadAverage = useCallback((slug: string) => setLoadedAvg((s) => new Set(s).add(slug)), []);
  const avgSlugs = useMemo(
    () => [...new Set([...head.slice(0, AVERAGE_CAP), ...rows.filter((r) => selection.has(r.key))].flatMap((r) => (r.slug ? [r.slug] : [])).concat([...loadedAvg]))],
    [head, rows, selection, loadedAvg],
  );
  const avgWanted = useMemo(() => new Set(avgSlugs), [avgSlugs]);
  const livePrices = useItemPrices(headSlugs, listed ? avgSlugs : [], allSlugs);
  const progress = useItemPriceProgress(allSlugs);
  // The platinum range filters the list: with a bound set, no market price hides the row; rows still loading stay.
  const visible = useMemo(
    () => sortRows(filtered, livePrices, sortKey, desc).filter((r) => passesPlatRange(r, r.slug ? livePrices.get(r.slug) : undefined, range)),
    [filtered, livePrices, sortKey, desc, range],
  );
  const shown = useMemo(() => visible.slice(0, limit), [visible, limit]);
  // Relic sources only for the sets on screen: each one walks the whole relic database.
  const relicDb = useRelicDb(showIncomplete);
  const relicIndex = useMemo(() => (relicDb ? buildRelicRewardIndex(relicDb) : null), [relicDb]);
  const ownedRelics = useMemo(() => parseOwnedRelics(data, relicDb), [data, relicDb]);
  const setEntries = useMemo(
    () =>
      new Map(
        buildIncompleteSets(shown.filter((r) => r.item.inventoryGroup === "incomplete_sets").map((r) => r.item), relicIndex, ownedRelics, undefined, (part, set) =>
          partSlug(part, set, (n) => catalog?.byName(n) ?? null),
        ).map((e) => [e.set, e]),
      ),
    [shown, relicIndex, ownedRelics, catalog],
  );
  const extraOf = (row: InvRow) => {
    const entry = setEntries.get(row.item);
    return entry ? <SetExtra entry={entry} prices={livePrices} itemDb={items as unknown as Record<string, ItemDbEntry>} tr={tr} /> : null;
  };

  // Every filtered row counts, not just the ones the scroll has shown.
  const filteredTotals = useMemo(() => computeInventoryValueTotals(valueEntries(visible.filter((r) => r.canTrade), livePrices), "tradable"), [visible, livePrices]);
  const allTradableTotals = useMemo(() => computeInventoryValueTotals(valueEntries(rows.filter((r) => r.canTrade), livePrices), "tradable"), [rows, livePrices]);
  const picked = useMemo(() => rows.filter((r) => selection.has(r.key)), [rows, selection]);
  const pickedTotals = useMemo(
    () => (selecting && picked.length > 0 ? computeInventoryValueTotals(valueEntries(picked, livePrices), "tradable") : null),
    [selecting, picked, livePrices],
  );

  const total = parsed.filter((i) => i.inventoryGroup !== "full_sets" && i.inventoryGroup !== "incomplete_sets").length;
  const eligible = useMemo(() => visible.filter((r) => r.canTrade && r.item.inventoryGroup !== "incomplete_sets"), [visible]);
  const visibleKeys = useMemo(() => new Set(visible.map((r) => r.key)), [visible]);
  const onTabCount = useMemo(() => [...selection].filter((k) => visibleKeys.has(k)).length, [selection, visibleKeys]);

  // Leaving the grid for another view must not leave the next visit in a half-picked state.
  useEffect(() => () => setSelectionMode(false), []);
  const toggleMode = () => setSelectionMode(!selecting);
  const open = (row: InvRow) => setActiveItem(row.item);
  const pickSort = (key: SortKey) => {
    setSortKey(key);
    setDesc(defaultDesc(key));
  };

  return (
    <div className="inventory-view">
      <PageHeader
        title={tr("inventory.title", { count: total })}
        actions={
          listed && (
            <>
              <Button
                size="sm"
                icon={<CheckSquare />}
                aria-pressed={selecting}
                data-inventory-select-toggle
                title={tr("inventory.selectModeHint")}
                className={selecting ? "is-active" : undefined}
                onClick={toggleMode}
              >
                {tr("inventory.selectMode")}
              </Button>
              {!selecting && (
                // shortcut: the queue starts from the visible tab unless items were picked in the selection mode.
                <Button size="sm" variant="primary" data-bulk-sell-open disabled={eligible.length === 0} onClick={() => setBulkOpen(true)}>
                  {tr("inventory.bulkSell", { count: eligible.length })}
                </Button>
              )}
            </>
          )
        }
      />
      <InventorySourceBar status={status} tr={tr} />
      {bulkNeedsAttention && (
        // Bulk Sell is only a modal, so this banner is the one way back to an unreconciled run.
        <div className="inv-banner" data-bulk-sell-banner role="status">
          <span>{tr(workbench?.reviewRequired ? "inventory.bulkSellReviewRequired" : "inventory.bulkSellRunActive")}</span>
          <Button size="sm" data-bulk-sell-banner-open onClick={() => setBulkOpen(true)}>
            {tr("inventory.openBulkSell")}
          </Button>
        </div>
      )}

      <div className="inv-sticky">
        <Tabs label={tr("inventory.title", { count: total })} active={tab} onSelect={setTab} items={TABS.map(([key, label]) => ({ key, label: tr(label), count: key === "resources" || key === "pets" ? undefined : (counts[key] ?? 0) }))} />
        {tab !== "resources" && (
          <div className="inv-toolbar">
            <SearchInput label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} value={query} onChange={(e) => setQuery(e.target.value)} className="inv-search" />
            {listed && (
              <>
                <span className="inv-sort">
                  <Select aria-label={tr("inventory.valueSort")} value={sortKey} options={SORT_KEYS.map((k) => [k, tr(SORT_LABEL[k])] as const)} onChange={(e) => pickSort(e.target.value as SortKey)} />
                  <IconButton label={tr("inventory.sortDirection")} onClick={() => setDesc(!desc)}>
                    {desc ? <ArrowDownWideNarrow /> : <ArrowUpNarrowWide />}
                  </IconButton>
                </span>
                <Segmented
                  label={tr("inventory.goal.label")}
                  active={goal}
                  onSelect={pickGoal}
                  items={GOALS.map((g) => ({ key: g, label: tr(`inventory.goal.${g}` as MessageKey) }))}
                />
                <Chip pressed={tradableOnly} onClick={manual(() => setTradableOnly(!tradableOnly))}>
                  {tr("inventory.tradableOnly")}
                </Chip>
                <Chip pressed={surplusOnly} data-inventory-surplus onClick={manual(() => setSurplusOnly(!surplusOnly))}>
                  {tr("inventory.surplus")}
                </Chip>
                {canFilterDucats && (
                  <Chip pressed={ducatsOnly} data-inventory-ducats onClick={manual(() => setDucatsOnly(!ducatsOnly))}>
                    {tr("common.ducats")}
                  </Chip>
                )}
                {canFilterMods && (
                  <>
                    <Chip pressed={modWant.duplicates} data-inventory-mod-duplicates onClick={manual(() => toggleMod("duplicates"))}>
                      {tr("inventory.mods.duplicates")}
                    </Chip>
                    <Chip pressed={modWant.notMax} data-inventory-mod-not-max onClick={manual(() => toggleMod("notMax"))}>
                      {tr("inventory.mods.notMax")}
                    </Chip>
                    <Chip pressed={modWant.notEquipped} data-inventory-mod-not-equipped onClick={manual(() => toggleMod("notEquipped"))}>
                      {tr("inventory.mods.notEquipped")}
                    </Chip>
                  </>
                )}
                {canFilterPrime && (
                  <>
                    <Chip pressed={primeWant === "prime"} data-inventory-prime onClick={manual(() => setPrimeWant(primeWant === "prime" ? "all" : "prime"))}>
                      {tr("inventory.primeOnly")}
                    </Chip>
                    <Chip pressed={primeWant === "non"} data-inventory-non-prime onClick={manual(() => setPrimeWant(primeWant === "non" ? "all" : "non"))}>
                      {tr("inventory.nonPrime")}
                    </Chip>
                  </>
                )}
                {canFilterMastery && (
                  <>
                    <Chip pressed={masteryWant === "mastered"} data-inventory-mastered onClick={manual(() => setMasteryWant(masteryWant === "mastered" ? "all" : "mastered"))}>
                      {tr("common.mastered")}
                    </Chip>
                    <Chip pressed={masteryWant === "missing"} data-inventory-not-mastered onClick={manual(() => setMasteryWant(masteryWant === "missing" ? "all" : "missing"))}>
                      {tr("inventory.notMastered")}
                    </Chip>
                  </>
                )}
                <Select
                  aria-label={tr("inventory.plat.label")}
                  title={tr("inventory.plat.hint")}
                  value={customRange ? "custom" : (presetKey(range) ?? "custom")}
                  options={[
                    ["any", `${tr("inventory.plat.label")}: ${tr("inventory.plat.any")}`] as const,
                    ...PLAT_PRESETS.map(([key, label]) => [key, `${tr("inventory.plat.label")}: ${label}`] as const),
                    ["custom", `${tr("inventory.plat.label")}: ${tr("inventory.plat.custom")}`] as const,
                  ]}
                  onChange={manual((e: ChangeEvent<HTMLSelectElement>) => {
                    const key = e.target.value;
                    setCustomRange(key === "custom");
                    if (key !== "custom") setRange(PLAT_PRESETS.find(([k]) => k === key)?.[2] ?? { min: null, max: null });
                  })}
                />
                {(customRange || presetKey(range) === null) && (
                  <span className="inv-plat-custom">
                    <Input type="number" min={0} aria-label={tr("inventory.plat.from")} placeholder={tr("inventory.plat.from")} value={range.min ?? ""} onChange={manual((e: ChangeEvent<HTMLInputElement>) => setRange({ ...range, min: parseBound(e.target.value) }))} />
                    <Input type="number" min={0} aria-label={tr("inventory.plat.to")} placeholder={tr("inventory.plat.to")} value={range.max ?? ""} onChange={manual((e: ChangeEvent<HTMLInputElement>) => setRange({ ...range, max: parseBound(e.target.value) }))} />
                  </span>
                )}
                {tab === "full_sets" && (
                  <Chip pressed={incomplete === "1"} data-sets-incomplete-toggle onClick={() => setIncomplete(incomplete === "1" ? "0" : "1")}>
                    {tr("inventory.sets.show", { count: counts.incomplete_sets ?? 0 })}
                  </Chip>
                )}
                <Segmented
                  label={tr("inventory.viewMode")}
                  active={mode}
                  onSelect={setMode}
                  items={[
                    { key: "cards", label: tr("inventory.viewGrid"), icon: <LayoutGrid /> },
                    { key: "list", label: tr("inventory.viewList"), icon: <List /> },
                  ]}
                />
              </>
            )}
          </div>
        )}
        {selecting && listed && (
          <InventorySelectionBar
            count={selection.size}
            onTabCount={onTabCount}
            eligibleCount={eligible.length}
            onSelectAll={() => selectKeys(eligible.map((r) => r.key))}
            onClear={clearSelection}
            onBulkSell={() => setBulkOpen(true)}
            tr={tr}
          />
        )}
      </div>

      {listed && (
        <InventoryValueStrip
          filtered={filteredTotals}
          filteredCount={visible.length}
          allTradable={allTradableTotals}
          selection={pickedTotals}
          selectionCount={picked.length}
          locale={locale}
          tr={tr}
        />
      )}

      {listed && progress.total > 0 && progress.done < progress.total && (
        <div className="inv-pricing" role="status" data-inventory-pricing>
          {progress.failed > 0 ? (
            <>
              <span>{tr("inventory.pricingFailed", { count: progress.failed })}</span>
              <Button size="sm" data-inventory-pricing-retry onClick={progress.retry}>
                {tr("inventory.pricingRetry")}
              </Button>
            </>
          ) : (
            <span>{tr("inventory.pricing", { done: progress.done, total: progress.total })}</span>
          )}
        </div>
      )}

      <ImportNote tr={tr} locale={locale} />

      {loaded && !data ? (
        <>
          <NoInventoryState tr={tr} why="noInventory.inventory" />
          <ProfileGear tr={tr} items={items} />
        </>
      ) : tab === "resources" ? (
        <ResourcesView resources={resources} tr={tr} />
      ) : tab === "pets" ? (
        <PetsInventory inventory={data} items={items} locale={locale} query={query} tr={tr} />
      ) : !loaded || (loaded && data && catalog === null && rows.length === 0) ? (
        <div className="inv-grid" aria-busy="true">
          {Array.from({ length: 12 }, (_, i) => (
            <Skeleton key={i} height="9.5rem" />
          ))}
        </div>
      ) : visible.length === 0 ? (
        <EmptyState icon={<PackageSearch />} title={tr("inventory.noItemsFound")} />
      ) : (
        <div className="inv-layout">
          <div className="min-w-0">
            {mode === "cards" ? (
              <div className="inv-grid">
                {shown.map((row) => (
                  <InventoryCard
                    key={row.key}
                    row={row}
                    info={row.slug ? livePrices.get(row.slug) : undefined}
                    avgWanted={!!row.slug && avgWanted.has(row.slug)}
                    onLoadAverage={loadAverage}
                    selecting={selecting}
                    selected={selection.has(row.key)}
                    onOpen={() => open(row)}
                    onToggle={() => toggleSelected(row.key)}
                    onBook={() => setBook(row)}
                    extra={extraOf(row)}
                    mastery={masteryOf(row)}
                    sellable={sellableOf(row)}
                    tr={tr}
                  />
                ))}
              </div>
            ) : (
              <InventoryList rows={shown} prices={livePrices} avgWanted={avgWanted} onLoadAverage={loadAverage} selecting={selecting} selection={selection} onOpen={open} onToggle={(r) => toggleSelected(r.key)} onBook={setBook} extraOf={extraOf} masteryOf={masteryOf} sellableOf={sellableOf} tr={tr} />
            )}
            <ShowMore shown={shown.length} total={visible.length} onMore={more} tr={tr} />
          </div>
          {book && <InventoryOrderBookPanel key={book.key} row={book} onClose={() => setBook(null)} tr={tr} />}
        </div>
      )}

      {bulkOpen && (
        <BulkSellModal
          items={parsed}
          selection={new Set(selecting && selection.size > 0 ? [...selection].filter((k) => rows.some((r) => r.key === k && r.canTrade)) : eligible.map((r) => r.key))}
          inventory={data}
          itemDb={items as unknown as Record<string, ItemDbEntry>}
          catalog={catalog}
          locale={document.documentElement.lang || "en"}
          tr={tr}
          onClose={() => setBulkOpen(false)}
        />
      )}
    </div>
  );
}
