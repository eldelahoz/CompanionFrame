import { Clock, Pin, PinOff, SearchX } from "lucide-react";
import ImportNote from "../components/inventory/ImportNote";
import { useEffect, useMemo, useState } from "react";
import IngredientChips, { type Chip } from "../components/foundry/IngredientChips";
import VirtualCards from "../components/inventory/VirtualCards";
import ItemImage from "../components/ItemImage";
import SortControl from "../components/SortControl";
import { Badge, Button, Card, EmptyState, Field, IconButton, PageHeader, Readout, SearchInput, Select as UiSelect, StatStrip, Tabs, type Tone } from "../components/ui";
import { buildPartState, collectRecipeMaterialNames } from "../lib/craftingTree";
import { DEFAULT_FILTERS, STATE_BY_STATUS, defaultSortDir, matches, sortRows, statusOf, toEntries, type FoundryFilters, type ItemStatus, type Row, type SortKey } from "../lib/foundryView";
import { formatBuildTime, formatNumber, formatTimeRemaining } from "../lib/format";
import { isFrameSubsumed, isSubsumableFrame } from "../lib/helminth";
import type { MessageKey, Translator } from "../lib/i18n";
import { EQUIPMENT_CATEGORY_ORDER } from "../lib/inventory/foundryResources";
import { type ItemDatabase } from "../lib/ipc";
import { itemLabel } from "../lib/itemLabel";
import { buildMasteryLookup, inheritedMasteryStatus } from "../lib/masteryLookup";
import { buildParsedItemFromDb } from "../lib/parsedItemFromDb";
import { computePinnedTotals } from "../lib/pinnedSummary";
import { readStorage, writeStorage } from "../lib/persistence";
import { usePersisted } from "../lib/usePersisted";
import { chainFor, foundryFor, ownershipFor, subsumedFor, useInventory } from "../stores/inventory";
import { useMasteryProgress } from "../stores/mastery";
import NoInventoryState from "../components/inventory/NoInventoryState";
import { setActiveItem } from "../stores/modals";
import type { ItemDbEntry, MasteryStatus } from "../types/inventory";

const FILTERS_KEY = "foundryView.filters";
const PINS_KEY = "foundry.pinnedRecipes";
const TAB_KEY = "foundryView.filter";
const STATUS_TABS = [
  ["all", "common.all"],
  ["status:in-progress", "common.inProgress"],
] as const;
const TAB_KEYS: string[] = [...STATUS_TABS.map(([k]) => k), ...EQUIPMENT_CATEGORY_ORDER.map((c) => `cat:${c}`)];

const STATUS_LABEL: Record<ItemStatus, MessageKey> = { claimable: "common.ready", "in-progress": "foundry.status.building", "ready-to-build": "foundry.status.readyToBuild", "not-ready": "foundry.status.missingParts" };
const MASTERY_LABEL: Record<MasteryStatus | "unknown", MessageKey> = { mastered: "common.mastered", progress: "common.inProgress", missing: "common.notMastered", unknown: "foundry.mastery.notApplicable" };
const MASTERY_TONE: Record<MasteryStatus | "unknown", Tone> = { mastered: "success", progress: "warning", missing: "neutral", unknown: "neutral" };

