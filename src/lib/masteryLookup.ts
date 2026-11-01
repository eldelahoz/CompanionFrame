import { componentParentOf } from "./inventory/partConsumers";
import type { ItemDbEntry, MasteryStatus } from "../types/inventory";

interface MasteryRow {
  uniqueName?: string;
  internalName?: string;
  name: string;
  status?: MasteryStatus;
  currentlyOwned?: boolean;
}

interface MasteryLookup {
  byUniqueName: Map<string, MasteryStatus>;
  byName: Map<string, MasteryStatus>;
  /** Ownership of the very rows above, keyed the same way. An item mastered and
   *  then sold stays "mastered" here and false there. */
  ownedByUniqueName: Map<string, boolean>;
  ownedByName: Map<string, boolean>;
}

interface MasteryFacts {
  status: MasteryStatus;
  /** `ParsedItem.currentlyOwned`: the built item is in the inventory now. */
  owned: boolean;
}

function normalizeLookupKey(value: string | null | undefined): string {
  return (value || "").trim().toLowerCase();
}

/** Plain Maps: callers rebuild the whole lookup inside a reactive statement,
 *  so per-entry reactivity would never be observed. */
export function buildMasteryLookup(data: { items: MasteryRow[] } | null): MasteryLookup {
  const byUniqueName = new Map<string, MasteryStatus>();
  const byName = new Map<string, MasteryStatus>();
  const ownedByUniqueName = new Map<string, boolean>();
  const ownedByName = new Map<string, boolean>();

  for (const item of data?.items ?? []) {
    const status = item.status;
    if (!status) continue;

    const uniqueName = item.uniqueName || item.internalName;
    if (uniqueName && !byUniqueName.has(uniqueName)) {
      byUniqueName.set(uniqueName, status);
      ownedByUniqueName.set(uniqueName, item.currentlyOwned === true);
    }

    const nameKey = normalizeLookupKey(item.name);
    if (nameKey && !byName.has(nameKey)) {
      byName.set(nameKey, status);
      ownedByName.set(nameKey, item.currentlyOwned === true);
    }
  }

  return { byUniqueName, byName, ownedByUniqueName, ownedByName };
}

function factsOf(
  lookup: MasteryLookup,
  uniqueName: string | null | undefined,
  name: string | null | undefined,
): MasteryFacts | undefined {
  if (uniqueName) {
    const status = lookup.byUniqueName.get(uniqueName);
    if (status) return { status, owned: lookup.ownedByUniqueName.get(uniqueName) === true };
  }
  const nameKey = normalizeLookupKey(name);
  const status = lookup.byName.get(nameKey);
  return status ? { status, owned: lookup.ownedByName.get(nameKey) === true } : undefined;
}

/** A build component has no mastery of its own and carries the row of the item
 *  it builds; anything else answers for itself, by uniqueName then name. */
function inheritedMasteryFacts(
  lookup: MasteryLookup,
  itemDb: Record<string, ItemDbEntry>,
  uniqueName: string | null | undefined,
  name: string | null | undefined,
): MasteryFacts | undefined {
  if (uniqueName) {
    const direct = factsOf(lookup, uniqueName, null);
    if (direct) return direct;
    const parent = componentParentOf(uniqueName, itemDb);
    if (parent) {
      const inherited = factsOf(lookup, parent, itemDb[parent]?.name);
      if (inherited) return inherited;
    }
  }
  return factsOf(lookup, null, name);
}

export function inheritedMasteryStatus(
  lookup: MasteryLookup,
  itemDb: Record<string, ItemDbEntry>,
  uniqueName: string | null | undefined,
  name: string | null | undefined,
): MasteryStatus | undefined {
  return inheritedMasteryFacts(lookup, itemDb, uniqueName, name)?.status;
}
