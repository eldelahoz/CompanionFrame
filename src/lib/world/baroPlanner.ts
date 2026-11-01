import type { BaroHistory } from "../../../config/shared/baroHistory";
import { EQUIPMENT_COLLECTIONS } from "../../../config/shared/gearCollections";
import type { RawInventoryData } from "../../types/inventory";
import { activeWindow } from "../format";
import { itemLabel } from "../itemLabel";

/** The part of the world state's void trader the planner reads. */
export interface VaultTrader {
  activation?: string | null;
  expiry?: string | null;
  location?: string;
  inventory?: Array<{ uniqueName: string; item?: string; ducats?: number | null; credits?: number | null; imageOverride?: string | null }>;
}

export interface BaroCatalogRow {
  uniqueName: string;
  name: string;
  imageUrl: string | null;
  ducats: number | null;
  credits: number | null;
  available: boolean;
  lastSeen: number | null;
  owned: boolean;
  quantity: number;
}

const DUCATS_ITEM = "/Lotus/Types/Items/MiscItems/PrimeBucks";

function amount(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

function quantity(value: unknown): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 ? value : 0;
}

function validName(value: unknown): value is string {
  return typeof value === "string" && value.startsWith("/Lotus/") && value.length <= 512;
}

/** uniqueNames the player owns: gear, mods, relics, cosmetics, misc. */
export function buildBaroOwnedSet(inventory: RawInventoryData | null): Set<string> {
  const owned = new Set<string>();
  if (!inventory) return owned;
  for (const key of [...EQUIPMENT_COLLECTIONS, "RawUpgrades", "Upgrades", "LevelKeys", "MiscItems", "FlavourItems"] as Array<keyof RawInventoryData>) {
    const rows = inventory[key];
    if (!Array.isArray(rows)) continue;
    for (const row of rows as Array<{ ItemType?: unknown; ItemCount?: unknown; Count?: unknown }>) {
      if (!row || typeof row.ItemType !== "string" || !row.ItemType) continue;
      const count = "ItemCount" in row ? row.ItemCount : row.Count;
      if (count !== undefined && (typeof count !== "number" || !Number.isFinite(count) || count <= 0)) continue;
      owned.add(row.ItemType);
    }
  }
  return owned;
}

export function buildBaroCatalog(
  history: BaroHistory | null,
  current: VaultTrader | null | undefined,
  inventory: RawInventoryData | null,
  itemDb: Record<string, { name: string; displayName?: string | null; imageUrl?: string | null }>,
  wishlist: Record<string, number>,
  now: number,
): BaroCatalogRow[] {
  const owned = buildBaroOwnedSet(inventory);
  const past = new Map(history?.lastSeen.map((entry) => [entry.uniqueName, entry]));
  const offers = new Map<string, NonNullable<VaultTrader["inventory"]>[number]>();
  for (const offer of current?.inventory ?? []) {
    if (validName(offer?.uniqueName) && !offers.has(offer.uniqueName)) offers.set(offer.uniqueName, offer);
  }
  const active = activeWindow(current?.activation, current?.expiry, now);
  const activation = Date.parse(current?.activation ?? "");
  const observedAt = Number.isFinite(activation) && activation > 0 && activation <= now ? activation : null;
  const identities = new Set([...past.keys(), ...offers.keys(), ...Object.keys(wishlist)]);
  return [...identities].filter(validName).map((uniqueName) => {
    const previous = past.get(uniqueName);
    const offer = offers.get(uniqueName);
    const db = itemDb[uniqueName];
    const currentName = typeof offer?.item === "string" ? offer.item.trim() : "";
    const lastSeen = offer && observedAt !== null ? Math.max(previous?.lastSeen ?? 0, observedAt) : (previous?.lastSeen ?? null);
    return {
      uniqueName,
      name: (db && itemLabel(uniqueName, db)) || currentName || itemLabel(uniqueName),
      imageUrl: (typeof offer?.imageOverride === "string" && offer.imageOverride) || db?.imageUrl || null,
      // An omitted current price is unknown even when an older visit quoted one.
      ducats: amount(offer ? offer.ducats : previous?.ducats),
      credits: amount(offer ? offer.credits : previous?.credits),
      available: active && !!offer,
      lastSeen,
      owned: owned.has(uniqueName),
      quantity: quantity(wishlist[uniqueName]),
    };
  });
}

function balance(inventory: RawInventoryData | null, id: "ducats" | "credits"): number | null {
  if (!inventory) return null;
  if (id === "credits") return amount(inventory.RegularCredits);
  if (!Array.isArray(inventory.MiscItems)) return null;
  const row = inventory.MiscItems.find((entry) => entry?.ItemType === DUCATS_ITEM);
  // DE removes exhausted resource stacks from an otherwise complete MiscItems array.
  return row ? amount(row.ItemCount) : 0;
}

export function baroBudget(rows: BaroCatalogRow[], inventory: RawInventoryData | null) {
  const ducats = balance(inventory, "ducats");
  const credits = balance(inventory, "credits");
  let totalDucats = 0;
  let totalCredits = 0;
  let unknownCosts = 0;
  let unavailableItems = 0;
  let unknownDucats = false;
  let unknownCredits = false;
  for (const row of rows) {
    const count = quantity(row.quantity);
    if (!count) continue;
    if (!row.available) {
      unavailableItems += 1;
      continue;
    }
    const ducatCost = amount(row.ducats);
    const creditCost = amount(row.credits);
    if (ducatCost === null || creditCost === null) unknownCosts += 1;
    if (ducatCost === null) unknownDucats = true;
    else totalDucats += ducatCost * count;
    if (creditCost === null) unknownCredits = true;
    else totalCredits += creditCost * count;
  }
  return {
    ducats,
    credits,
    totalDucats,
    totalCredits,
    unknownCosts,
    unknownDucats,
    unknownCredits,
    unavailableItems,
    ducatShortfall: ducats === null || unknownDucats ? null : Math.max(0, totalDucats - ducats),
    creditShortfall: credits === null || unknownCredits ? null : Math.max(0, totalCredits - credits),
  };
}
