import { beforeEach, describe, expect, it } from "vitest";
import { subtypeChoicesOf } from "../../../config/shared/wfmOrders";
import { resetMock, wfmErrorOf, wfmOrderClose, wfmOrderCreate, wfmOrderDelete, wfmOrderUpdate, wfmOrdersMy, wfmOrdersSetVisible } from "../ipc";

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

  it("reports each order of a bulk visibility change", async () => {
    const a = await wfmOrderCreate(order as never);
    const got = await wfmOrdersSetVisible([a.id, "f".repeat(24)], false);
    expect(got[0]).toMatchObject({ id: a.id, error: null, order: { visible: false } });
    expect(got[1].error).toBe("Order not found.");
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
