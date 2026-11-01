// Hydration state for game data (item DB, relics, drops...). Backend loaders flip `ready` per key as they finish.

export type DataKey = "items" | "relics" | "drops" | "inventory";
type State = Partial<Record<DataKey, boolean>>;

let state: State = {};
const listeners = new Set<() => void>();

export const getDataState = (): State => state;

export function setReady(key: DataKey, ready = true): void {
  if (state[key] === ready) return;
  state = { ...state, [key]: ready };
  listeners.forEach((l) => l());
}

export const resetData = (): void => {
  state = {};
  listeners.forEach((l) => l());
};
