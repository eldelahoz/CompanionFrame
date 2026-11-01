import { ownedComponentCount } from "../../../config/shared/componentNames.js";
import { mergeDuplicateIngredients } from "../../../config/shared/recipeRows.js";
import type { ComponentInfo, ItemDbEntry, ParsedItem, PartType } from "../../types/inventory.js";
import {
  type ResolvedItem,
  resolveItem,
  isAyatanLikeItem,
  isSceneLikeItem,
  isRelicLikeItem,
} from "./itemClassification.js";
import { FULL_SET_OVERRIDES, getFullSetOverride } from "./fullSetOverrides.js";
import { isCraftingResource } from "./partConsumers.js";

// Set rows are synthesized, so they key off the root uniqueName plus a marker
// that cannot collide with a real DE path.
const FULL_SET_SUFFIX = "#set";

/** Undoes the suffix minted below; a non-set internalName passes through. */
export function setRootOf(internalName: string): string {
  return internalName.endsWith(FULL_SET_SUFFIX)
    ? internalName.slice(0, -FULL_SET_SUFFIX.length)
    : internalName;
}

function isGenericSetComponent(
  component: ComponentInfo,
  itemDb: Record<string, ItemDbEntry>,
  isPrimeRoot: boolean,
): boolean {
  const uniqueName = component.uniqueName || "";
  if (!uniqueName || isCraftingResource(uniqueName)) return false;

  const entry = itemDb[uniqueName];
  if (entry?.isBuildComponent === false || entry?.masterable === true) return false;
  if (entry?.isBuildComponent !== true && !/\/Types\/Recipes\//i.test(uniqueName)) {
    return false;
  }

  if (component.tradable === true || entry?.tradable === true || isPrimeRoot) return true;
  return /\/Types\/Recipes\/Weapons\/WeaponParts?\//i.test(uniqueName);
}

function overrideComponents(
  rootUniqueName: string,
  itemDb: Record<string, ItemDbEntry>,
): ComponentInfo[] | null {
  const override = getFullSetOverride(rootUniqueName);
  if (!override) return null;
  return override.components.map((component) => ({
    name: component.name,
    uniqueName: component.uniqueName,
    itemCount: component.itemCount || 1,
    tradable: true,
    drops: itemDb[component.uniqueName]?.drops || [],
  }));
}

function isEligibleFullSetRoot(
  uniqueName: string,
  dbEntry: ItemDbEntry,
  resolved: ResolvedItem,
  tradableComponentCount: number,
): boolean {
  if (tradableComponentCount < 2) return false;
  if (isAyatanLikeItem(uniqueName, dbEntry, resolved)) return false;
  if (isSceneLikeItem(uniqueName, dbEntry, resolved)) return false;
  if (isRelicLikeItem(uniqueName, dbEntry, resolved)) return false;

  const type = String(dbEntry.type || "").toLowerCase();
  const name = String(resolved.name || "").toLowerCase();

  if (
    type.includes("captura") ||
    type.includes("ayatan") ||
    type.includes("resource") ||
    type.includes("booster")
  ) {
    return false;
  }

  if (
    name.includes("ayatan") ||
    name.endsWith(" scene") ||
    name.includes("booster") ||
    name.includes("quest")
  ) {
    return false;
  }

  if (resolved.isPrime === true || /\bprime\b/i.test(resolved.name)) return true;

  const category = String(dbEntry.category || "").toLowerCase();

  if (
    /(warframe|rifle|shotgun|sniper|bow|pistol|melee|companion|sentinel|archwing|necramech|orbiter|landing craft)/.test(
      type,
    )
  ) {
    return true;
  }

  return /(warframe|weapon|primary|secondary|melee|sentinel|pet|companion|archwing|necramech)/.test(
    category,
  );
}

