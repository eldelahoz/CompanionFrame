import { normalizeWfmSlugKey } from "../../config/shared/wfm";
import { polarityToWfm, tagToWfmUrlName } from "../../config/shared/wfmRivenVocabulary";
import { VARIANT_PREFIXES, VARIANT_SUFFIXES } from "../../config/shared/weaponVariants";
import type { DecodedRiven } from "../bindings";
import { foldName } from "./foldName";
import type { WfmContract } from "./ipc";

type Attribute = WfmContract["stats"][number];

/** The words a contract stat is searched and shown by: its label, else its slug with spaces. */
export function attributeKeyword(attribute: Pick<Attribute, "label" | "urlName">): string {
  if (typeof attribute.label === "string" && attribute.label.trim()) return attribute.label;
  if (typeof attribute.urlName === "string" && attribute.urlName.trim()) return attribute.urlName.replace(/_/g, " ");
  return "";
}

type ContractIdentity = Pick<WfmContract, "weaponUrlName" | "rivenSuffix" | "modRank" | "rerolls" | "masteryLevel">;
type OwnedRiven = Pick<DecodedRiven, "weaponName" | "rivenName" | "currentRank" | "maxRank" | "rerolls" | "masteryReq">;

function collapseSpaces(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}

/**
 * The generated part of a riven name, keeping the casing it came in. This is
 * what WFM stores as rivenSuffix and what the in-game chat tag shows.
 */
export function rivenNameSuffix(rivenName: string, weaponName: string): string {
  const name = collapseSpaces(rivenName ?? "");
  const weapon = collapseSpaces(weaponName ?? "");
  // Slice the collapsed strings, never the raw ones: the offset has to match the
  // side the prefix test ran on or a padded name cuts in the wrong place.
  if (!weapon || !name.toLowerCase().startsWith(weapon.toLowerCase())) return name;
  return name.slice(weapon.length).trim();
}

function ownedSuffixKey(riven: OwnedRiven): string {
  return foldName(rivenNameSuffix(riven.rivenName ?? "", riven.weaponName ?? ""));
}

// warframe.market spells "&" as "and" in its slugs (silva_and_aegis), so folding
// the game name straight to underscores would never meet its side.
function weaponSlug(weaponName: string): string {
  return normalizeWfmSlugKey(weaponName.replace(/&/g, " and "));
}

// WFM keys a riven on the family slug, which strips the variant affix from both
// ends: Kuva Karak is listed as karak and MK1-Braton as braton.
export function familySlug(weaponName: string): string {
  let name = weaponName.trim();
  for (const suffix of VARIANT_SUFFIXES) {
    if (!name.toLowerCase().endsWith(suffix.toLowerCase())) continue;
    name = name.slice(0, -suffix.length);
    break;
  }
  for (const prefix of VARIANT_PREFIXES) {
    if (!name.toLowerCase().startsWith(prefix.toLowerCase())) continue;
    name = name.slice(prefix.length);
    break;
  }
  return weaponSlug(name);
}

// A riven covers the whole weapon family, so "rubico" and "rubico_prime" match
// either way round. The unstripped slug stays in play because a weapon whose
// base form was never made keeps its affix on the WFM side too (kuva_bramma).
export function sameWeapon(contractSlug: string, ownedWeaponName: string): boolean {
  if (!contractSlug) return false;
  return [weaponSlug(ownedWeaponName), familySlug(ownedWeaponName)].some(
    (owned) =>
      owned !== "" &&
      (owned === contractSlug ||
        owned.startsWith(`${contractSlug}_`) ||
        contractSlug.startsWith(`${owned}_`)),
  );
}

// The suffix is derived from the buff tags, so two rolls of the same stats on
// one weapon share a name. These separate the twins; a contract that states
// none of them, or that narrows to nothing, keeps the whole name set.
type IdentityHint = (contract: ContractIdentity, riven: OwnedRiven) => boolean;

const sameRerolls: IdentityHint = (contract, riven) =>
  contract.rerolls == null || riven.rerolls === contract.rerolls;

const sameMasteryLevel: IdentityHint = (contract, riven) =>
  contract.masteryLevel == null || riven.masteryReq === contract.masteryLevel;

// Listing at max rank is a feature of the riven modal, so a rank-0 riven offered
// at its max rank is exactly what the user asked for, not a mismatch.
const sameModRank: IdentityHint = (contract, riven) =>
  contract.modRank == null ||
  riven.currentRank === contract.modRank ||
  riven.maxRank === contract.modRank;

type ListedRiven = OwnedRiven & Pick<DecodedRiven, "itemId" | "polarity" | "stats">;
type ListedContract = ContractIdentity & Pick<WfmContract, "polarity" | "stats">;

// Both sides carry the same upgrade tag, so the WFM attribute slug is the one
// vocabulary they share. The display labels disagree on most elemental and
// faction stats, and half the stat names contain "damage".
function ownedStatKey(stat: DecodedRiven["stats"][number]): string {
  return tagToWfmUrlName(stat.tag) ?? "";
}

function contractStatKey(attribute: Attribute): string {
  return String(attribute.urlName ?? "")
    .trim()
    .toLowerCase();
}

/** Exact set equality on WFM attribute slugs, used only when a name is absent. */
function sameStatSet(
  attributes: readonly Attribute[],
  stats: readonly DecodedRiven["stats"][number][],
): boolean {
  if (attributes.length === 0 || attributes.length !== stats.length) return false;
  const pool = stats.map((stat) => ({ key: ownedStatKey(stat), used: false }));
  for (const attribute of attributes) {
    const key = contractStatKey(attribute);
    const hit = key ? pool.find((stat) => !stat.used && stat.key === key) : undefined;
    if (!hit) return false;
    hit.used = true;
  }
  return true;
}

