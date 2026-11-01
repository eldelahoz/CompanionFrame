// Joins the backend's mastery rows with foundry state, part ownership and market prices into what the roadmap ranks.
import { componentUniqueNameAliases } from "../../config/shared/componentNames";
import type { CatalogIndex } from "./wfm/catalog";
import type { MasteryProgress } from "./ipc";
import type { ComponentInfo, FoundryBuildingItem, ItemDbEntry } from "../types/inventory";
import type { FoundryState } from "./foundryView";
import type { WfmItemsLookup } from "./componentResolution";
import { buildPartState } from "./craftingTree";
import { componentMarketSlug, estimateMasteryPurchaseCost, masteryBuildReadiness, type MasteryRoadmapSourceItem } from "./masteryRoadmap";

type Item = MasteryProgress["items"][number];
type FoundryStatus = "in-progress" | "claimable";

export type PriceMap = ReadonlyMap<string, number>;

/** Catalog as the lowercase name / gameRef lookup the roadmap's slug resolution expects. */
export function catalogLookup(catalog: CatalogIndex | null): WfmItemsLookup {
  const lookup: WfmItemsLookup = {};
  for (const i of catalog?.items ?? []) {
    lookup[i.name.toLowerCase()] = { url_name: i.slug };
    if (i.gameRef) lookup[i.gameRef.toLowerCase()] = { url_name: i.slug };
    // Warframes and weapons trade as "<name> Set"; the mastery row is the bare name.
    const bare = i.name.toLowerCase().replace(/ set$/, "");
    if (bare !== i.name.toLowerCase() && !lookup[bare]) lookup[bare] = { url_name: i.slug };
  }
  return lookup;
}

function foundryIndex(building: readonly FoundryBuildingItem[], now: number) {
  const byUnique = new Map<string, FoundryStatus>();
  const byName = new Map<string, FoundryStatus>();
  for (const b of building) {
    const status: FoundryStatus = b.endDate && b.endDate.getTime() <= now ? "claimable" : "in-progress";
    if (b.productUniqueName && byUnique.get(b.productUniqueName) !== "claimable") byUnique.set(b.productUniqueName, status);
    const key = b.name.trim().toLowerCase();
    if (key && byName.get(key) !== "claimable") byName.set(key, status);
  }
  return { byUnique, byName };
}

/** The market slugs worth pricing: the item itself and every part it still lacks. */
export function roadmapPriceSlugs(items: readonly Item[], lookup: WfmItemsLookup): string[] {
  const slugs = new Set<string>();
  for (const item of items) {
    if (item.status === "mastered" || (item.masteryXpRemaining ?? 0) <= 0 || item.currentlyOwned) continue;
    const root = lookup[item.name.toLowerCase()]?.url_name;
    if (root) slugs.add(root);
    for (const c of item.components) {
      if (c.owned) continue;
      const slug = componentMarketSlug(item.name, c, lookup);
      if (slug) slugs.add(slug);
    }
  }
  return [...slugs];
}

export function hydrateMasteryItems(
  items: readonly Item[],
  ctx: { building: readonly FoundryBuildingItem[]; ownership: ReadonlyMap<string, number>; db: Record<string, ItemDbEntry>; lookup: WfmItemsLookup; prices: PriceMap; now: number },
): MasteryRoadmapSourceItem[] {
  const foundry = foundryIndex(ctx.building, ctx.now);
  return items.map((item) => {
    const foundryStatus = (item.uniqueName ? foundry.byUnique.get(item.uniqueName) : undefined) ?? foundry.byName.get(item.name.trim().toLowerCase());
    // Bindings type chance as number | null where the roadmap reads number | undefined; the roadmap never touches drops.
    const components = item.components.map((c) => ({
      ...c,
      // Sets name a part ...Component while the foundry builds ...Blueprint.
      building: c.uniqueName ? componentUniqueNameAliases(c.uniqueName).some((u) => foundry.byUnique.has(u)) : false,
      blueprintHeld: c.uniqueName ? buildPartState({ uniqueName: c.uniqueName, count: c.itemCount || 1 }, ctx.ownership, ctx.db, item.uniqueName) === "blueprint" : false,
    })) as unknown as ComponentInfo[];
    const owned = item.currentlyOwned === true;
    const buildable = !owned && masteryBuildReadiness(components) === "buildable";
    const priceOf = (slug: string | null | undefined) => (slug ? (ctx.prices.get(slug) ?? null) : null);
    const rootPrice = priceOf(ctx.lookup[item.name.toLowerCase()]?.url_name);
    const estimatedCost = estimateMasteryPurchaseCost(rootPrice, components, (c) => priceOf(componentMarketSlug(item.name, c, ctx.lookup)));
    const foundryState: FoundryState | undefined = foundryStatus === "claimable" ? "claimable" : foundryStatus === "in-progress" ? "building" : buildable ? "buildable" : undefined;
    return {
      ...item,
      internalName: item.uniqueName,
      masteryXpRemaining: item.masteryXpRemaining ?? 0,
      components,
      owned,
      platinum: rootPrice,
      estimatedCost,
      foundryState,
    } as unknown as MasteryRoadmapSourceItem;
  });
}
