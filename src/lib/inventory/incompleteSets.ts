import { componentUniqueNameAliases } from "../../../config/shared/componentNames";
import type { ComponentInfo, ParsedItem } from "../../types/inventory";
import { QUALITY_MODES } from "../relic/relicConstants";
import { ownedRelicQualities } from "../relic/relicInventory";
import type { OwnedCounts, RelicDatabase, RelicGroup, RelicQuality } from "../relic/types";

export interface RelicSource {
  group: RelicGroup;
  rarity: string;
  /** Owned copies per refinement, empty ones dropped. */
  ownedQualities: Array<{ quality: RelicQuality; count: number }>;
  ownedTotal: number;
}

export interface MissingPart {
  part: ComponentInfo;
  have: number;
  need: number;
  /** warframe.market slug of the part, when the catalog knows it; priced through the normal queue. */
  slug: string | null;
  /** null while the relic database is not loaded, so "Not from relics" is never claimed early. */
  sources: RelicSource[] | null;
}

export interface IncompleteSetRow {
  set: ParsedItem;
  owned: ComponentInfo[];
  missing: MissingPart[];
}

interface Hit {
  group: RelicGroup;
  rarity: string;
}

export interface RelicRewardIndex {
  byUniqueName: Map<string, Hit[]>;
  byName: Map<string, Hit[]>;
}

const nameKey = (name: string): string => name.trim().toLowerCase();

function push(index: Map<string, Hit[]>, key: string, hit: Hit): void {
  const hits = index.get(key);
  if (!hits) index.set(key, [hit]);
  else if (!hits.some((h) => h.group === hit.group)) hits.push(hit);
}

/** Every relic that lists a reward, by the reward's unique name and by its English name. Build once per database. */
export function buildRelicRewardIndex(db: RelicDatabase): RelicRewardIndex {
  const index: RelicRewardIndex = { byUniqueName: new Map(), byName: new Map() };
  for (const group of Object.values(db.groups)) {
    for (const quality of QUALITY_MODES) {
      for (const reward of group.qualities[quality]?.rewards ?? []) {
        const hit = { group, rarity: reward.rarity };
        if (reward.uniqueName) push(index.byUniqueName, reward.uniqueName, hit);
        if (reward.name) push(index.byName, nameKey(reward.name), hit);
      }
    }
  }
  return index;
}

const rarityRank = (rarity: string): number => ["common", "uncommon", "rare"].indexOf(rarity.toLowerCase());

function sourcesFor(part: ComponentInfo, setName: string, index: RelicRewardIndex, owned: OwnedCounts): RelicSource[] {
  const hits = new Map<RelicGroup, Hit>();
  for (const alias of part.uniqueName ? componentUniqueNameAliases(part.uniqueName) : []) {
    for (const hit of index.byUniqueName.get(alias) ?? []) hits.set(hit.group, hit);
  }
  // The relic data names a part "Ash Prime Systems"; the set's component is only "Systems".
  if (hits.size === 0) {
    const root = setName.replace(/ Set$/, "");
    for (const hit of index.byName.get(nameKey(`${root} ${part.name}`)) ?? []) hits.set(hit.group, hit);
  }
  return [...hits.values()]
    .map(({ group, rarity }) => {
      const ownedQualities = ownedRelicQualities(owned, group.key);
      return { group, rarity, ownedQualities, ownedTotal: ownedQualities.reduce((n, q) => n + q.count, 0) };
    })
    .sort(
      (a, b) =>
        b.ownedTotal - a.ownedTotal ||
        Number(a.group.vaulted) - Number(b.group.vaulted) ||
        rarityRank(a.rarity) - rarityRank(b.rarity) ||
        a.group.key.localeCompare(b.group.key),
    );
}

/** Incomplete sets, closest to complete first, then by `value` (0 when unpriced). */
export function buildIncompleteSets(
  sets: readonly ParsedItem[],
  index: RelicRewardIndex | null,
  owned: OwnedCounts,
  value: (set: ParsedItem) => number = () => 0,
  slugOf: (part: ComponentInfo, set: ParsedItem) => string | null = () => null,
): IncompleteSetRow[] {
  return sets
    .filter((set) => set.inventoryGroup === "incomplete_sets")
    .map((set) => {
      const parts = set.components ?? [];
      const missing = parts
        .filter((p) => !p.owned)
        .map((part) => ({
          part,
          have: part.ownedCount ?? 0,
          need: part.itemCount ?? 1,
          slug: slugOf(part, set),
          sources: index ? sourcesFor(part, set.name, index, owned) : null,
        }));
      return { set, owned: parts.filter((p) => (p.ownedCount ?? 0) > 0), missing };
    })
    .sort(
      (a, b) =>
        a.missing.length - b.missing.length ||
        b.owned.length - a.owned.length ||
        value(b.set) - value(a.set) ||
        a.set.name.localeCompare(b.set.name),
    );
}

/** The part's market slug: "<root> <part>" ("Ash Prime Systems"), as the relic data names it. */
export const partSlug = (part: ComponentInfo, set: ParsedItem, byName: (name: string) => { slug: string } | null): string | null =>
  byName(`${set.name.replace(/ Set$/, "")} ${part.name}`)?.slug ?? null;

/** Sum of the missing parts' known prices (x the copies still needed); `priced` says how many parts that covers. */
export function missingPartsPrice(missing: readonly MissingPart[], price: (slug: string) => number | null | undefined): { total: number; priced: number } {
  let total = 0;
  let priced = 0;
  for (const m of missing) {
    const p = m.slug ? price(m.slug) : null;
    if (p == null) continue;
    total += p * Math.max(1, m.need - m.have);
    priced++;
  }
  return { total, priced };
}
