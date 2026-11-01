// @vitest-environment jsdom
import { beforeEach, expect, test, vi } from "vitest";
import type { ParsedItem } from "../../types/inventory";
import { computeInventoryValueTotals } from "./valueTotals";
import { medianOfRecent, mediansOfRecent, resetItemPricesForTest, type PriceInfo } from "./itemPrices";
import { rowAvg, rowPrice, sortRows, valueEntries, type InvRow } from "./inventoryRows";

beforeEach(resetItemPricesForTest);

const day = 86_400_000;
const pt = (daysAgo: number, median: number, rank?: number | null) => ({ time: Date.now() - daysAgo * day, median, rank }) as never;

test("the average is the median of the last week, rank-less or rank 0 only", () => {
  expect(medianOfRecent([pt(1, 10), pt(2, 30), pt(3, 20), pt(20, 999), pt(1, 500, 10)])).toBe(20);
  expect(medianOfRecent([pt(1, 10), pt(2, 20)])).toBe(15);
  expect(medianOfRecent([pt(30, 10)])).toBeNull();
});

const row = (name: string, slug: string | null, amount = 1, ducats: number | null = null): InvRow => ({
  item: { name, internalName: name, amount, ducats } as unknown as ParsedItem,
  key: name, artSlug: slug,
  slug,
  canTrade: !!slug,
  maxRank: 0,
  label: name,
});

test("sorting by a price puts unpriced rows last in both directions", () => {
  const prices = new Map<string, PriceInfo>([
    ["a", { state: "ok", price: 5, maxPrice: null, avg: 4, maxAvg: null, avgState: "ok" }],
    ["b", { state: "ok", price: 9, maxPrice: null, avg: null, maxAvg: null, avgState: "none" }],
  ]);
  const rows = [row("zeta", null), row("a", "a"), row("b", "b")];
  expect(sortRows(rows, prices, "platinum", true).map((r) => r.key)).toEqual(["b", "a", "zeta"]);
  expect(sortRows(rows, prices, "platinum", false).map((r) => r.key)).toEqual(["a", "b", "zeta"]);
  expect(sortRows(rows, prices, "name", false).map((r) => r.key)).toEqual(["a", "b", "zeta"]);
});

test("value totals multiply stacks, floor at one price hole and count ducats for prime parts only", () => {
  const prime = { inventoryGroup: "all_parts", isPrime: true, tradable: true, amount: 3, marketSlug: "x", platinum: 10, ducats: 45 } as const;
  const totals = computeInventoryValueTotals([prime, { ...prime, marketSlug: "y", platinum: null, ducats: null }, { ...prime, marketSlug: null }], "prime");
  expect(totals).toMatchObject({ platinum: 30, ducats: 135, platinumUnpriced: 1, ducatsUnpriced: 1, unpriced: 1, counted: 2 });
  expect(computeInventoryValueTotals([prime], "prime", 15).counted).toBe(0);
});

test("a transient price failure is not cached, it is asked again after the backoff; a no_data answer is cached", async () => {
  vi.resetModules();
  const calls: string[][] = [];
  let failed = true;
  vi.doMock("../ipc", () => ({
    wfmPricesBulk: async (slugs: string[]) => {
      calls.push(slugs);
      return slugs.map((slug) => ({ slug, status: failed ? "failed" : slug === "none" ? "no_data" : "ok", price: failed || slug === "none" ? null : 7, stale: false }));
    },
    wfmStatsHistory: async () => [],
    onPricesRefreshed: async () => () => {},
  }));
  const { renderHook, waitFor } = await import("@testing-library/react");
  const mod = await import("./itemPrices");
  const slugs = ["a", "none"];
  const first = renderHook(() => mod.useItemPrices(slugs, []));
  await waitFor(() => expect(calls.length).toBe(1));
  expect(first.result.current.get("a")?.state).toBe("loading");
  first.unmount();
  failed = false;
  const second = renderHook(() => mod.useItemPrices(slugs, []));
  await waitFor(() => expect(second.result.current.get("a")?.price).toBe(7), { timeout: 5000 }); // after the 2 s backoff
  expect(second.result.current.get("none")?.state).toBe("none");
  vi.doUnmock("../ipc");
}, 10000);

