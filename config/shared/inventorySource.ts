/** Where the inventory snapshot came from; "helper" only in the opt-in game-memory mode. */
export type InventorySource = "manual" | "aleca" | "helper" | "none";

const DEFAULT_INVENTORY_SOURCE: InventorySource = "none";

export function normalizeInventorySource(raw: unknown): InventorySource {
  return raw === "manual" || raw === "aleca" || raw === "helper" ? raw : DEFAULT_INVENTORY_SOURCE;
}
