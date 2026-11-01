import { Lock, Pin, PinOff } from "lucide-react";
import { useMemo, useState } from "react";
import { itemLabel } from "../../lib/itemLabel";
import type { MessageKey, Translator } from "../../lib/i18n";
import { easyMasteryPotentialRank } from "../../lib/masteryProjection";
import { componentPartState, masteryCraftableCount, masteryPartCounts, type MasteryRoadmap as Roadmap, type MasteryRoadmapRecommendation as Rec } from "../../lib/masteryRoadmap";
import { readStorage, writeStorage } from "../../lib/persistence";
import type { ItemDbEntry } from "../../types/inventory";
import ItemImage from "../ItemImage";
import { Badge, EmptyState, Field, IconButton, Readout, SearchInput, Select, StatStrip, Tabs } from "../ui";

type Mode = "easy" | "relics" | "platinum";
type Sort = "recommended" | "xp" | "price";

const MODE_KEY = "wf_mastery_roadmap_tab";
const MODES: Mode[] = ["easy", "relics", "platinum"];
const MODE_LABEL: Record<Mode, MessageKey> = { easy: "mastery.roadmap.easy", relics: "mastery.roadmap.fromRelics", platinum: "mastery.roadmap.withPlatinum" };
const ACCESS_LABEL: Record<Rec["access"], MessageKey> = {
  owned: "mastery.roadmap.accessOwned",
  gild: "mastery.roadmap.accessGild",
  claimable: "mastery.roadmap.accessClaimable",
  building: "mastery.roadmap.accessBuilding",
  buildable: "common.canBuild",
  foundryParts: "mastery.roadmap.accessFoundryParts",
  craftParts: "mastery.roadmap.accessCraftParts",
  marketBlueprint: "mastery.roadmap.accessMarket",
  relics: "mastery.roadmap.accessRelics",
  platinum: "mastery.roadmap.accessPlatinum",
};

const percent = (p: number | null) => `${((p ?? 0) * 100).toFixed(2).replace(/\.00$/, "")}%`;
const savedMode = (): Mode => {
  const raw = readStorage(MODE_KEY) ?? "";
  return (MODES as string[]).includes(raw) ? (raw as Mode) : "easy";
};

interface Props {
  tr: Translator;
  locale: string;
  roadmap: Roadmap;
  db: Record<string, ItemDbEntry>;
  totalXp: number | null;
  currentRank: number | null;
  pins: readonly string[];
  /** Platinum estimates are still loading, so an empty platinum list is not final. */
  pricing: boolean;
  onOpen: (uniqueName: string) => void;
  onTogglePin: (uniqueName: string) => void;
}

