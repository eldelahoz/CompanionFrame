import { getMasteryProgress, onInventoryUpdated, onItemDbUpdated, onProfileInventoryUpdated } from "../lib/ipc";
import { createSharedStore } from "./shared";

type Mastery = Awaited<ReturnType<typeof getMasteryProgress>>;

const store = createSharedStore<{ data: Mastery; loaded: boolean }>({ data: null, loaded: false }, (patch) => {
  let live = true;
  const load = () =>
    void getMasteryProgress()
      .then((data) => live && patch({ data, loaded: true }))
      .catch(() => live && patch({ data: null, loaded: true }));
  load();
  const offs = [onItemDbUpdated(load), onInventoryUpdated(load), onProfileInventoryUpdated(load)];
  return () => {
    live = false;
    offs.forEach((off) => void off.then((f) => f()));
  };
});

/** The shared mastery progress; `data` is `undefined`-free, `loaded` says whether the first read finished. */
export const useMasteryProgress = store.use;
