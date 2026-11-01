import type { Translator } from "./i18n";

/** Whether a listing still lines up with the inventory. "match" doubles as "no
 *  opinion", so an unjudgeable listing is never called dead. Riven contracts
 *  cannot be partially backed; they simply never produce that state. */
export type ListingInventoryMatch =
  | { state: "match" }
  | { state: "missing" }
  | { state: "partial"; owned: number; listed: number }
  | { state: "rank-mismatch"; ownedRank: number };

interface ListingWarning {
  label: string;
  title: string;
}

/** Null when the listing is fine or unjudgeable. The translator is an argument
 *  so the call site keeps the reactive dependency on it and this module stays
 *  free of store imports. */
export function listingWarning(
  match: ListingInventoryMatch | null,
  listedRank: number | null | undefined,
  tr: Translator,
): ListingWarning | null {
  if (match == null || match.state === "match") return null;
  if (match.state === "missing") {
    return {
      label: tr("market.listing.missingFromInventory"),
      title: tr("market.listing.missingTitle"),
    };
  }
  if (match.state === "partial") {
    const params = { owned: match.owned, listed: match.listed };
    return {
      label: tr("market.listing.partialBacking", params),
      title: tr("market.listing.partialTitle", params),
    };
  }
  return {
    label: tr("market.listing.rankMismatch", { owned: match.ownedRank }),
    title: tr("market.listing.rankMismatchTitle", {
      listed: listedRank ?? 0,
      owned: match.ownedRank,
    }),
  };
}
