import type { MyOrder } from "../ipc";

export type OrderSortKey = "name" | "platinum" | "quantity" | "date";
export type OrderSortDir = "asc" | "desc";

const time = (o: MyOrder): number => {
  const t = Date.parse(o.createdAt ?? "");
  return Number.isNaN(t) ? Number.NaN : t;
};

/** A sorted copy. Orders without a date go last in either direction; ties fall back to the name. */
export function sortOrders(orders: readonly MyOrder[], key: OrderSortKey, dir: OrderSortDir): MyOrder[] {
  const sign = dir === "asc" ? 1 : -1;
  const byName = (a: MyOrder, b: MyOrder) => a.itemName.localeCompare(b.itemName);
  return [...orders].sort((a, b) => {
    if (key === "name") return sign * byName(a, b);
    if (key === "date") {
      const [x, y] = [time(a), time(b)];
      if (Number.isNaN(x) || Number.isNaN(y)) return Number.isNaN(x) === Number.isNaN(y) ? byName(a, b) : Number.isNaN(x) ? 1 : -1;
      return sign * (x - y) || byName(a, b);
    }
    return sign * (a[key] - b[key]) || byName(a, b);
  });
}
