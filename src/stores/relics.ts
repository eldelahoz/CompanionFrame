// Relic planner view state. Module-level so it outlives the view; only the tier tab is persisted.
import { useSyncExternalStore } from "react";
import { DEFAULT_RELIC_PLANNER_FILTERS, type RelicPlannerFilters } from "../../config/shared/relicPlannerView";

export type RelicOwnershipMode = "owned" | "all";

export interface RelicViewState extends RelicPlannerFilters {
  tierFilter: string;
  ownershipMode: RelicOwnershipMode;
}

const TAB_KEY = "wf_relics_tab";
export const RELIC_TABS = ["all", "Lith", "Meso", "Neo", "Axi", "Requiem"] as const;

const readTab = (): string => {
  try {
    const raw = localStorage.getItem(TAB_KEY);
    return raw && (RELIC_TABS as readonly string[]).includes(raw) ? raw : "all";
  } catch {
    return "all";
  }
};

const initial = (): RelicViewState => ({ ...DEFAULT_RELIC_PLANNER_FILTERS, tierFilter: readTab(), ownershipMode: "owned" });

let state = initial();
const listeners = new Set<() => void>();

export function setRelicFilter(patch: Partial<RelicViewState>): void {
  if (patch.tierFilter && (RELIC_TABS as readonly string[]).includes(patch.tierFilter)) {
    try {
      localStorage.setItem(TAB_KEY, patch.tierFilter);
    } catch {
      /* private window or blocked storage: the tab just is not remembered */
    }
  }
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

export const resetRelicView = (): void => {
  state = initial();
  listeners.forEach((l) => l());
};

export const useRelicView = (): RelicViewState =>
  useSyncExternalStore(
    (cb) => (listeners.add(cb), () => void listeners.delete(cb)),
    () => state,
  );
