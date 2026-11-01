import {
  getInventory,
  getInventoryStatus,
  onInventoryStatusUpdated,
  onInventoryUpdated,
  type InventoryStatus,
  type ItemDatabase,
} from "../lib/ipc";
import { parseInventory } from "../lib/inventory";
import { parseResources } from "../lib/inventory/foundryResources";
import type { ItemDbEntry, ParsedItem, RawInventoryData, Resource } from "../types/inventory";
import { aggregateComponentOwnership } from "../../config/shared/componentOwnership";
import { withoutFoundryPending } from "../../config/shared/foundryPending";
import { buildSubsumedFamilySet } from "../lib/helminth";
import { chainBuildableBlueprints, parseFoundry } from "../lib/inventory/foundryResources";
import { setReady } from "./data";
import { createSharedStore, memoByIdentity } from "./shared";

export interface InventoryState {
  data: RawInventoryData | null;
  status: InventoryStatus | null;
  /** The first read finished, so "no inventory" is a fact rather than "not loaded yet". */
  loaded: boolean;
}

const store = createSharedStore<InventoryState>({ data: null, status: null, loaded: false }, (patch) => {
  let live = true;
  void Promise.all([getInventory(), getInventoryStatus()]).then(([data, status]) => {
    if (!live) return;
    patch({ data, status, loaded: true });
    setReady("inventory");
  });
  const offs = [
    onInventoryUpdated((data) => {
      if (!live) return;
      patch({ data });
      void getInventoryStatus().then((status) => live && patch({ status }));
    }),
    onInventoryStatusUpdated((status) => live && patch({ status })),
  ];
  return () => {
    live = false;
    offs.forEach((off) => void off.then((f) => f()));
  };
});

/** The shared inventory: fetched once, followed through `inventory-updated` / `inventory-status-updated`. */
export const useInventory = store.use;
export const resetInventoryStore = store.reset;

// The item DB comes from specta with a stricter shape than the inventory code reads; it only reads fields both sides name.
const asDb = (items: ItemDatabase) => items as unknown as Record<string, ItemDbEntry>;

const EMPTY_FOUNDRY = { building: [], recipes: [] } as ReturnType<typeof parseFoundry>;
const EMPTY: ParsedItem[] = [];

// Derived views are cached per (data, itemDb) identity, so every view that asks shares one parse until the data changes.
export const parsedFor = (data: RawInventoryData | null, items: ItemDatabase): ParsedItem[] =>
  data ? memoByIdentity("parsed", data, items, () => parseInventory(data, asDb(items))) : EMPTY;

const resourcesFor = (data: RawInventoryData | null, items: ItemDatabase): Resource[] =>
  data ? memoByIdentity("resources", data, items, () => parseResources(data, asDb(items))) : [];

export const foundryFor = (data: RawInventoryData | null, items: ItemDatabase) =>
  data && Object.keys(items).length ? memoByIdentity("foundry", data, items, () => parseFoundry(data, asDb(items))) : EMPTY_FOUNDRY;

const NO_OWNERSHIP = new Map<string, number>();
export const ownershipFor = (data: RawInventoryData | null, items: ItemDatabase) =>
  data
    ? memoByIdentity("ownership", data, items, () => aggregateComponentOwnership(withoutFoundryPending(data, (u) => asDb(items)[u]?.reusableBlueprint === true)))
    : NO_OWNERSHIP;

const NO_FAMILIES = new Set<string>();
export const subsumedFor = (data: RawInventoryData | null, items: ItemDatabase) =>
  data ? memoByIdentity("subsumed", data, items, () => buildSubsumedFamilySet(data, asDb(items))) : NO_FAMILIES;

export const chainFor = (foundry: ReturnType<typeof parseFoundry>, owned: Map<string, number>, items: ItemDatabase) =>
  memoByIdentity("chain", foundry, items, () => chainBuildableBlueprints(foundry.recipes, owned, asDb(items)));

export const useParsedInventory = parsedFor;
export const useResources = resourcesFor;
