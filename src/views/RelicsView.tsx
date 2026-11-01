import { MonitorUp, SearchX, SlidersHorizontal } from "lucide-react";
import ImportNote from "../components/inventory/ImportNote";
import { useEffect, useMemo, useState } from "react";
import HeaderTabs from "../components/HeaderTabs";
import SortControl from "../components/SortControl";
import RelicCompactCard from "../components/relics/RelicCompactCard";
import { RelicIcon, type RelicTier } from "../components/icons";
import EditLayoutBar from "../components/layout/EditLayoutBar";
import LayoutGrid from "../components/layout/LayoutGrid";
import OpenInWindowButton from "../components/layout/OpenInWindowButton";
import { relicDataLabel } from "../components/settings/AboutCard";
import { Badge, Button, EmptyState, Field, IconButton, PageHeader, SearchInput, Select as UiSelect, ShowMore, Skeleton, useBatch } from "../components/ui";
import { ownedComponentCount } from "../../config/shared/componentNames";
import type { MessageKey, Translator } from "../lib/i18n";
import { foundryBuildProducts, parseFoundry } from "../lib/inventory/foundryResources";
import { getRelicDataInfo, getRelicDatabase, onRelicDbUpdated, type ItemDatabase, type RelicDataInfo } from "../lib/ipc";
import { registerSections } from "../lib/layout/registry";
import { buildMasteryLookup } from "../lib/masteryLookup";
import { pushRelicOverlayFilters } from "../lib/relic/overlayFilters";
import { relicPrice, useRelicPrices } from "../lib/relic/relicPrices";
import { isRewardNeeded, type RewardNeedContext } from "../lib/relic/rewardNeed";
import { useWfmCatalog } from "../stores/market";
import { ownershipFor } from "../stores/inventory";
import { useInventorySafety } from "../stores/inventorySafety";
import { addToast } from "../stores/toasts";
import { openRelicDetailed } from "../stores/modals";
import { isPopoutWindow } from "../stores/popout";
import type { ItemDbEntry, RawInventoryData } from "../types/inventory";
import { QUALITY_MODES, RELIC_QUALITY_LABEL_KEY } from "../lib/relic/relicConstants";
import { evDataFor } from "../lib/relic/relicEv";
import { parseOwnedRelics } from "../lib/relic/relicInventory";
import type { RelicDatabase, RelicGroup, RelicQuality, RelicReward } from "../lib/relic/types";
import { setReady } from "../stores/data";
import { RELIC_TABS, setRelicFilter, useRelicView, type RelicOwnershipMode, type RelicViewState } from "../stores/relics";
import {
  DEFAULT_RELIC_PLANNER_FILTERS,
  RELIC_OWNED_ABOVE_STEPS,
  relicOwnedCountForMode,
  relicQualityForMode,
  selectRelicPlannerRows,
  type RelicPlannerFilters,
  type RelicQualityMode,
  type RelicSortMode,
  type RelicVaultedMode,
} from "../../config/shared/relicPlannerView";
import { relicGroupMatchesSearch } from "../../config/shared/relicSearch";
import { sortRelicRewards } from "../../config/shared/relicRewardOrder";

registerSections("relics", [
  { id: "relics.filters", view: "relics", labelKey: "common.filters", defaultSpan: "full", minSpan: "full", canCollapse: true },
  { id: "relics.grid", view: "relics", labelKey: "common.relics", defaultSpan: "full", minSpan: "full", canHide: false },
]);

const PREVIEW_REWARD_LIMIT = 6;
const DESCENDING_DEFAULT_SORTS: ReadonlySet<RelicSortMode> = new Set(["ev", "ducat", "ducatonator", "owned"]);

interface Props {
  tr: Translator;
  locale?: string;
  items: ItemDatabase;
  /** Raw inventory (`MiscItems` etc.). Without it every relic is listed and counts read zero. */
  inventory?: Record<string, unknown> | null;
}

type Option<K> = ReadonlyArray<readonly [K, string]>;

function Select<K extends string | number>({ label, title, value, options, onChange }: { label: string; title: string; value: K; options: Option<K>; onChange: (v: string) => void }) {
  return (
    <Field label={label} className="relic-filter">
      <UiSelect title={title} value={value} options={options} onChange={(e) => onChange(e.target.value)} />
    </Field>
  );
}

