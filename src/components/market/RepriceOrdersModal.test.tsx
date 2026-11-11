// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import { resetMock, wfmOrderCreate, wfmOrdersMy, wfmPrice, wfmSignIn, type MyOrder } from "../../lib/ipc";
import { resetJobs } from "../../stores/jobs";
import RepriceOrdersModal from "./RepriceOrdersModal";

const tr = translator("en");
// The jobs store outlives a test: a finished load from the last one would otherwise apply its books to this one.
beforeEach(() => {
  resetMock();
  resetJobs();
});
afterEach(cleanup);

const makeOrders = async (): Promise<MyOrder[]> => {
  await wfmSignIn("tenno@example.com", "pw");
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 90, quantity: 1 } as never);
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "BUY", platinum: 5, quantity: 1 } as never);
  return (await wfmOrdersMy()).sell;
};

test("previews nothing until prices load, then sends only what the user applies", async () => {
  const orders = await makeOrders();
  const onClose = vi.fn();
  render(<RepriceOrdersModal orders={orders} ownUserName="tenno" tr={tr} onClose={onClose} />);
  expect(screen.getByText("No market data")).toBeTruthy();
  expect((screen.getByText(/^Update 0$/) as HTMLButtonElement).disabled).toBe(true);

  fireEvent.click(screen.getByText("Load prices"));
  await waitFor(() => expect(screen.queryByText("No market data")).toBeNull());
  const apply = screen.getByText(/^Update 1$/) as HTMLButtonElement;
  const next = Number(document.querySelector("[data-reprice-row] td:last-child")!.textContent!.replace("p", ""));
  expect(next).toBeLessThan(90);

  fireEvent.click(apply);
  await waitFor(async () => expect((await wfmOrdersMy()).sell[0].platinum).toBe(next));
  expect(onClose).toHaveBeenCalled();
});

test("an unticked row is not sent", async () => {
  const orders = await makeOrders();
  render(<RepriceOrdersModal orders={orders} ownUserName={null} tr={tr} onClose={() => {}} />);
  fireEvent.click(screen.getByText("Load prices"));
  await waitFor(() => expect(screen.queryByText("No market data")).toBeNull());
  fireEvent.click(screen.getByLabelText("Serration"));
  expect((screen.getByText(/^Update 0$/) as HTMLButtonElement).disabled).toBe(true);
  expect((await wfmOrdersMy()).sell[0].platinum).toBe(90);
});

test("buy orders never enter the list", async () => {
  const orders = await makeOrders();
  const all = (await wfmOrdersMy()).buy.concat(orders);
  render(<RepriceOrdersModal orders={all} ownUserName={null} tr={tr} onClose={() => {}} />);
  expect(document.querySelectorAll("[data-reprice-row]")).toHaveLength(1);
});

test("a median strategy prices from the 48h reference plus the offset, inside the damping bound", async () => {
  const orders = await makeOrders();
  const ref = await wfmPrice(orders[0].itemSlug!);
  const reference = ref.status === "ok" ? ref.price : null;
  expect(reference).toBeGreaterThan(0);
  render(<RepriceOrdersModal orders={orders} ownUserName={null} tr={tr} onClose={() => {}} />);
  fireEvent.change(screen.getByLabelText("Strategy"), { target: { value: "median-48h" } });
  fireEvent.change(screen.getByLabelText("Offset (p)"), { target: { value: "60" } });
  fireEvent.click(screen.getByText("Load prices"));
  const cell = () => document.querySelector("[data-reprice-row] td:last-child")!.textContent;
  await waitFor(() => expect(cell()).toBe(`${Math.round(reference! + 60)}p`));
});

test("shift-click on a row checkbox unticks the rows between two clicks", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  for (const platinum of [90, 91, 92]) await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum, quantity: 1 } as never);
  const orders = (await wfmOrdersMy()).sell;
  render(<RepriceOrdersModal orders={orders} ownUserName={null} tr={tr} onClose={() => {}} />);
  fireEvent.click(screen.getByText("Load prices"));
  await waitFor(() => expect(screen.queryByText("No market data")).toBeNull());
  const boxes = () => screen.getAllByLabelText("Serration") as HTMLInputElement[];
  expect(boxes().map((b) => b.checked)).toEqual([true, true, true]);
  fireEvent.click(boxes()[0]);
  fireEvent.click(boxes()[2], { shiftKey: true });
  expect(boxes().map((b) => b.checked)).toEqual([false, false, false]);
});
