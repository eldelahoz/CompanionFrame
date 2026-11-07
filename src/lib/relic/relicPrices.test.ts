// @vitest-environment jsdom
import { expect, test, vi } from "vitest";

test("relic prices come from the shared store and update on a refreshed batch, with no bulk call", async () => {
  vi.resetModules();
  let send: (e: unknown[]) => void = () => {};
  const bulk = vi.fn();
  vi.doMock("../ipc", () => ({
    wfmPricesBulk: bulk,
    wfmPricesLoad: async () => [{ slug: "a", status: "ok", price: 7, stale: false }],
    onPricesRefreshed: async (cb: (e: unknown[]) => void) => ((send = cb), () => {}),
  }));
  const { act, renderHook, waitFor } = await import("@testing-library/react");
  const { relicPrice, useRelicPrices } = await import("./relicPrices");
  renderHook(() => useRelicPrices(["a", "b"]));
  await waitFor(() => expect(relicPrice("a")).toBe(7));
  expect(relicPrice("b")).toBeNull();
  act(() => send([{ slug: "b", status: "ok", price: 3, stale: false }]));
  await waitFor(() => expect(relicPrice("b")).toBe(3));
  expect(bulk).not.toHaveBeenCalled();
  vi.doUnmock("../ipc");
});
