// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { MyOrder } from "../ipc";
import { useOnSale } from "./useOnSale";

const sell = [{ id: "a", orderType: "sell" }] as MyOrder[];
const onSaleByItem = vi.fn(() => new Map([["row", sell[0]]]));
vi.mock("../marketOrderInventory", () => ({ onSaleByItem: (...a: unknown[]) => (onSaleByItem as (...x: unknown[]) => unknown)(...a) }));
vi.mock("../market/wfmLookup", () => ({ wfmLookup: () => ({}) }));
vi.mock("../../stores/market", () => ({ useWfmCatalog: () => ({ catalog: null }) }));
vi.mock("../../stores/myOrders", () => ({ useMyOrders: () => ({ orders: { sell, buy: [] }, loaded: true, refreshing: false }) }));

test("on-sale markers come from the shared orders store, and only while signed in", () => {
  const signedOut = renderHook(() => useOnSale(false, []));
  expect(signedOut.result.current.size).toBe(0);
  expect(onSaleByItem).not.toHaveBeenCalled();

  const signedIn = renderHook(() => useOnSale(true, []));
  expect(signedIn.result.current.size).toBe(1);
  expect(onSaleByItem).toHaveBeenCalledWith(sell, [], {});
});
