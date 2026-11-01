import { getItemDatabase, onItemDbUpdated, type ItemDatabase } from "../lib/ipc";
import { setReady } from "./data";
import { createSharedStore } from "./shared";

const store = createSharedStore<{ db: ItemDatabase }>({ db: {} }, (patch) => {
  let live = true;
  const load = () =>
    void getItemDatabase().then((db) => {
      if (!live) return;
      patch({ db });
      setReady("items");
    });
  load();
  const off = onItemDbUpdated(load);
  return () => {
    live = false;
    void off.then((f) => f());
  };
});

/** The shared item DB: loaded once, reloaded on `item-db-updated` / `game-locale-updated`. */
export const useItemDatabase = (): ItemDatabase => store.use().db;
