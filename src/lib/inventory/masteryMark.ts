import type { MasteryStatus } from "../../../config/shared/masteryTypes";
import type { ItemDbEntry, ParsedItem } from "../../types/inventory";
import { setRootOf } from "./fullSets";
import { componentParentOf } from "./partConsumers";

export interface MasteryMark {
  status: MasteryStatus;
  rank: number;
  maxRank: number;
}

interface MasteryRow extends MasteryMark {
  uniqueName: string;
}

/** uniqueName -> mastery state, built once per progress snapshot. */
export const buildMasteryIndex = (items: readonly MasteryRow[]): ReadonlyMap<string, MasteryMark> =>
  new Map(items.map((i) => [i.uniqueName, { status: i.status, rank: i.rank, maxRank: i.maxRank }]));

/** The mastery state of the masterable item an inventory row is, or belongs to (a part, or a set's root); null for the rest. */
export function masteryOfItem(item: ParsedItem, index: ReadonlyMap<string, MasteryMark>, itemDb: Record<string, ItemDbEntry>): MasteryMark | null {
  const root = setRootOf(item.uniqueName ?? item.internalName);
  return index.get(root) ?? index.get(componentParentOf(root, itemDb) ?? "") ?? null;
}

/** Masterable and not finished: unmasterable rows (null) never count. */
export const isNotMastered = (mark: MasteryMark | null): boolean => mark !== null && mark.status !== "mastered";