// Without inventory every relic reads as zero copies, which would empty the list instead of filtering it.
// Every reward of the tier on screen, so the platinum EV and its sort have prices; the backend caches them.
const priceSlugs = (db: RelicDatabase | null, tier: string, keep: (g: RelicGroup) => boolean): string[] =>
  Object.values(db?.groups ?? {})
    .filter((g) => (tier === "all" || g.tier === tier) && keep(g))
    .flatMap((g) => QUALITY_MODES.flatMap((q) => (g.qualities[q]?.rewards ?? []).flatMap((r) => (r.urlName ? [r.urlName] : []))));

const plannerFilters = (view: RelicViewState, hasInventory: boolean): RelicPlannerFilters => {
  const { tierFilter: _tier, ownershipMode: _ownership, ...filters } = view;
  return hasInventory ? filters : { ...filters, ownedAbove: 0 };
};

export default function RelicsView({ tr, locale = "en", items, inventory = null }: Props) {
  const view = useRelicView();
  const [db, setDb] = useState<RelicDatabase | null>(null);
  const [state, setState] = useState<"loading" | "error" | "ready">("loading");
  // The refinement pinned on a card in "owned" mode.
  const [pinned, setPinned] = useState<Record<string, RelicQuality>>({});
  // Narrow windows fold the selects behind a Filters button (relics.css container query).
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [dataInfo, setDataInfo] = useState<RelicDataInfo | null>(null);
    const { catalog } = useWfmCatalog();
  const safety = useInventorySafety(inventory as RawInventoryData | null, items as unknown as Record<string, ItemDbEntry>);
  const mastery = safety.mastery;

  useEffect(() => {
    let live = true;
    const load = () =>
      getRelicDatabase().then(
        (next) => {
          if (!live) return;
          setDb(next);
          setState("ready");
          setReady("relics");
        },
        (e) => {
          console.error("[Relics] getRelicDatabase failed:", e);
          if (live) setState((s) => (s === "ready" ? s : "error"));
        },
      );
    void load();
    const loadInfo = () => void getRelicDataInfo().then(setDataInfo).catch(() => {});
    loadInfo();
    const off = onRelicDbUpdated(() => {
      void load();
      loadInfo();
    });
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, []);

  const hasInventory = inventory !== null;
  const ownedCounts = useMemo(() => parseOwnedRelics(inventory, db), [inventory, db]);

  const ownedQualityOf = (group: RelicGroup): RelicQuality | null => relicQualityForMode("owned", ownedCounts[group.key], pinned[group.key]);
  const itemDb = items as unknown as Record<string, ItemDbEntry>;
  const ownership = ownershipFor(inventory as RawInventoryData | null, items);
  const uniqueNameOf = (reward: RelicReward) => reward.uniqueName || (reward.urlName ? catalog?.bySlug(reward.urlName)?.gameRef : null) || null;

  // The needed-reward engine reads mastery, which only this filter and the overlay push need.
  const needOn = view.containsNeededReward;
  const neededKeys = useMemo(() => {
    if (!needOn || !db || !inventory) return null;
    const building = foundryBuildProducts(parseFoundry(inventory as RawInventoryData, itemDb));
    const ctx: RewardNeedContext = {
      safety: safety.context,
      building,
      mastery: buildMasteryLookup(mastery).byUniqueName,
      uniqueNameOf,
      ownedByName: (reward) => {
        const unique = uniqueNameOf(reward);
        return !!unique && ownedComponentCount(unique, ownership) > 0;
      },
    };
    const keys = new Set<string>();
    for (const g of Object.values(db.groups)) {
      if (QUALITY_MODES.some((q) => (g.qualities[q]?.rewards ?? []).some((r) => isRewardNeeded(r, ctx)))) keys.add(g.key);
    }
    return keys;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [needOn, db, inventory, itemDb, mastery, ownership, safety.context, catalog]);
  const ownedOnly = hasInventory && view.ownershipMode === "owned";
  const slugs = useMemo(() => priceSlugs(db, view.tierFilter, (g) => !ownedOnly || Object.values(ownedCounts[g.key] ?? {}).some((n) => n > 0)), [db, view.tierFilter, ownedOnly, ownedCounts]);
  const priceRev = useRelicPrices(slugs);
  const evFor = (group: RelicGroup) => evDataFor(group, view.squadSize, view.qualityMode, ownedQualityOf(group), relicPrice);
  const ownedCount = (group: RelicGroup, quality: RelicQuality) => ownedCounts[group.key]?.[quality] ?? 0;

  const qualityLabels = useMemo(() => Object.fromEntries(QUALITY_MODES.map((q) => [q, tr(RELIC_QUALITY_LABEL_KEY[q])])) as Record<RelicQuality, string>, [tr]);

  const groups = useMemo(() => {
    if (!db) return [];
    let list = Object.values(db.groups);
    if (hasInventory && view.ownershipMode === "owned") {
      list = list.filter((g) => Object.values(ownedCounts[g.key] ?? {}).some((n) => n > 0));
    }
    if (view.tierFilter !== "all") list = list.filter((g) => g.tier === view.tierFilter);
    const filters = plannerFilters(view, hasInventory);
    const rows = list.map((group) => {
      const ev = evDataFor(group, view.squadSize, view.qualityMode, relicQualityForMode("owned", ownedCounts[group.key], pinned[group.key]), relicPrice);
      return {
        group,
        name: group.name,
        tier: group.tier,
        vaulted: group.vaulted,
        ownedCount: relicOwnedCountForMode(ownedCounts[group.key], view.qualityMode),
        ownedTotal: relicOwnedCountForMode(ownedCounts[group.key], "owned"),
        plat: ev.plat,
        ducat: ev.ducat,
        ratio: ev.ratio,
      };
    });
    return selectRelicPlannerRows(rows, filters, {
      matchesSearch: (row) => relicGroupMatchesSearch(row.group, filters.search, { qualityLabels, ownedCounts: hasInventory ? (ownedCounts[row.group.key] ?? null) : undefined }),
      hasNeededReward: (row) => neededKeys?.has(row.group.key) ?? true,
    }).map((row) => row.group);
  }, [db, view, hasInventory, ownedCounts, pinned, qualityLabels, neededKeys, priceRev]);

  const { limit, more } = useBatch(JSON.stringify(view));
  const shownGroups = groups.slice(0, limit);

  const previewRewards = (group: RelicGroup): RelicReward[] => {
    for (const quality of ["intact", ...QUALITY_MODES]) {
      const rewards = group.qualities[quality]?.rewards ?? [];
      if (rewards.length > 0) return sortRelicRewards(rewards).slice(-PREVIEW_REWARD_LIMIT);
    }
    return [];
  };
  const isOwnedReward = (reward: RelicReward) => {
    const unique = uniqueNameOf(reward);
    return !!unique && ownedComponentCount(unique, ownership) > 0;
  };
  const rewardIconSrc = (reward: RelicReward) => items[uniqueNameOf(reward) ?? ""]?.imageUrl || reward.imageUrl || null;
  const pushToOverlay = () => {
    const ok = pushRelicOverlayFilters({
      ...plannerFilters(view, hasInventory),
      tierFilter: view.tierFilter === "all" ? null : view.tierFilter,
      neededRewardKeys: neededKeys ? [...neededKeys] : null,
      pinnedQualities: pinned,
    });
    addToast({ level: ok ? "success" : "error", message: tr(ok ? "relics.pushOverlayDone" : "relics.pushOverlayFailed") });
  };
  const dataLabel = relicDataLabel(tr, locale, dataInfo);
  const healthy = !!dataInfo && !dataInfo.updateOffline && !dataInfo.updateError;
  const dataDetail = dataInfo && [dataLabel, tr(dataInfo.source === "downloaded" ? "relics.dataSource.downloaded" : "relics.dataSource.bundled")].join(", ");
  const dataTone = dataInfo?.updateError || dataInfo?.updateOffline ? "warning" : "neutral";
  const rewardTooltip = (reward: RelicReward) => tr("relics.rewardTooltip", { name: reward.name, rarity: reward.rarity || tr("common.unknown"), chance: reward.chance ?? 0 });

  const opts = <K extends string>(pairs: ReadonlyArray<readonly [K, MessageKey]>): Option<K> => pairs.map(([k, key]) => [k, tr(key)] as const);
  const allGroups = Object.values(db?.groups ?? {});
  const tierTabs = [
    { key: "all", label: tr("common.all"), count: db ? allGroups.length : undefined },
    ...RELIC_TABS.slice(1).map((t) => ({
      key: t,
      label: tr(`relics.tier.${t.toLowerCase()}` as MessageKey),
      icon: <RelicIcon tier={t.toLowerCase() as RelicTier} />,
      count: db ? allGroups.filter((g) => g.tier === t).length : undefined,
    })),
  ];
  // Filters that differ from their defaults, shown on the Filters button when folded.
  const d = DEFAULT_RELIC_PLANNER_FILTERS;
  const activeFilters = [view.ownershipMode !== "owned", view.qualityMode !== d.qualityMode && view.qualityMode !== "owned", view.ownedAbove !== d.ownedAbove, view.vaultedMode !== d.vaultedMode, view.squadSize !== d.squadSize].filter(Boolean).length;
  const sortOptions = opts<RelicSortMode>([
    ["tier", "common.default"],
    ["name", "common.name"],
    ["ev", "common.platinum"],
    ["ducat", "common.ducats"],
    ["ducatonator", "relics.sort.ducatsPerPlat"],
    ["owned", "common.owned"],
  ]);
  const qualityOptions = opts<RelicQualityMode>([
    ["owned", "common.owned"],
    ["intact", "relics.quality.intact"],
    ["exceptional", "relics.quality.exceptional"],
    ["flawless", "relics.quality.flawless"],
    ["radiant", "relics.quality.radiant"],
  ]);
  const ownedAboveOptions: Option<number> = [[0, tr("relics.minOwned.any")], ...RELIC_OWNED_ABOVE_STEPS.map((count) => [count, tr("relics.minOwned.moreThan", { count })] as const)];

  const filterSection = (
    <div className="bg-bg-base" data-tour="relic-filters">
      <HeaderTabs label={tr("common.relics")} options={tierTabs} activeKey={view.tierFilter} onSelect={(tierFilter) => setRelicFilter({ tierFilter })} />
      <div className={`relic-toolbar ${filtersOpen ? "is-open" : ""}`}>
        <SearchInput label={tr("relics.searchPlaceholder")} value={view.search} onChange={(e) => setRelicFilter({ search: e.target.value })} placeholder={tr("relics.searchPlaceholder")} className="relic-search" />
        <Button size="sm" className="relic-filter-toggle" aria-expanded={filtersOpen} onClick={() => setFiltersOpen(!filtersOpen)} icon={<SlidersHorizontal />}>
          {tr("common.filters")}
          {activeFilters > 0 && <span className="ui-badge ui-badge--accent">{activeFilters}</span>}
        </Button>
        <div className="relic-filters">
          <Select
            label={tr("common.relics")}
            title={tr("relics.ownershipTitle")}
            value={view.ownershipMode}
            options={opts<RelicOwnershipMode>([["owned", "relics.ownership.ownedOnly"], ["all", "relics.ownership.all"]])}
            onChange={(v) => setRelicFilter({ ownershipMode: v as RelicOwnershipMode, qualityMode: v === "owned" ? "owned" : "intact" })}
          />
          <Select
            label={tr("relics.neededRewardLabel")}
            title={tr("relics.neededRewardTitle")}
            value={view.containsNeededReward ? "needed" : "any"}
            options={opts<"any" | "needed">([["any", "relics.minOwned.any"], ["needed", "relics.neededReward.needed"]])}
            onChange={(v) => setRelicFilter({ containsNeededReward: v === "needed" })}
          />
          <Select label={tr("relics.qualityLabel")} title={tr("relics.qualityTitle")} value={view.qualityMode} options={qualityOptions} onChange={(v) => setRelicFilter({ qualityMode: v as RelicQualityMode })} />
          <Select label={tr("relics.minOwnedLabel")} title={tr("relics.minOwnedTitle")} value={view.ownedAbove} options={ownedAboveOptions} onChange={(v) => Number.isFinite(Number(v)) && setRelicFilter({ ownedAbove: Number(v) })} />
          <Select
            label={tr("relics.vaultedLabel")}
            title={tr("relics.vaultedTitle")}
            value={view.vaultedMode}
            options={opts<RelicVaultedMode>([["all", "common.all"], ["vaulted", "common.vaulted"], ["unvaulted", "common.unvaulted"]])}
            onChange={(v) => setRelicFilter({ vaultedMode: v as RelicVaultedMode })}
          />
          <Select
            label={tr("relics.squadLabel")}
            title={tr("relics.squadTitle")}
            value={view.squadSize}
            options={[[1, tr("relics.squad.solo")], [2, tr("relics.squad.p2")], [3, tr("relics.squad.p3")], [4, tr("relics.squad.p4")]]}
            onChange={(v) => Number.isFinite(Number(v)) && setRelicFilter({ squadSize: Number(v) })}
          />
          <div className="relic-filter">
            <SortControl
              value={view.sortMode}
              options={sortOptions}
              direction={view.sortDirection}
              onSelect={(v) => setRelicFilter({ sortMode: v as RelicSortMode, sortDirection: DESCENDING_DEFAULT_SORTS.has(v as RelicSortMode) ? "desc" : "asc" })}
              onToggleDirection={() => setRelicFilter({ sortDirection: view.sortDirection === "asc" ? "desc" : "asc" })}
              tr={tr}
            />
          </div>
        </div>
      </div>
    </div>
  );

  const gridSection = (
    <>
      {state === "loading" ? (
        <div className="relic-grid" aria-busy="true" aria-label={tr("relics.loading")}>
          {[0, 1, 2, 3, 4, 5].map((i) => (
            <div key={i} className="ui-card flex flex-col gap-3 p-4">
              <Skeleton width="45%" height="1.1rem" />
              <Skeleton width="70%" />
              <Skeleton />
            </div>
          ))}
        </div>
      ) : state === "error" ? (
        <div className="ui-card">
          <EmptyState title={tr("relics.loadFailed")} />
        </div>
      ) : groups.length === 0 ? (
        <div className="ui-card">
          <EmptyState icon={<SearchX />} title={tr("relics.empty")} />
        </div>
      ) : (
        <>
          <div className="relic-grid">
          {shownGroups.map((group) => (
            <RelicCompactCard
              key={group.key}
              group={group}
              qualityMode={view.qualityMode}
              selectedOwned={ownedQualityOf(group)}
              selected={evFor(group)}
              rewardIcons={previewRewards(group)}
              ownedCount={ownedCount}
              isOwnedReward={isOwnedReward}
              rewardIconSrc={rewardIconSrc}
              rewardTooltip={rewardTooltip}
              setOwnedQuality={(g, q) => ownedCount(g, q) > 0 && setPinned((p) => ({ ...p, [g.key]: q }))}
              openRelic={openRelicDetailed}
              tr={tr}
            />
          ))}
          </div>
          <ShowMore shown={shownGroups.length} total={groups.length} onMore={more} tr={tr} />
        </>
      )}
    </>
  );

  return (
    <div className="relics-view">
      <PageHeader
        title={tr("relics.title", { count: groups.length })}
        sub={
          dataLabel && (
            <Badge tone={dataTone} title={dataInfo?.updateError ?? (healthy ? dataDetail : null) ?? undefined} data-relic-data-status>
              {/* healthy: a plain status, the version is the tooltip; offline/failed keep the explicit label */}
              {healthy ? tr("relics.dataUpToDate") : `${tr("relics.dataTables")}: ${dataLabel}`}
            </Badge>
          )
        }
        actions={
          <>
            <IconButton label={tr("relics.pushOverlayTitle")} onClick={pushToOverlay} data-relic-push-overlay>
              <MonitorUp />
            </IconButton>
            {!isPopoutWindow && <OpenInWindowButton target={{ kind: "view", view: "relics" }} tr={tr} data-layout-popout="relics" className="ui-icon-btn" />}
            <EditLayoutBar view="relics" tr={tr} />
          </>
        }
      />
      <ImportNote tr={tr} locale={locale} />
      <LayoutGrid view="relics" tr={tr} gapClass="gap-y-4" columnGapClass="gap-4" sticky={["relics.filters"]}>
        {(id) => (id === "relics.filters" ? filterSection : id === "relics.grid" ? gridSection : null)}
      </LayoutGrid>
    </div>
  );
}