test("a slug that keeps failing gives up into a retry state, and retrying asks again", async () => {
  vi.resetModules();
  vi.useFakeTimers();
  let failed = true;
  vi.doMock("../ipc", () => ({
    wfmPricesBulk: async (slugs: string[]) => slugs.map((slug) => ({ slug, status: failed ? "failed" : "ok", price: failed ? null : 3, stale: false })),
    wfmStatsHistory: async () => [],
    onPricesRefreshed: async () => () => {},
  }));
  const { renderHook, act } = await import("@testing-library/react");
  const mod = await import("./itemPrices");
  const slugs = ["a"];
  const hook = renderHook(() => ({ prices: mod.useItemPrices(slugs, []), progress: mod.useItemPriceProgress(slugs) }));
  await act(() => vi.advanceTimersByTimeAsync(60_000));
  expect(hook.result.current.progress).toMatchObject({ done: 0, failed: 1, total: 1 });
  failed = false;
  await act(async () => hook.result.current.progress.retry());
  await act(() => vi.advanceTimersByTimeAsync(10));
  expect(hook.result.current.prices.get("a")?.price).toBe(3);
  expect(hook.result.current.progress).toMatchObject({ done: 1, failed: 0 });
  vi.useRealTimers();
  vi.doUnmock("../ipc");
});

test("a dropped bulk call is retried once and concurrent hooks share one request", async () => {
  vi.resetModules();
  let calls = 0;
  vi.doMock("../ipc", () => ({
    wfmPricesBulk: async (slugs: string[]) => {
      if (++calls === 1) throw new Error("Failed to fetch");
      return slugs.map((slug) => ({ slug, status: "ok", price: 7, stale: false }));
    },
    wfmStatsHistory: async () => [],
    onPricesRefreshed: async () => () => {},
  }));
  const { renderHook, waitFor } = await import("@testing-library/react");
  const mod = await import("./itemPrices");
  const slugs = ["a"];
  const one = renderHook(() => mod.useItemPrices(slugs, []));
  const two = renderHook(() => mod.useItemPrices(slugs, []));
  await waitFor(() => expect(one.result.current.get("a")?.price).toBe(7));
  expect(two.result.current.get("a")?.price).toBe(7);
  expect(calls).toBe(2); // the failed try and its retry, not one pair per hook
  vi.doUnmock("../ipc");
});

const mod = (rank: number, maxRank = 10): InvRow => ({ ...row("Adaptation", "adaptation"), item: { name: "Adaptation", internalName: `Adaptation#r${rank}`, rank, maxRank, inventoryGroup: "mods", amount: 1 } as unknown as ParsedItem, key: `adaptation#${rank}`, maxRank });

test("one stats payload prices the max-rank copy and the unranked copy differently, in the sort and in the totals", () => {
  const info: PriceInfo = { state: "ok", price: 4, maxPrice: 30, avg: null, maxAvg: null, avgState: "none" };
  const prices = new Map([["adaptation", info]]);
  expect(rowPrice(mod(0), info)).toEqual({ price: 4, basis: "plain" });
  expect(rowPrice(mod(10), info)).toEqual({ price: 30, basis: "max" });
  expect(rowPrice(mod(5), info)).toEqual({ price: 4, basis: "rank0" });
  // No max-rank sale yet: the max copy falls back to the rank-0 price.
  expect(rowPrice(mod(10), { ...info, maxPrice: null }).price).toBe(4);
  expect(valueEntries([mod(0), mod(10)], prices).map((e) => e.platinum)).toEqual([4, 30]);
});