export default function MasteryRoadmap({ tr, locale, roadmap, db, totalXp, currentRank, pins, pricing, onOpen, onTogglePin }: Props) {
  const [mode, setModeState] = useState<Mode>(savedMode);
  const [sort, setSort] = useState<Sort>("recommended");
  const [category, setCategory] = useState("all");
  const [search, setSearch] = useState("");
  const setMode = (m: Mode) => (setModeState(m), writeStorage(MODE_KEY, m), setSort("recommended"));
  const num = (n: number) => n.toLocaleString(locale);

  const source = roadmap[mode];
  const categories = useMemo(() => [...new Set([...roadmap.easy, ...roadmap.relics, ...roadmap.platinum].map((i) => i.category))].sort((a, b) => a.localeCompare(b)), [roadmap]);
  const visible = useMemo(() => {
    const q = search.trim().toLowerCase();
    // Match both: the list shows the localized name but traders search in English.
    const rows = source.filter((i) => (category === "all" || i.category === category) && (!q || i.name.toLowerCase().includes(q) || itemLabel(i.uniqueName ?? "", i).toLowerCase().includes(q)));
    if (sort === "xp") return [...rows].sort((a, b) => b.masteryXpRemaining - a.masteryXpRemaining || a.name.localeCompare(b.name));
    if (sort === "price") return [...rows].sort((a, b) => (a.estimatedCost ?? Infinity) - (b.estimatedCost ?? Infinity) || a.name.localeCompare(b.name));
    return rows;
  }, [source, category, search, sort]);

  const sum = (rows: Rec[]) => rows.reduce((s, i) => s + i.masteryXpRemaining, 0);
  const easyXp = sum(roadmap.easy);
  const potential = easyMasteryPotentialRank(currentRank, totalXp, easyXp);
  const best = roadmap.platinum[0] ?? null;
  const xp = (n: number) => (
    <Readout currency="mastery" value={n} locale={locale}>
      <span>{tr("mastery.roadmap.xpAmount", { amount: num(n) })}</span>
    </Readout>
  );

  return (
    <div className="grid gap-3" data-tour="mastery-roadmap">
      <StatStrip
        className="ms-stats"
        items={[
          { key: "easy", label: tr("mastery.roadmap.easyMastery"), value: xp(easyXp), tone: "success", hint: potential != null ? tr("mastery.roadmap.easyDescWithRank", { count: roadmap.easy.length, rank: potential }) : tr("mastery.roadmap.easyDesc", { count: roadmap.easy.length }) },
          { key: "relics", label: tr("mastery.roadmap.accessRelics"), value: xp(sum(roadmap.relics)), hint: tr("mastery.roadmap.relicsDesc", { count: roadmap.relics.length }) },
          { key: "buyable", label: tr("mastery.roadmap.buyableMastery"), value: xp(sum(roadmap.platinum)), hint: tr("mastery.roadmap.buyableDesc", { count: roadmap.platinum.length }) },
          {
            key: "best",
            label: tr("mastery.roadmap.bestValue"),
            value: best ? <span className="truncate">{itemLabel(best.uniqueName ?? "", best)}</span> : <span className="text-lg text-text-muted">{tr("mastery.roadmap.noPricedItems")}</span>,
            // Gated on a real price: "at 0p" would read as a genuine quote.
            hint: best?.estimatedCost != null ? tr("mastery.roadmap.xpPerPlatAt", { xp: num(Math.round(best.xpPerPlatinum ?? 0)), cost: best.estimatedCost }) : undefined,
          },
        ]}
      />

      <div className="ms-toolbar">
        <Tabs label={tr("mastery.viewRoadmap")} items={MODES.map((m) => ({ key: m, label: tr(MODE_LABEL[m]), count: roadmap[m].length }))} active={mode} onSelect={setMode} />
        <div className="ms-filters">
          <SearchInput label={tr("common.searchPlaceholder")} placeholder={tr("common.searchPlaceholder")} value={search} onChange={(e) => setSearch(e.target.value)} />
          <Field label={tr("common.category")}>
            <Select value={category} options={[["all", tr("common.all")], ...categories.map((c) => [c, c] as const)]} onChange={(e) => setCategory(e.target.value)} />
          </Field>
          <Field label={tr("common.sort")}>
            <Select
              value={sort}
              options={[["recommended", tr("common.recommended")], ["xp", tr("mastery.roadmap.sortMostXp")], ...(mode === "platinum" ? [["price", tr("mastery.roadmap.sortLowestPrice")] as const] : [])]}
              onChange={(e) => setSort(e.target.value as Sort)}
            />
          </Field>
        </div>
      </div>

      {visible.length === 0 ? (
        <div className="ui-card">
          <EmptyState title={tr(mode === "platinum" && pricing ? "common.loading" : "mastery.roadmap.noItemsMatch")} />
        </div>
      ) : (
        <div className="ms-list">
          {visible.map((item) => {
            const uniqueName = item.uniqueName ?? item.internalName;
            const label = itemLabel(uniqueName, item);
            const access = item.access === "marketBlueprint" && item.marketCredits != null ? tr("mastery.roadmap.accessMarketCredits", { credits: num(item.marketCredits as number) }) : tr(ACCESS_LABEL[item.access]);
            const parts = masteryPartCounts(item.components.map(componentPartState));
            const craftable = masteryCraftableCount(item.components, componentPartState, db);
            const locked = currentRank !== null && item.masteryReq > currentRank;
            const pinned = pins.includes(uniqueName);
            const partsLine = craftable > 0 ? tr("mastery.roadmap.partsOwnedCraftableShort", { owned: parts.built, craftable, total: parts.total }) : tr("mastery.roadmap.partsOwnedShort", { owned: parts.built, total: parts.total });
            return (
              <div key={`${uniqueName}-${item.access}`} className="ms-row" data-roadmap-row={uniqueName} data-access={item.access}>
                <button type="button" className="ms-row-main" onClick={() => onOpen(uniqueName)} title={tr("common.openDetailsFor", { name: label })}>
                  <span className="ms-thumb">
                    <ItemImage src={item.imageUrl} fallback={item.fallbackImageUrl} alt="" cls="max-h-full max-w-full object-contain" tr={tr} />
                  </span>
                  <span className="min-w-0">
                    <span className="ms-name">
                      <strong className="truncate">{label}</strong>
                      {item.masteryReq > 0 && (
                        <Badge tone={locked ? "warning" : "neutral"} icon={locked ? <Lock aria-hidden="true" /> : undefined} title={locked ? tr("mastery.roadmap.mrLockedTitle", { rank: item.masteryReq, current: currentRank ?? 0 }) : undefined}>
                          {tr("rivens.mr", { level: item.masteryReq })}
                        </Badge>
                      )}
                      {item.dojoResearch === true && <Badge tone="info" title={tr("mastery.roadmap.dojoResearchTitle")}>{tr("mastery.roadmap.dojoResearchTag")}</Badge>}
                    </span>
                    {item.access === "relics" ? (
                      <span className="ms-sub text-success" title={tr("mastery.roadmap.relicChanceTitle")}>
                        {tr(item.relevantRelicCount === 1 ? "mastery.roadmap.chanceWithOneRelic" : "mastery.roadmap.chanceWithRelics", { prob: percent(item.relicProbability), count: item.relevantRelicCount })}
                      </span>
                    ) : (
                      <span className="ms-sub">{tr("mastery.roadmap.categoryAccessLine", { category: item.category, access })}</span>
                    )}
                    <span className="ms-sub ms-sub--muted">
                      {item.access === "owned" ? tr("mastery.roadmap.levelLine", { rank: item.rank, maxRank: item.maxRank }) : item.components.length > 0 ? partsLine : tr("mastery.notOwned")}
                    </span>
                  </span>
                  <span className="ms-values">
                    {xp(item.masteryXpRemaining)}
                    {item.estimatedCost != null && item.access === "platinum" && (
                      <>
                        <Readout currency="platinum" value={item.estimatedCost} locale={locale} />
                        <span className="ms-sub ms-sub--muted">{tr("mastery.roadmap.xpPerPlat", { xp: num(Math.round(item.xpPerPlatinum ?? 0)) })}</span>
                      </>
                    )}
                  </span>
                </button>
                <IconButton label={tr(pinned ? "mastery.planner.unpin" : "mastery.planner.pin")} aria-pressed={pinned} data-mastery-pin={uniqueName} onClick={() => onTogglePin(uniqueName)}>
                  {pinned ? <PinOff /> : <Pin />}
                </IconButton>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
