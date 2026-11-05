// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { MyOrder, MyOrders } from "../lib/ipc";
import { resetMyOrders, useMyOrders } from "./myOrders";

const order = (id: string, platinum: number): MyOrder =>
  ({ id, orderType: "sell", platinum, quantity: 1, perTrade: 1, visible: true, itemName: "Serration" }) as MyOrder;

let pushUpdate: ((o: MyOrders) => void) | undefined;
let cached: { orders: MyOrders | null; refreshing: boolean } = { orders: null, refreshing: false };
vi.mock("../lib/ipc", () => ({
  wfmOrdersMyCached: async () => cached,
  wfmOrdersMy: async () => ({ sell: [], buy: [] }),
  wfmOrdersRefresh: async () => ({ sell: [], buy: [] }),
  onWfmOrdersUpdated: async (cb: (o: MyOrders) => void) => {
    pushUpdate = cb;
    return () => {};
  },
}));

beforeEach(resetMyOrders);
afterEach(cleanup);

test("starts from the backend's cached copy, then follows its updates", async () => {
  cached = { orders: { sell: [order("a", 5)], buy: [] }, refreshing: true };
  const { result } = renderHook(() => useMyOrders());
  await waitFor(() => expect(result.current.orders.sell.map((o) => o.id)).toEqual(["a"]));
  expect(result.current).toMatchObject({ loaded: true, refreshing: true });

  act(() => pushUpdate!({ sell: [order("a", 9), order("b", 1)], buy: [] }));
  expect(result.current.orders.sell.map((o) => o.platinum)).toEqual([9, 1]);
  expect(result.current.refreshing).toBe(false);
});

test("nothing cached and nothing refreshing is loaded and empty, not a spinner", async () => {
  cached = { orders: null, refreshing: false };
  const { result } = renderHook(() => useMyOrders());
  await waitFor(() => expect(result.current.loaded).toBe(true));
  expect(result.current.orders).toEqual({ sell: [], buy: [] });
});
