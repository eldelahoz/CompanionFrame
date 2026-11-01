import { normalizeForSearch, normalizeForSlug } from "../../config/shared/textNormalize.js";

export { normalizeForSearch as normalizeMarketName } from "../../config/shared/textNormalize.js";

export function toMarketSlug(name: string): string {
  return normalizeForSlug(name) ?? "";
}

// warframe.market's gameRef points at DE's internalName but does not promise its
// casing, so every join on it folds through this one rule. It is a lookup key
// only: itemDb stays keyed by the exact internalName.
export function gameRefKey(value: unknown): string {
  return normalizeForSearch(value);
}
