// Which detail modal is open. Module-level, so any view can open one and ModalHost renders it.
import { useSyncExternalStore } from "react";
import type { ComponentInfo, ParsedItem } from "../types/inventory";
import type { RelicGroup } from "../lib/relic/types";

export interface ActiveComponent {
  comp: ComponentInfo;
  parentName: string;
}

/** Views that only know a name open the item popup through ModalHost, which holds the item database. */
export type ItemRequest = { uniqueName: string } | { name: string };

interface ModalState {
  request: ItemRequest | null;
  item: ParsedItem | null;
  component: ActiveComponent | null;
  relic: RelicGroup | null;
}

let state: ModalState = { request: null, item: null, component: null, relic: null };
const listeners = new Set<() => void>();
const set = (patch: Partial<ModalState>) => {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
};

export const openItemBy = (request: ItemRequest) => set({ request });
export const clearItemRequest = () => set({ request: null });
export const setActiveItem = (item: ParsedItem | null) => set({ item });
export const setActiveComponent = (component: ActiveComponent | null) => set({ component });
/** The relic breakdown replaces the item popup, never stacks on it. */
export const openRelicDetailed = (relic: RelicGroup) => set({ item: null, relic });
export const setActiveRelic = (relic: RelicGroup | null) => set({ relic });

export const useModals = (): ModalState =>
  useSyncExternalStore(
    (cb) => (listeners.add(cb), () => void listeners.delete(cb)),
    () => state,
  );