test("the rank copies of one mod stay adjacent, lowest rank first, whatever the sort", () => {
  const info = (price: number, maxPrice: number | null = null): PriceInfo => ({ state: "ok", price, maxPrice, avg: null, maxAvg: null, avgState: "none" });
  const prices = new Map([["adaptation", info(4, 30)], ["other", info(2)]]);
  const rows = [mod(10), { ...row("Other", "other") }, mod(0), mod(5)];
  const keys = (key: "name" | "platinum", desc: boolean) => sortRows(rows, prices, key, desc).map((r) => r.key);
  expect(keys("name", false)).toEqual(["adaptation#0", "adaptation#5", "adaptation#10", "Other"]);
  // By price the 30p max copy leads its group, the group stays together in rank order.
  expect(keys("platinum", true)).toEqual(["adaptation#0", "adaptation#5", "adaptation#10", "Other"]);
  expect(keys("platinum", false)).toEqual(["Other", "adaptation#0", "adaptation#5", "adaptation#10"]);
});

test("a mod with rank 0 and rank 10 history gets two medians; a 10/10 and a 0/10 copy show different 48h and 7-day values", () => {
  const history = [pt(1, 4, 0), pt(2, 6, 0), pt(1, 40, 10), pt(3, 60, 10)];
  const m = mediansOfRecent(history);
  expect(m).toEqual({ avg: 5, maxAvg: 50 });
  expect(mediansOfRecent([pt(1, 4)])).toEqual({ avg: 4, maxAvg: null });
  const info: PriceInfo = { state: "ok", price: 4, maxPrice: 30, avg: m.avg, maxAvg: m.maxAvg, avgState: "ok" };
  const copy = (rank: number) => ({ item: { rank }, maxRank: 10, slug: "adaptation" }) as unknown as InvRow;
  expect([rowPrice(copy(10), info).price, rowAvg(copy(10), info)]).toEqual([30, 50]);
  expect([rowPrice(copy(0), info).price, rowAvg(copy(0), info)]).toEqual([4, 5]);
});

test("a refresh event replaces the cached price; cached answers are known at once and a failed refresh entry is ignored", async () => {
  vi.resetModules();
  vi.doMock("../ipc", () => ({
    wfmPricesBulk: async (slugs: string[]) => slugs.map((slug) => ({ slug, status: "ok", price: 5, maxPrice: null, stale: true, updatedAt: 1 })),
    wfmStatsHistory: async () => [],
    onPricesRefreshed: async () => () => {},
  }));
  const { act, renderHook, waitFor } = await import("@testing-library/react");
  const mod = await import("./itemPrices");
  const slugs = ["a", "b"];
  const hook = renderHook(() => mod.useItemPrices(slugs, []));
  await waitFor(() => expect(hook.result.current.get("a")?.price).toBe(5));
  expect(hook.result.current.get("a")?.state).toBe("ok"); // stale-but-cached is not "loading"
  act(() => mod.applyRefreshed([{ slug: "a", status: "ok", price: 9, maxPrice: 20, stale: false }, { slug: "b", status: "failed", price: null, stale: false }] as never));
  expect(hook.result.current.get("a")).toMatchObject({ price: 9, maxPrice: 20 });
  expect(hook.result.current.get("b")?.price).toBe(5);
  vi.doUnmock("../ipc");
});

test("a refresh event recomputes the weekly median of slugs already shown, without a request for the rest", async () => {
  vi.resetModules();
  let median = 10;
  const asked: string[] = [];
  vi.doMock("../ipc", () => ({
    wfmPricesBulk: async (slugs: string[]) => slugs.map((slug) => ({ slug, status: "ok", price: 5, maxPrice: null, stale: false, updatedAt: 1 })),
    wfmStatsHistory: async (slug: string) => (asked.push(slug), [pt(1, median)]),
    onPricesRefreshed: async () => () => {},
  }));
  const { act, renderHook, waitFor } = await import("@testing-library/react");
  const mod = await import("./itemPrices");
  const hook = renderHook(() => mod.useItemPrices(["a"], ["a"]));
  await waitFor(() => expect(hook.result.current.get("a")?.avg).toBe(10));
  median = 14;
  await act(async () => mod.applyRefreshed([{ slug: "a", status: "ok", price: 9, stale: false }, { slug: "z", status: "ok", price: 1, stale: false }] as never));
  await waitFor(() => expect(hook.result.current.get("a")?.avg).toBe(14));
  expect(asked).toEqual(["a", "a"]);
  vi.doUnmock("../ipc");
});
