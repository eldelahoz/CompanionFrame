import type { RelicDatabase, RelicGroup, RelicReward } from "../../bindings";
import type { RelicQuality } from "../../../config/shared/relicPlannerView";

export type { RelicDatabase, RelicGroup, RelicQuality, RelicReward };

export type OwnedQualityCounts = Record<RelicQuality, number>;
/** Copies owned per relic group key and refinement. */
export type OwnedCounts = Record<string, OwnedQualityCounts>;
