import { onWfmOrdersUpdated, wfmOrdersMy, wfmOrdersMyCached, wfmOrdersRefresh, type MyOrders } from "../lib/ipc";
import { createSharedStore } from "./shared";

export interface MyOrdersState {
  orders: MyOrders;
  /** Something real (a cached copy or a fetch) has landed, or there is nothing to wait for. */
  loaded: boolean;
  /** A fetch is under way: the rows shown may be stale. */
  refreshing: boolean;
}

const EMPTY: MyOrders = { sell: [], buy: [] };
/** A refresh whose answer never arrives (offline) must not spin forever. */
const GIVE_UP_MS = 20_000;

let push: ((p: Partial<MyOrdersState>) => void) | null = null;

/**
 * My warframe.market orders, shared by Market, the inventory "on sale" markers and the Workbench. The backend owns the
 * copy: this starts from it (instantly, however old) and follows `wfm-orders-updated`, so a returning view paints at once.
 */
const store = createSharedStore<MyOrdersState>({ orders: EMPTY, loaded: false, refreshing: false }, (patch) => {
  push = patch;
  let dead = false;
  let off: (() => void) | undefined;
  let giveUp: ReturnType<typeof setTimeout> | undefined;
  void onWfmOrdersUpdated((orders) => {
    clearTimeout(giveUp);
    patch({ orders, loaded: true, refreshing: false });
  }).then((u) => (dead ? u() : (off = u)));
  wfmOrdersMyCached().then(
    (r) => {
      if (dead) return;
      patch({ ...(r.orders && { orders: r.orders }), loaded: r.orders !== null || !r.refreshing, refreshing: r.refreshing });
      if (r.refreshing) giveUp = setTimeout(() => patch({ loaded: true, refreshing: false }), GIVE_UP_MS);
    },
    () => !dead && patch({ loaded: true, refreshing: false }),
  );
  return () => {
    dead = true;
    push = null;
    clearTimeout(giveUp);
    off?.();
  };
});

export const useMyOrders = store.use;
export const resetMyOrders = store.reset;

async function run(fetch: () => Promise<MyOrders>): Promise<MyOrders> {
  push?.({ refreshing: true });
  try {
    const orders = await fetch();
    push?.({ orders, loaded: true, refreshing: false });
    return orders;
  } catch (e) {
    push?.({ refreshing: false });
    throw e;
  }
}

/** The Refresh button: asks warframe.market now. */
export const refreshMyOrders = () => run(wfmOrdersRefresh);

/** My orders for code that needs them in hand (the backend answers from its copy while it is under 30 s old). */
export const loadMyOrders = () => run(wfmOrdersMy);
