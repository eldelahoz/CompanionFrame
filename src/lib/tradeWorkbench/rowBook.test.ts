import { beforeEach, describe, expect, it, vi } from "vitest";

const fetchItemOrderBookBySlug = vi.hoisted(() => vi.fn());
vi.mock("../wfm/orderBook", () => ({ fetchItemOrderBookBySlug }));

import { fetchRowBook } from "./rowBook";

describe("fetchRowBook", () => {
  beforeEach(() => fetchItemOrderBookBySlug.mockReset());

  it("asks for the row's own relic refinement and rank", async () => {
    fetchItemOrderBookBySlug.mockResolvedValue({ status: "ok", data: { sell: [], buy: [] } });
    await fetchRowBook({ slug: "axi_a1_relic", rank: null, subtype: "radiant" });
    expect(fetchItemOrderBookBySlug).toHaveBeenCalledWith("axi_a1_relic", {
      rank: null,
      subtype: "radiant",
    });
  });

  it("returns both books, or null when the load failed", async () => {
    const sell = [{ platinum: 10 }];
    fetchItemOrderBookBySlug.mockResolvedValueOnce({ status: "ok", data: { sell, buy: null } });
    await expect(fetchRowBook({ slug: "serration", rank: 10, subtype: null })).resolves.toEqual({
      sell,
      buy: null,
    });
    fetchItemOrderBookBySlug.mockResolvedValueOnce({ status: "error", slug: "serration" });
    await expect(fetchRowBook({ slug: "serration", rank: 10, subtype: null })).resolves.toBeNull();
  });
});
