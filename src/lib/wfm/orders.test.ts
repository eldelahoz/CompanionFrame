import { beforeEach, describe, expect, it, vi } from "vitest";
import { subtypeChoicesOf } from "../../../config/shared/wfmOrders";
import { resetMock, wfmErrorOf, wfmOrderClose, wfmOrderCreate, wfmOrderDelete, wfmOrderUpdate, wfmOrdersMy, jobsStartOrdersVisibility, onJobUpdated, type JobUpdate } from "../ipc";

const order = { itemId: "mock_serration", orderType: "SELL", platinum: 10, quantity: 6 };

beforeEach(resetMock);

describe("orders ipc (mock backend)", () => {
  it("creates, updates, closes and deletes, keeping veiled riven subtypes", async () => {
    const o = await wfmOrderCreate({ ...order, subtype: "Unrevealed" } as never);
    expect(o).toMatchObject({ orderType: "sell", itemName: "Serration", subtype: "unrevealed", modRank: null });
    expect((await wfmOrdersMy()).sell).toHaveLength(1);

    expect(await wfmOrderUpdate(o.id, { subtype: "revealed", platinum: 20 } as never)).toMatchObject({ subtype: "revealed", platinum: 20 });
    await wfmOrderClose(o.id, 2, 2);
    expect((await wfmOrdersMy()).sell[0].quantity).toBe(4);
    await wfmOrderClose(o.id, 4);
    expect((await wfmOrdersMy()).sell).toEqual([]);

    const again = await wfmOrderCreate(order as never);
    await wfmOrderDelete(again.id);
    await expect(wfmOrderDelete(again.id)).rejects.toThrow("Order not found.");
  });

  it("runs a hide as a job that reports each order and keeps the failed ones for a retry", async () => {
    const a = await wfmOrderCreate(order as never);
    const b = await wfmOrderCreate(order as never);
    const seen: JobUpdate[] = [];
    const off = await onJobUpdated((u) => seen.push(u));
    await jobsStartOrdersVisibility([a.id, b.id], false);
    await vi.waitFor(() => expect(seen[seen.length - 1]).toMatchObject({ state: "done", done: 2, failed: 0 }));
    expect((await wfmOrdersMy()).sell.map((o) => o.visible)).toEqual([false, false]);
    expect(seen[0].job?.items).toHaveLength(2);

    await wfmOrderDelete(a.id);
    const second = await jobsStartOrdersVisibility([b.id], true);
    await vi.waitFor(async () => expect((await wfmOrdersMy()).sell[0].visible).toBe(true));
    expect(second).not.toBe(seen[0].id);
    off();
  });
});

describe("wfmErrorOf", () => {
  it("keeps the subtype choices where the shared helper reads them", () => {
    const err = wfmErrorOf({ kind: "subtype_required", subtypes: ["intact", "radiant"] });
    expect(subtypeChoicesOf(err)).toEqual(["intact", "radiant"]);
  });

  it("uses the WFM detail as the message", () => {
    expect(wfmErrorOf({ kind: "api", detail: "HTTP 400: platinum" }).message).toBe("HTTP 400: platinum");
    expect(wfmErrorOf({ kind: "rate_limited", retry_after_secs: 3 }).message).toMatch(/rate limited/i);
  });
});
