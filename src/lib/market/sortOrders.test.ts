import { describe, expect, it } from "vitest";
import type { MyOrder } from "../ipc";
import { sortOrders } from "./sortOrders";

const o = (itemName: string, platinum: number, quantity: number, createdAt: string | null): MyOrder =>
  ({ id: itemName, itemName, platinum, quantity, createdAt, updatedAt: null }) as unknown as MyOrder;
const list = [o("Beta", 20, 1, "2026-03-02T00:00:00Z"), o("Alpha", 20, 5, "2026-03-03T00:00:00Z"), o("Gamma", 5, 2, null), o("Delta", 50, 3, "2026-03-01T00:00:00Z")];
const names = (key: Parameters<typeof sortOrders>[1], dir: "asc" | "desc") => sortOrders(list, key, dir).map((x) => x.itemName);

describe("sortOrders", () => {
  it("sorts by name", () => {
    expect(names("name", "asc")).toEqual(["Alpha", "Beta", "Delta", "Gamma"]);
    expect(names("name", "desc")).toEqual(["Gamma", "Delta", "Beta", "Alpha"]);
  });
  it("sorts by platinum, ties by name", () => {
    expect(names("platinum", "asc")).toEqual(["Gamma", "Alpha", "Beta", "Delta"]);
    expect(names("platinum", "desc")).toEqual(["Delta", "Alpha", "Beta", "Gamma"]);
  });
  it("sorts by quantity", () => {
    expect(names("quantity", "asc")).toEqual(["Beta", "Gamma", "Delta", "Alpha"]);
    expect(names("quantity", "desc")).toEqual(["Alpha", "Delta", "Gamma", "Beta"]);
  });
  it("sorts by date with undated orders last either way", () => {
    expect(names("date", "desc")).toEqual(["Alpha", "Beta", "Delta", "Gamma"]);
    expect(names("date", "asc")).toEqual(["Delta", "Beta", "Alpha", "Gamma"]);
  });
  it("does not mutate the input", () => {
    sortOrders(list, "name", "asc");
    expect(list[0].itemName).toBe("Beta");
  });
});