const load = <T,>(key: string, fallback: T, ok: (v: unknown) => boolean): T => {
  try {
    const v: unknown = JSON.parse(readStorage(key) ?? "null");
    return ok(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
};

interface SelectProps {
  label: string;
  value: string;
  options: ReadonlyArray<readonly [string, string]>;
  onChange: (v: string) => void;
  stateAttr?: boolean;
}
const Select = ({ label, value, options, onChange, stateAttr }: SelectProps) => (
  <Field label={label}>
    <UiSelect value={value} options={options} onChange={(e) => onChange(e.target.value)} data-foundry-state={stateAttr ? "" : undefined} />
  </Field>
);

interface Props {
  items: ItemDatabase;
  tr: Translator;
}

export default function FoundryView({ items, tr }: Props) {
  const db = items as unknown as Record<string, ItemDbEntry>;
  const { data, loaded } = useInventory();
  const { data: mastery } = useMasteryProgress();
  const [now, setNow] = useState(() => Date.now());
  const [filters, setFilters] = useState<FoundryFilters>(() => ({ ...DEFAULT_FILTERS, ...load<Partial<FoundryFilters>>(FILTERS_KEY, {}, (v) => typeof v === "object" && v !== null) }));
  const [tab, setTab] = usePersisted(TAB_KEY, TAB_KEYS, "all");
  const [pins, setPins] = useState<string[]>(() => load<string[]>(PINS_KEY, [], Array.isArray));

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      clearInterval(timer);
    };
  }, []);

  const patch = (p: Partial<FoundryFilters>) =>
    setFilters((f) => {
      const next = { ...f, ...p };
      writeStorage(FILTERS_KEY, JSON.stringify(next));
      return next;
    });
  const setPinList = (next: string[]) => {
    setPins(next);
    writeStorage(PINS_KEY, JSON.stringify(next));
  };

  const foundry = foundryFor(data, items);
  const owned = ownershipFor(data, items);
  const entries = useMemo(() => toEntries(foundry.building, foundry.recipes), [foundry]);
  const chain = chainFor(foundry, owned, items);
  const lookup = useMemo(() => buildMasteryLookup(mastery), [mastery]);
  const subsumedFamilies = subsumedFor(data, items);

  const rows: Row[] = useMemo(
    () =>
      entries.map((e) => ({
        e,
        status: statusOf(e, now, owned, chain),
        mastery: mastery ? (inheritedMasteryStatus(lookup, db, e.productUniqueName, e.name) ?? "missing") : "unknown",
        subsumed: e.category === "Warframe" && isSubsumableFrame(e.name) ? isFrameSubsumed(e.name, subsumedFamilies) : undefined,
        keywords: e.productUniqueName ? collectRecipeMaterialNames(e.productUniqueName, db) : [],
      })),
    [entries, now, owned, chain, mastery, lookup, db, subsumedFamilies],
  );

  const pinnedSet = useMemo(() => new Set(pins), [pins]);
  const pinned = useMemo(() => computePinnedTotals(entries, pinnedSet, (u) => owned.get(u) ?? 0), [entries, pinnedSet, owned]);
  const visible = useMemo(() => {
    const passTab = (r: Row) => tab === "all" || (tab === "status:in-progress" ? r.status === "in-progress" || r.status === "claimable" : r.e.category === tab.slice(4));
    return sortRows(
      rows.filter((r) => passTab(r) && matches(r, filters, db)),
      filters,
      pinnedSet,
      now,
    );
  }, [rows, tab, filters, db, pinnedSet, now]);

  const name = (un: string) => db[un]?.name ?? un.split("/").pop() ?? un;
  const image = (un: string) => (db[un]?.imageUrl as string | null | undefined) ?? null;
  const open = (un: string | null) => {
    const entry = un ? db[un] : undefined;
    if (un && entry) setActiveItem(buildParsedItemFromDb(un, entry, owned));
  };
  const togglePin = (un: string) => setPinList(pins.includes(un) ? pins.filter((p) => p !== un) : [...pins, un].slice(-60));

  const sortOptions = [
    ["count", tr("foundry.sort.count")],
    ["time", tr("foundry.sort.time")],
    ["name", tr("common.name")],
  ] as const;
  const all = [["all", tr("common.all")]] as const;
  const count = (status: ItemStatus) => rows.filter((r) => r.status === status).length;
  const tabs = [...STATUS_TABS.map(([key, label]) => ({ key: key as string, label: tr(label) })), ...EQUIPMENT_CATEGORY_ORDER.map((c) => ({ key: `cat:${c}`, label: c }))];

  const chipsOf = (ingredients: Row["e"]["ingredients"]): Chip[] =>
    ingredients.map((ing, n) => {
      const have = owned.get(ing.uniqueName) ?? 0;
      const ok = have >= ing.count;
      return { key: `${ing.uniqueName}:${n}`, uniqueName: ing.uniqueName, name: name(ing.uniqueName), image: image(ing.uniqueName), have, need: ing.count, state: ok ? "owned" : buildPartState(ing, owned, db) === "blueprint" ? "blueprint" : "missing" };
    });

  const card = (row: Row, i: number) => {
    const { e, status } = row;
    const owns = e.productUniqueName ? (owned.get(e.productUniqueName) ?? 0) : 0;
    const pinnable = e.source === "blueprint" && !!e.uniqueName;
    const isPinned = pinnable && pinnedSet.has(e.uniqueName ?? "");
    const title = itemLabel(e.uniqueName ?? e.name, e);
    return (
      <div key={`${e.source}:${e.uniqueName ?? e.name}:${i}`} className="fd-card" data-foundry-card={e.uniqueName ?? e.name} data-status={STATE_BY_STATUS[status]}>
        <button type="button" disabled={!e.productUniqueName} onClick={() => open(e.productUniqueName)} aria-current={isPinned || undefined} className="fd-card-main">
          <div className="fd-card-head">
            <span className="ms-thumb">
              <ItemImage src={e.imageUrl} alt="" cls="max-h-full max-w-full object-contain" tr={tr} />
            </span>
            <div className="grid min-w-0 flex-1 gap-1">
              <span className="ms-name">
                <span className="truncate" title={title}>{title}</span>
                {e.source === "blueprint" && <b className="text-accent">×{e.count}</b>}
              </span>
              <div className="flex items-center gap-2 overflow-hidden whitespace-nowrap">
                <span className={`ms-eyebrow fd-state--${STATE_BY_STATUS[status]}`}>{status === "in-progress" && e.endDate ? formatTimeRemaining(e.endDate, now) : tr(STATUS_LABEL[status])}</span>
                {e.source === "blueprint" && e.isIngredient && <Badge>{tr("foundry.usedInCrafting")}</Badge>}
              </div>
            </div>
          </div>
          <div className="fd-chips-slot">
            {e.ingredients.length > 0 ? <IngredientChips chips={chipsOf(e.ingredients)} tr={tr} /> : e.source === "blueprint" && <span className="italic text-text-muted">{tr("foundry.noRecipeData")}</span>}
          </div>
          <div className="fd-card-foot">
            <div className="flex min-w-0 items-center gap-3 overflow-hidden">
              {e.buildPrice > 0 && <Readout currency="credits" value={e.buildPrice} />}
              {e.source === "blueprint" && e.buildTime > 0 && (
                <span className="inline-flex items-center gap-1 tabular-nums">
                  <Clock className="h-4 w-4" aria-hidden="true" />
                  {formatBuildTime(e.buildTime)}
                </span>
              )}
            </div>
            <div className="flex shrink-0 items-center justify-end gap-1.5">
              <Badge tone={owns > 0 ? "success" : "neutral"} title={tr("foundry.ownedCopies", { count: owns })}>
                {tr("foundry.ownedCount", { count: owns })}
              </Badge>
              <Badge tone={MASTERY_TONE[row.mastery]} title={tr(MASTERY_LABEL[row.mastery])}>
                {tr(MASTERY_LABEL[row.mastery])}
              </Badge>
            </div>
          </div>
        </button>
        {pinnable && (
          <IconButton className="fd-pin" label={tr(isPinned ? "foundry.unpinBlueprint" : "foundry.pinBlueprint")} aria-pressed={isPinned} data-foundry-pin onClick={() => togglePin(e.uniqueName ?? "")}>
            {isPinned ? <PinOff /> : <Pin />}
          </IconButton>
        )}
      </div>
    );
  };

  return (
    <div>
      <PageHeader title={tr("common.foundry")} />
      <ImportNote tr={tr} />
      <StatStrip
        className="mb-3"
        items={[
          { key: "claimable", label: tr("common.ready"), value: count("claimable"), tone: count("claimable") > 0 ? "success" : "default" },
          { key: "building", label: tr("foundry.status.building"), value: count("in-progress") },
          { key: "buildable", label: tr("foundry.status.readyToBuild"), value: count("ready-to-build") },
          { key: "missing", label: tr("foundry.status.missingParts"), value: count("not-ready") },
        ]}
      />

      <div className="ms-sticky">
        <div className="ms-filters ms-filters--start" data-foundry-filters>
          <SearchInput className="ms-search" label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} value={filters.search} onChange={(e) => patch({ search: e.target.value })} />
          <Select label={tr("common.prime")} value={filters.prime} onChange={(v) => patch({ prime: v as FoundryFilters["prime"] })} options={[...all, ["prime", tr("common.prime")], ["non_prime", tr("filters.nonPrime")]]} />
          <Select label={tr("common.mastery")} value={filters.mastery} onChange={(v) => patch({ mastery: v as FoundryFilters["mastery"] })} options={[...all, ["mastered", tr("common.mastered")], ["not_mastered", tr("common.notMastered")]]} />
          <Select stateAttr label={tr("filters.claimLabel")} value={filters.claim} onChange={(v) => patch({ claim: v as FoundryFilters["claim"] })} options={[...all, ["claimable", tr("common.ready")], ["not_ready", tr("filters.notReady")], ["buildable", tr("common.canBuild")], ["buildable_sets", tr("filters.buildableSets")]]} />
          <Select label={tr("common.vaulted")} value={filters.vaulted} onChange={(v) => patch({ vaulted: v as FoundryFilters["vaulted"] })} options={[...all, ["yes", tr("common.vaulted")], ["no", tr("common.unvaulted")]]} />
          <Select label={tr("common.subsumed")} value={filters.subsumed} onChange={(v) => patch({ subsumed: v as FoundryFilters["subsumed"] })} options={[...all, ["yes", tr("common.subsumed")], ["no", tr("filters.notSubsumed")]]} />
          <SortControl value={filters.sortBy} options={sortOptions} direction={filters.sortDir} tr={tr} onSelect={(k) => patch({ sortBy: k as SortKey, sortDir: defaultSortDir(k as SortKey) })} onToggleDirection={() => patch({ sortDir: filters.sortDir === "asc" ? "desc" : "asc" })} />
        </div>
        <Tabs label={tr("common.foundry")} items={tabs} active={tab} onSelect={setTab} />
      </div>

      {pinned.count > 0 && (
        <Card className="fd-pinned">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="ms-name">
              {tr("foundry.pinnedBlueprints")} <span className="text-accent">({pinned.count})</span>
            </span>
            <div className="flex items-center gap-3">
              {pinned.credits > 0 && <Readout currency="credits" value={pinned.credits} />}
              <Button size="sm" title={tr("foundry.unpinAll")} onClick={() => setPinList([])}>
                {tr("foundry.clear")}
              </Button>
            </div>
          </div>
          <div className="flex flex-wrap gap-x-4 gap-y-2">
            {pinned.resources.map((r) => (
              <div key={r.uniqueName} className="flex items-center gap-1.5 text-sm" title={name(r.uniqueName)}>
                <ItemImage src={image(r.uniqueName)} alt={name(r.uniqueName)} cls="h-7 w-7" tr={tr} />
                <span className={`tabular-nums ${r.owned >= r.needed ? "text-text-secondary" : "text-danger"}`}>
                  {formatNumber(r.owned)}/{formatNumber(r.needed)}
                </span>
              </div>
            ))}
          </div>
          {pinned.missing.length > 0 && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-t border-border-subtle pt-2">
              <span className="ms-eyebrow text-danger">{tr("common.missing")}</span>
              {pinned.missing.map((r) => (
                <span key={r.uniqueName} className="flex items-center gap-1 text-xs text-text-secondary" title={name(r.uniqueName)}>
                  <ItemImage src={image(r.uniqueName)} alt={name(r.uniqueName)} cls="h-5 w-5" tr={tr} />
                  {formatNumber(r.needed - r.owned)} {name(r.uniqueName)}
                </span>
              ))}
            </div>
          )}
        </Card>
      )}

      {loaded && !data ? (
        <Card data-foundry-empty>
          <NoInventoryState tr={tr} why="noInventory.foundry" />
        </Card>
      ) : visible.length === 0 ? (
        <Card data-foundry-empty>
          <EmptyState icon={<SearchX />} title={tr(rows.length === 0 ? "foundry.noItems" : "foundry.noItemsMatch")} />
        </Card>
      ) : (
        <VirtualCards rows={visible} render={card} gridClass="fd-grid" kind="foundry" />
      )}
    </div>
  );
}
