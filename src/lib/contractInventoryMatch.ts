import { normalizeWfmSlugKey } from "../../config/shared/wfm";
import type { DecodedRiven } from "../bindings";
import type { WfmContract } from "./ipc";
import type { ListingInventoryMatch } from "./marketListing";
import { foldName } from "./foldName";
import { rivenNameSuffix, sameWeapon } from "./marketContract";

type ContractIdentity = Pick<WfmContract, "weaponUrlName" | "rivenSuffix" | "modRank" | "rerolls" | "masteryLevel">;
type OwnedRiven = Pick<DecodedRiven, "weaponName" | "rivenName" | "currentRank" | "maxRank" | "rerolls" | "masteryReq">;

/**
 * Whether an active riven listing still lines up with the inventory. Name and
 * rank both have to agree, so a levelled riven reads as a mismatch, not missing.
 */
export function contractInventoryMatch(contract: ContractIdentity, owned: readonly OwnedRiven[]): ListingInventoryMatch {
  const suffix = foldName(contract.rivenSuffix ?? "");
  const slug = normalizeWfmSlugKey(contract.weaponUrlName);
  // Without both halves of the identity any answer would be a guess.
  if (!suffix || !slug) return { state: "match" };

  const named = owned.filter((riven) => sameWeapon(slug, riven.weaponName ?? "") && foldName(rivenNameSuffix(riven.rivenName ?? "", riven.weaponName ?? "")) === suffix);
  if (named.length === 0) return { state: "missing" };

  // The suffix is derived from the buff tags, so twins share a name; rerolls and
  // mastery level separate them when the contract states them.
  const narrowed = named.filter(
    (riven) => (contract.rerolls == null || riven.rerolls === contract.rerolls) && (contract.masteryLevel == null || riven.masteryReq === contract.masteryLevel),
  );
  const candidates = narrowed.length > 0 ? narrowed : named;

  // Listing at max rank is a feature of the riven modal, so a rank-0 riven offered at its max rank is no mismatch.
  const sameRank = (riven: OwnedRiven) => contract.modRank == null || riven.currentRank === contract.modRank || riven.maxRank === contract.modRank;
  if (candidates.some(sameRank)) return { state: "match" };
  return { state: "rank-mismatch", ownedRank: candidates[0].currentRank ?? 0 };
}
