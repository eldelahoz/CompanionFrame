// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import * as ipc from "../../lib/ipc";
import { mockMarketDown, resetMock, wfmOrderCreate, wfmOrdersMy, wfmSignIn, type BookOrder, type MyOrder } from "../../lib/ipc";
import { workbenchMedianLoader } from "../../lib/tradeWorkbench/medianReference";
import { resetJobs } from "../../stores/jobs";
import RepriceOrdersModal from "./RepriceOrdersModal";

// The mock job finishes at once, so the test decides which books the backend "has".
let available: Set<string> | null = null;
let books: Record<string, BookOrder[]> = {};
vi.mock("../../lib/ipc", async (orig) => {
  const actual = await orig<typeof import("../../lib/ipc")>();
  return {
    ...actual,
    jobsStartMarketLoad: vi.fn(actual.jobsStartMarketLoad),
    jobsMarketBooks: vi.fn(async (slugs: string[]) => {
      const real = await actual.jobsMarketBooks(slugs);
      if (!available) return real;
      return real.filter((b) => available!.has(b.slug)).map((b) => ({ ...b, orders: books[b.slug] ?? b.orders }));
    }),
  };
});

const tr = translator("en");
const NO_BOOK = "No market data";
beforeEach(() => {
  available = null;
  books = {};
  vi.mocked(ipc.jobsStartMarketLoad).mockClear();
  resetMock();
  resetJobs();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const sell = (rank: number | null, price: number, itemId = "mock_serration") => wfmOrderCreate({ itemId, orderType: "SELL", platinum: price, quantity: 1, modRank: rank } as never);
const ordersOf = async (): Promise<MyOrder[]> => (await wfmOrdersMy()).sell;
const open = (orders: MyOrder[]) => render(<RepriceOrdersModal orders={orders} ownUserName={null} tr={tr} onClose={() => {}} />);
const load = () => fireEvent.click(screen.getByText("Load prices"));
const noBooks = () => screen.queryAllByText(NO_BOOK).length;
const sellers = (rank: number | null, prices: number[]): BookOrder[] =>
  prices.map((platinum, i) => ({ id: `${rank}-${platinum}`, type: "sell", platinum, quantity: 1, rank, visible: true, user: { ingameName: `Seller${rank}_${i}`, status: "ingame", platform: "pc", avatar: null } }) as BookOrder);
const nextOf = (label: string) => screen.getByText(label).closest("tr")!.querySelector("td:last-child")!.textContent;

test("closing the modal mid-load keeps the books; reopening applies them and a later load reads only the rest", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  await sell(null, 90);
  await sell(null, 90, "mock_vitality");
  const orders = await ordersOf();
  available = new Set(["serration"]);

  open(orders);
  load();
  await waitFor(() => expect(noBooks()).toBe(1)); // vitality has no book yet
  cleanup();

  // Reopened for the same orders: serration is priced without loading again.
  open(orders);
  await waitFor(() => expect(noBooks()).toBe(1));
  expect(nextOf("Serration")).not.toContain(NO_BOOK);
  expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(1);

  // The rest lands later: the next load asks only for it and the row follows.
  available.add("vitality");
  load();
  await waitFor(() => expect(noBooks()).toBe(0));
  expect(vi.mocked(ipc.jobsStartMarketLoad).mock.calls[1][0].map((r) => r.slug)).toEqual(["vitality"]);
});

test("a rank 0 order is priced against rank 0 sellers, a maxed one against max-rank sellers", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  // Both below their book, so the price rises and the damping never holds it back.
  await sell(0, 5);
  await sell(10, 20);
  available = new Set(["serration"]);
  books = { serration: [...sellers(0, [10, 12, 13]), ...sellers(10, [50, 52, 53])] };

  open(await ordersOf());
  fireEvent.change(screen.getByLabelText("Strategy"), { target: { value: "match-cheapest" } });
  load();
  await waitFor(() => expect(noBooks()).toBe(0));
  expect(nextOf("R0")).toBe("10p");
  expect(nextOf("R10")).toBe("50p");
});

test("an item whose load failed gets the no-book skip reason, the others are priced", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  await sell(null, 90);
  await sell(null, 90, "mock_vitality");
  mockMarketDown.add("vitality");
  open(await ordersOf());
  load();
  await waitFor(() => expect(nextOf("Vitality")).toContain(NO_BOOK));
  await waitFor(() => expect(nextOf("Serration")).not.toContain(NO_BOOK));
  expect(noBooks()).toBe(1);
});

test("a median strategy loads its reference once, after the job is done", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  await sell(null, 10); // below the 22p median: a raise is never damped
  const spy = vi.spyOn(workbenchMedianLoader, "load");
  open(await ordersOf());
  fireEvent.change(screen.getByLabelText("Strategy"), { target: { value: "median-48h" } });
  expect(spy).not.toHaveBeenCalled(); // no books yet
  load();
  await waitFor(() => expect(spy).toHaveBeenCalledTimes(1));
  await waitFor(() => expect(nextOf("Serration")).toBe("22p"));
  expect(spy).toHaveBeenCalledTimes(1);
});
