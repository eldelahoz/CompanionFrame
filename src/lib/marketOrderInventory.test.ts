import { describe, expect, it } from "vitest";
import type { ParsedItem } from "../types/inventory";
import type { WfmItemsLookup } from "../types/stats";
import { contractInventoryMatch } from "./contractInventoryMatch";
import { orderInventoryMatch, type ListedOrder } from "./marketOrderInventory";

const item = (over: Partial<ParsedItem>): ParsedItem =>
  ({ name: "Serration", internalName: "/Lotus/Upgrades/Mods/Rifle/Serration", category: "Mods", categoryLabel: "Mod", rank: 0, maxRank: 10, tradable: true, amount: 1, inventoryGroup: "mods", ...over }) as ParsedItem;

const order = (over: Partial<ListedOrder>): ListedOrder => ({ id: "o1", orderType: "sell", quantity: 1, modRank: null, subtype: null, itemName: "Serration", itemUrlName: "serration", ...over });
const wfmItems: WfmItemsLookup = { serration: { url_name: "serration", gameRef: "/Lotus/Upgrades/Mods/Rifle/Serration" } };

describe("orderInventoryMatch", () => {
  const match = (o: ListedOrder, parsed: ParsedItem[]) => orderInventoryMatch(o, parsed, wfmItems, {});

  it("has no opinion on buy orders", () => {
    expect(match(order({ orderType: "buy" }), [])).toEqual({ state: "match" });
  });
  it("flags a listing with nothing behind it", () => {
    expect(match(order({}), [])).toEqual({ state: "missing" });
  });
  it("matches a backed listing and flags partial backing", () => {
    expect(match(order({}), [item({})])).toEqual({ state: "match" });
    expect(match(order({ quantity: 3 }), [item({ amount: 2 })])).toEqual({ state: "partial", owned: 2, listed: 3 });
  });
  it("flags a rank the inventory does not hold", () => {
    expect(match(order({ modRank: 10 }), [item({ rank: 0 })])).toEqual({ state: "rank-mismatch", ownedRank: 0 });
    expect(match(order({ modRank: 0 }), [item({ rank: 0 })])).toEqual({ state: "match" });
  });
  it("never accuses a listing the inventory cannot model", () => {
    expect(match(order({ subtype: "atragraph" }), [])).toEqual({ state: "match" });
    expect(orderInventoryMatch(order({}), [], {}, {})).toEqual({ state: "match" }); // no game reference: no proof either way
  });
});

describe("contractInventoryMatch", () => {
  const riven = { weaponName: "Braton", rivenName: "Braton Critacan", currentRank: 8, maxRank: 8, rerolls: 3, masteryReq: 9 };
  const contract = { weaponUrlName: "braton", rivenSuffix: "critacan", modRank: 8, rerolls: 3, masteryLevel: 9 };

  it("matches, and treats a missing riven as missing", () => {
    expect(contractInventoryMatch(contract, [riven])).toEqual({ state: "match" });
    expect(contractInventoryMatch(contract, [])).toEqual({ state: "missing" });
  });
  it("reads a levelled riven as a rank mismatch and accepts max rank on an unranked one", () => {
    expect(contractInventoryMatch({ ...contract, modRank: 3 }, [{ ...riven, currentRank: 5, maxRank: 8 }])).toEqual({ state: "rank-mismatch", ownedRank: 5 });
    expect(contractInventoryMatch(contract, [{ ...riven, currentRank: 0 }])).toEqual({ state: "match" });
  });
  it("has no opinion without a full identity", () => {
    expect(contractInventoryMatch({ ...contract, rivenSuffix: null }, [])).toEqual({ state: "match" });
  });
});
