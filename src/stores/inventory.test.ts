// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import * as ipc from "../lib/ipc";
import * as lib from "../lib/inventory";
import { parsedFor, resetInventoryStore, useInventory } from "./inventory";

afterEach(cleanup);
let emit: (d: unknown) => void = () => undefined;
beforeEach(() => {
  vi.restoreAllMocks();
  resetInventoryStore();
  vi.spyOn(ipc, "getInventory").mockResolvedValue({ n: 1 } as never);
  vi.spyOn(ipc, "getInventoryStatus").mockResolvedValue(null as never);
  vi.spyOn(ipc, "onInventoryStatusUpdated").mockResolvedValue(() => undefined);
  vi.spyOn(ipc, "onInventoryUpdated").mockImplementation(((cb: (d: unknown) => void) => {
    emit = cb;
    return Promise.resolve(() => undefined);
  }) as never);
});

it("two consumers share one fetch and both follow inventory-updated", async () => {
  const a = renderHook(() => useInventory());
  const b = renderHook(() => useInventory());
  await waitFor(() => expect(a.result.current.loaded).toBe(true));
  expect(ipc.getInventory).toHaveBeenCalledTimes(1);
  act(() => emit({ n: 2 }));
  expect(a.result.current.data).toEqual({ n: 2 });
  expect(b.result.current.data).toEqual({ n: 2 });
});

it("a remounted consumer paints the cached data at once", async () => {
  const a = renderHook(() => useInventory());
  await waitFor(() => expect(a.result.current.loaded).toBe(true));
  a.unmount();
  const b = renderHook(() => useInventory());
  expect(b.result.current.data).toEqual({ n: 1 });
});

it("parses once per data identity and again for new data", () => {
  const spy = vi.spyOn(lib, "parseInventory").mockReturnValue([]);
  const db = {};
  const d1 = { n: 1 } as never;
  parsedFor(d1, db);
  parsedFor(d1, db);
  expect(spy).toHaveBeenCalledTimes(1);
  parsedFor({ n: 2 } as never, db);
  expect(spy).toHaveBeenCalledTimes(2);
});