export function buildFullSetItems(
  itemDb: Record<string, ItemDbEntry>,
  ownedCounts: Map<string, number>,
  sellableEquipmentCounts?: Map<string, number>,
): ParsedItem[] {
  const setItems: ParsedItem[] = [];
  const roots = Object.entries(itemDb);
  for (const override of FULL_SET_OVERRIDES) {
    if (!itemDb[override.rootUniqueName] && override.rootName) {
      roots.push([
        override.rootUniqueName,
        { name: override.rootName, category: "Misc", type: "Set" },
      ]);
    }
  }

  for (const [uniqueName, dbEntry] of roots) {
    const explicitComponents = overrideComponents(uniqueName, itemDb);
    const components =
      explicitComponents || (Array.isArray(dbEntry.components) ? dbEntry.components : []);
    // Do not gate on root tradability: assembled Warframes are untradable even
    // when their parts and full set are tradable.
    if (components.length === 0) continue;

    const resolved = itemDb[uniqueName]
      ? resolveItem(uniqueName, itemDb)
      : { ...dbEntry, name: dbEntry.name || "Unknown", imageUrl: dbEntry.imageUrl ?? null };

    const isPrimeRoot = resolved.isPrime === true || /\bPrime\b/.test(resolved.name);
    const setComponents = mergeDuplicateIngredients(
      explicitComponents
        ? components
        : components.filter((component) => isGenericSetComponent(component, itemDb, isPrimeRoot)),
      (component) => component.itemCount,
      (component, itemCount) => ({ ...component, itemCount }),
    );
    if (setComponents.length === 0) continue;

    if (
      !explicitComponents &&
      !isEligibleFullSetRoot(uniqueName, dbEntry, resolved, setComponents.length)
    ) {
      continue;
    }

    let completeSets = Number.POSITIVE_INFINITY;

    const hydratedComponents = setComponents.map((component) => {
      const unique = component.uniqueName || "";
      const required =
        typeof component.itemCount === "number" && component.itemCount > 0
          ? component.itemCount
          : 1;
      const componentEntry = itemDb[unique];
      const ownership =
        componentEntry?.masterable === true && sellableEquipmentCounts
          ? sellableEquipmentCounts
          : ownedCounts;
      const ownedCount = ownedComponentCount(unique, ownership);
      completeSets = Math.min(completeSets, Math.floor(ownedCount / required));

      return {
        ...component,
        ownedCount,
        owned: ownedCount >= required,
      };
    });

    if (!Number.isFinite(completeSets)) completeSets = 0;

    const totalPartTypes = hydratedComponents.length;
    const ownedPartTypes = hydratedComponents.filter((component) => component.owned).length;
    const missingParts = totalPartTypes - ownedPartTypes;

    const withSet = (base: string) => (base.endsWith(" Set") ? base : `${base} Set`);
    const setName = withSet(resolved.name);
    // "Set" is our own word, not one DE ships, so only the item name follows the
    // game language here.
    const setDisplayName = resolved.displayName ? withSet(resolved.displayName) : null;
    const isPrime = isPrimeRoot;
    const marketSlug = getFullSetOverride(uniqueName)?.slug;

    const common = {
      name: setName,
      ...(setDisplayName ? { displayName: setDisplayName } : {}),
      internalName: `${uniqueName}${FULL_SET_SUFFIX}`,
      rank: 0,
      maxRank: 1,
      imageUrl: resolved.imageUrl ?? null,
      fallbackImageUrl: resolved.fallbackImageUrl ?? null,
      isPrime,
      masteryReq: resolved.masteryReq ?? 0,
      vaulted: resolved.vaulted ?? false,
      tradable: true,
      description: typeof dbEntry.description === "string" ? dbEntry.description : "",
      components: hydratedComponents,
      drops: Array.isArray(dbEntry.drops) ? dbEntry.drops : [],
      wikiaUrl: typeof dbEntry.wikiaUrl === "string" ? dbEntry.wikiaUrl : null,
      partType: (isPrime ? "prime" : "normal") as PartType,
      leveledUp: false,
      ...(marketSlug ? { marketSlug } : {}),
    };

    if (completeSets >= 1) {
      // A full set's worth of spare components - the user can sell it.
      setItems.push({
        ...common,
        category: "full_sets",
        categoryLabel: "Full Set",
        amount: completeSets,
        completeSets,
        inventoryGroup: "full_sets",
        keywords: ["set", "full set", resolved.name.toLowerCase()],
      });
    } else if (ownedPartTypes > 0) {
      // In-progress: at least one part owned, at least one still missing.
      setItems.push({
        ...common,
        category: "incomplete_sets",
        categoryLabel: "Incomplete Set",
        amount: 0,
        completeSets: 0,
        missingParts,
        ownedPartTypes,
        totalPartTypes,
        inventoryGroup: "incomplete_sets",
        keywords: ["set", "incomplete set", "missing", resolved.name.toLowerCase()],
      });
    }
    // completeSets < 1 with zero owned parts = not started; left out of the tab.
  }

  return setItems;
}
