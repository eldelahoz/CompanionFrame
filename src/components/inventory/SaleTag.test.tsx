// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

afterEach(cleanup);
import type { MyOrder } from "../../lib/ipc";
import { requestedView } from "../../stores/nav";
import SaleTag from "./SaleTag";

const tr = ((k: string, p?: Record<string, unknown>) => `${k}|${p?.quantity}|${p?.price}`) as never;
const order = (visible: boolean) => ({ id: "o", orderType: "sell", platinum: 35, quantity: 1, visible }) as MyOrder;

it("shows the price and the on-sale tooltip for a visible order", () => {
  render(<SaleTag order={order(true)} tr={tr} />);
  const tag = screen.getByRole("button");
  expect(tag.textContent).toBe("35p");
  expect(tag.getAttribute("data-inventory-sale-tag")).toBe("visible");
  expect(tag.title).toBe("inventory.onSaleTitle|1|35");
  fireEvent.click(tag);
  expect(requestedView.get()).toBe("market");
});

it("is muted with the hidden tooltip for a hidden order", () => {
  render(<SaleTag order={order(false)} tr={tr} />);
  const tag = screen.getByRole("button");
  expect(tag.classList.contains("is-hidden")).toBe(true);
  expect(tag.title).toBe("inventory.hiddenSaleTitle|1|35");
});