function sameRiven(contract: ListedContract, riven: ListedRiven): boolean {
  const suffix = foldName(contract.rivenSuffix ?? "");
  const owned = ownedSuffixKey(riven);
  if (suffix && owned) return suffix === owned;
  return sameStatSet(contract.stats, riven.stats);
}

// A percentage stat shows one decimal, so the two sides can round the same roll
// to values one display step apart.
const STAT_VALUE_EPSILON = 0.1;

function samePolarity(contract: ListedContract, riven: ListedRiven): boolean {
  const listed = polarityToWfm(contract.polarity);
  const own = polarityToWfm(riven.polarity);
  // An unknown polarity on either side separates nobody.
  return !listed || !own || listed === own;
}

function attributeValue(attribute: Attribute): number | null {
  // Number(null) and Number("") are 0, which would demand a zero roll instead of
  // reading as the absent value it is.
  if (attribute.value == null || attribute.value === "") return null;
  const value = Number(attribute.value);
  // Signs travel differently on the two sides, and one stat name never appears
  // twice on a riven, so magnitude is the part worth comparing.
  return Number.isFinite(value) ? Math.abs(value) : null;
}

/** The numeric rolls, which separate twins the shared suffix cannot. */
function sameStatValues(contract: ListedContract, riven: ListedRiven): boolean {
  const pool = (riven.stats ?? []).map((stat) => ({
    key: ownedStatKey(stat),
    // Listing an unranked riven at max rank is a feature of the riven modal, so
    // the listed roll is either the copy's own value or that value at rank 8.
    values: [Math.abs(stat.displayValue ?? NaN), Math.abs(stat.maxRankValue ?? NaN)],
    used: false,
  }));

  for (const attribute of contract.stats ?? []) {
    const value = attributeValue(attribute);
    const key = contractStatKey(attribute);
    if (value == null || !key) continue;
    const hit = pool.find(
      (stat) =>
        !stat.used &&
        stat.key === key &&
        stat.values.some((own) => Math.abs(own - value) <= STAT_VALUE_EPSILON),
    );
    if (!hit) return false;
    hit.used = true;
  }
  return true;
}

const NARROWING_HINTS: ReadonlyArray<(contract: ListedContract, riven: ListedRiven) => boolean> = [
  sameRerolls,
  sameMasteryLevel,
  sameModRank,
  samePolarity,
  sameStatValues,
];

function candidateRivens(
  contract: ListedContract,
  rivens: readonly ListedRiven[],
): readonly ListedRiven[] {
  const slug = normalizeWfmSlugKey(contract.weaponUrlName);
  if (!slug) return [];

  let pool = rivens.filter(
    (riven) => sameWeapon(slug, riven.weaponName ?? "") && sameRiven(contract, riven),
  );
  for (const hint of NARROWING_HINTS) {
    if (pool.length <= 1) break;
    const next = pool.filter((riven) => hint(contract, riven));
    // A hint nobody answers is stale, not disqualifying: a riven rerolled or
    // levelled after it was listed still carries its own auction.
    if (next.length > 0) pool = next;
  }
  return pool;
}

/**
 * Maps each owned riven to the auction listing it, taking only forced pairs. An
 * unmarked riven costs a badge; a wrong mark can remove the wrong listing.
 */
export function matchRivenListings<C extends ListedContract>(
  rivens: readonly ListedRiven[],
  contracts: readonly C[],
): Map<string, C> {
  const matched = new Map<string, C>();
  const claimed = new Set<string>();
  const open = contracts
    .map((contract) => ({ contract, candidates: candidateRivens(contract, rivens), done: false }))
    .filter((entry) => entry.candidates.length > 0);

  const claim = (riven: ListedRiven, contract: C): void => {
    claimed.add(riven.itemId);
    matched.set(riven.itemId, contract);
  };
  const free = (entry: (typeof open)[number]): readonly ListedRiven[] =>
    entry.candidates.filter((riven) => !claimed.has(riven.itemId));

  // Claiming in one pass would let the first auction take a riven a later
  // auction can prove is its own, so repeat until no new pair is forced.
  let settled = true;
  while (settled) {
    settled = false;
    for (const entry of open) {
      if (entry.done) continue;
      const rest = free(entry);
      if (rest.length > 1) continue;
      entry.done = true;
      if (rest.length === 1) {
        claim(rest[0], entry.contract);
        settled = true;
      }
    }
  }

  // Auctions that cannot tell the same rivens apart still prove every one of
  // them is listed once there are at least as many auctions as rivens. Which
  // auction lands on which riven is arbitrary, but listedness is never wrong.
  const groups = new Map<string, { entries: typeof open; rivens: readonly ListedRiven[] }>();
  for (const entry of open) {
    if (entry.done) continue;
    const rest = free(entry);
    const signature = rest
      .map((riven) => riven.itemId)
      .sort()
      .join("\0");
    const group = groups.get(signature);
    if (group) group.entries.push(entry);
    else groups.set(signature, { entries: [entry], rivens: rest });
  }

  // Signatures are read from one snapshot, so a riven that two groups both reach
  // could only be settled by taking the contracts in some order. Leave it alone.
  const seen = new Set<string>();
  const shared = new Set<string>();
  for (const group of groups.values()) {
    for (const riven of group.rivens) {
      if (seen.has(riven.itemId)) shared.add(riven.itemId);
      else seen.add(riven.itemId);
    }
  }

  for (const group of groups.values()) {
    // Fewer auctions than twins proves nothing about which of them is listed.
    if (group.entries.length < group.rivens.length) continue;
    if (group.rivens.some((riven) => shared.has(riven.itemId))) continue;
    group.rivens.forEach((riven, index) => claim(riven, group.entries[index].contract));
  }

  return matched;
}
