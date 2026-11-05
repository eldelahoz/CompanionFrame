// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { resetMock, wfmOrderClose, wfmOrderCreate, wfmOrdersMy } from "../lib/ipc";
import { resetMyOrders } from "../stores/myOrders";
import { useWfmSession } from "../stores/wfmSession";
import { usePresence } from "../lib/wfm/presence";
import ConfirmHost from "../components/shell/ConfirmHost";
import ToastHost from "../components/shell/ToastHost";
import { clearToasts } from "../stores/toasts";
import MarketView from "./MarketView";

// While set, the backend's cached copy answers with this and its refresh never lands.
let cached: { orders: import("../lib/ipc").MyOrders | null; refreshing: boolean } | null = null;
let reputation: number | null = null;
// While set, order updates wait for it: lets a test observe a bulk action mid-run.
let updateGate: Promise<void> | null = null;
// Order writes past the first `through` wait for the gate; `calls` counts every write.
let through = Infinity;
const calls = { update: 0, delete: 0 };
vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  wfmMyReputation: async () => reputation,
  wfmOrderDelete: async (...a: Parameters<typeof import("../lib/ipc").wfmOrderDelete>) => {
    if (++calls.delete > through) await updateGate;
    return (await orig<typeof import("../lib/ipc")>()).wfmOrderDelete(...a);
  },
  wfmOrderUpdate: async (...a: Parameters<typeof import("../lib/ipc").wfmOrderUpdate>) => {
    if (++calls.update > through) await updateGate;
    return (await orig<typeof import("../lib/ipc")>()).wfmOrderUpdate(...a);
  },
  wfmOrdersMyCached: async () => cached ?? (await orig<typeof import("../lib/ipc")>()).wfmOrdersMyCached(),
}));

const tr = translator("en");
function Harness() {
  const wfm = useWfmSession();
  const presence = usePresence(wfm.session?.logged_in === true);
  return (
    <>
      <MarketView tr={tr} items={{}} session={wfm.session} signIn={wfm.signIn} signOut={wfm.signOut} presence={presence} />
      <ConfirmHost tr={tr} />
      <ToastHost tr={tr} />
    </>
  );
}

beforeEach(() => {
  reputation = null;
  cached = null;
  resetMyOrders();
  updateGate = null;
  through = Infinity;
  calls.update = 0;
  calls.delete = 0;
  resetMock();
  localStorage.clear();
});
afterEach(cleanup);

const signIn = async (password = "hunter2") => {
  fireEvent.change(await screen.findByLabelText("Email"), { target: { value: "tenno@example.com" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: password } });
  fireEvent.click(screen.getByText("Sign In"));
};

test("signed out the view only offers sign-in; the password is wiped and never logged", async () => {
  const spies = (["log", "info", "warn", "error"] as const).map((m) => vi.spyOn(console, m));
  render(<Harness />);
  await signIn();
  const pw = screen.getByLabelText("Password") as HTMLInputElement;
  expect(await screen.findByText("My Orders")).toBeTruthy();
  expect(document.body.innerHTML).not.toContain("hunter2");
  spies.forEach((s) => expect(JSON.stringify(s.mock.calls)).not.toContain("hunter2"));
  expect(pw.isConnected).toBe(false); // the form is gone
  spies.forEach((s) => s.mockRestore());
});

test("a rejected sign-in shows the error and clears the password field", async () => {
  render(<Harness />);
  await signIn("wrong");
  expect(await screen.findByText("Wrong email or password.")).toBeTruthy();
  expect((screen.getByLabelText("Password") as HTMLInputElement).value).toBe("");
});

test("signed in: create, edit price, close one, delete an order, and change presence", async () => {
  render(<Harness />);
  await signIn();
  // the header button; the empty state offers a second one
  fireEvent.click((await screen.findAllByText("New Order"))[0]);
  fireEvent.change(screen.getByLabelText("Search items..."), { target: { value: "serr" } });
  fireEvent.click(await screen.findByText("Serration"));
  fireEvent.change(screen.getByLabelText("Price"), { target: { value: "30" } });
  fireEvent.change(screen.getByLabelText("Quantity"), { target: { value: "3" } });
  fireEvent.click(screen.getByText("Create Order"));
  const row = await waitFor(() => {
    const r = document.querySelector("[data-market-order]");
    expect(r).toBeTruthy();
    return r as HTMLElement;
  });
  expect(row.textContent).toContain("Serration");

  fireEvent.click(screen.getByLabelText("Increase Platinum"));
  fireEvent.click(screen.getByText("Apply new price/quantity"));
  await waitFor(async () => expect((await wfmOrdersMy()).sell[0].platinum).toBe(31));

  fireEvent.click(screen.getByText("Sold"));
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(async () => expect((await wfmOrdersMy()).sell[0].quantity).toBe(2));

  fireEvent.click(screen.getByText("Online"));
  await waitFor(() => expect(document.querySelector('[data-wfm-status-option="online"]')?.className).toContain("success"));

  fireEvent.click(screen.getByLabelText("Delete"));
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(() => expect(document.querySelector("[data-market-order]")).toBeNull());
});

test("delete asks first: cancel keeps the order, confirm deletes it and toasts success", async () => {
  clearToasts();
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 5, quantity: 1 } as never);
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(document.querySelector("[data-market-order]")).toBeTruthy());
  fireEvent.click(screen.getByLabelText("Delete"));
  expect(await screen.findByText("Delete this order?")).toBeTruthy();
  fireEvent.click(screen.getByText("Cancel"));
  expect((await wfmOrdersMy()).sell).toHaveLength(1);
  expect(document.querySelector("[data-market-order]")).toBeTruthy();
  fireEvent.click(screen.getByLabelText("Delete"));
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(async () => expect((await wfmOrdersMy()).sell).toHaveLength(0));
  expect(await screen.findByText("Order deleted")).toBeTruthy();
});

test("existing orders load on sign-in and the buy tab filters by side", async () => {
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "BUY", platinum: 5, quantity: 1 } as never);
  render(<Harness />);
  await signIn();
  await screen.findByText("My Orders");
  expect(document.querySelector("[data-market-order]")).toBeNull(); // sell tab: none
  fireEvent.click(screen.getByText("Buy Orders"));
  expect(await screen.findByText("Serration")).toBeTruthy();
});

test("the Browse tab works without a session", async () => {
  render(<Harness />);
  fireEvent.click(await screen.findByText(tr("market.tab.browse")));
  expect(screen.getByRole("combobox")).toBeTruthy();
  expect(screen.queryByLabelText("Password")).toBeNull();
});

test("each order row has an item thumbnail whose larger preview opens on hover and keyboard focus", async () => {
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 5, quantity: 1 } as never);
  render(<Harness />);
  await signIn();
  const thumb = (await waitFor(() => {
    const t = document.querySelector("[data-market-order] [data-item-thumb]");
    expect(t).toBeTruthy();
    return t;
  })) as HTMLElement;
  expect(document.querySelector("[data-item-preview]")).toBeNull();
  fireEvent.pointerEnter(thumb);
  expect(document.querySelector("[data-item-preview]")?.textContent).toContain("Serration");
  fireEvent.pointerLeave(thumb);
  expect(document.querySelector("[data-item-preview]")).toBeNull();
  // jsdom has no :focus-visible; stub it so the keyboard path runs
  const matches = thumb.matches.bind(thumb);
  vi.spyOn(thumb, "matches").mockImplementation((s: string) => s === ":focus-visible" || matches(s));
  fireEvent.focus(thumb);
  expect(document.querySelector("[data-item-preview]")).toBeTruthy();
  fireEvent.blur(thumb);
  expect(document.querySelector("[data-item-preview]")).toBeNull();
});

test("a listing closed behind our back lowers its row from the backend's update, and a full close removes it", async () => {
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 5, quantity: 3 } as never);
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(document.querySelector("[data-market-order]")).toBeTruthy());
  const { id } = (await wfmOrdersMy()).sell[0];
  // price then quantity steppers: the last number input is the quantity
  const qty = () => [...document.querySelectorAll<HTMLInputElement>("[data-market-order] input[type=number]")].pop()?.value;

  // The mock backend announces a close the way the real one does (wfm-orders-updated).
  await act(async () => void (await wfmOrderClose(id, 1)));
  await waitFor(() => expect(qty()).toBe("2"));

  await act(async () => void (await wfmOrderClose(id, 2)));
  await waitFor(() => expect(document.querySelector("[data-market-order]")).toBeNull());
});

test("a returning visit paints the cached rows at once while the refresh spins", async () => {
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 5, quantity: 3 } as never);
  cached = { orders: await wfmOrdersMy(), refreshing: true };
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(document.querySelector("[data-market-order]")).toBeTruthy());
  expect(screen.getByLabelText("Refresh").getAttribute("aria-busy")).toBe("true");
});

test("the header dot follows the wfm status and the reputation shows only when known", async () => {
  reputation = 42;
  const { container, unmount } = render(<Harness />);
  await signIn();
  const user = await waitFor(() => {
    const el = container.querySelector("[data-market-user]");
    expect(el).toBeTruthy();
    return el as HTMLElement;
  });
  const dot = () => user.querySelector(".status-dot")!;
  fireEvent.click(await waitFor(() => container.querySelector('[data-wfm-status-option="ingame"]')!));
  await waitFor(() => expect(dot().className).toContain("bg-info"));
  fireEvent.click(container.querySelector('[data-wfm-status-option="online"]')!);
  await waitFor(() => expect(dot().className).toContain("bg-success"));
  fireEvent.click(container.querySelector('[data-wfm-status-option="invisible"]')!);
  await waitFor(() => expect(dot().className).toContain("bg-text-muted"));
  expect(user.querySelector("[data-market-reputation]")?.textContent).toContain("42");
  expect(screen.getByLabelText("Reputation")).toBeTruthy();
  unmount();

  reputation = null;
  const again = render(<Harness />);
  await waitFor(() => expect(again.container.querySelector("[data-market-user]")).toBeTruthy());
  expect(again.container.querySelector("[data-market-reputation]")).toBeNull();
});

test("a bulk hide shows its progress and a busy button while it runs", async () => {
  for (const platinum of [5, 6]) await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum, quantity: 1 } as never);
  let release!: () => void;
  updateGate = new Promise<void>((r) => (release = r));
  through = 0;
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(document.querySelectorAll("[data-market-order]")).toHaveLength(2));
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  fireEvent.click(await screen.findByRole("button", { name: "Hide" }));
  expect(await screen.findByText("Hiding 0 of 2...")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Hide" }).getAttribute("aria-busy")).toBe("true");
  expect((within(document.querySelector<HTMLElement>("[data-market-bulk-bar]")!).getByRole("button", { name: "Delete" }) as HTMLButtonElement).disabled).toBe(true);
  release();
  await waitFor(() => expect(screen.queryByText(/Hiding/)).toBeNull());
  expect(screen.getByText("2 of 2 selected")).toBeTruthy();
});

test("shift-click on an order checkbox selects the whole range in the shown order", async () => {
  for (const platinum of [5, 6, 7, 8]) await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum, quantity: 1 } as never);
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(document.querySelectorAll("[data-market-order]")).toHaveLength(4));
  const boxes = () => screen.getAllByLabelText(/^Select Serration/) as HTMLInputElement[];
  const checked = () => boxes().map((b) => b.checked);
  fireEvent.click(boxes()[0]);
  fireEvent.click(boxes()[2], { shiftKey: true });
  expect(checked()).toEqual([true, true, true, false]);
  // shift-click on a selected row unselects the range back to the last click
  fireEvent.click(boxes()[0], { shiftKey: true });
  expect(checked()).toEqual([false, false, false, false]);
});

const startDelete = async (orders: number, letThrough: number) => {
  for (let i = 0; i < orders; i++) await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 5 + i, quantity: 1 } as never);
  let release!: () => void;
  updateGate = new Promise<void>((r) => (release = r));
  through = letThrough;
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(document.querySelectorAll("[data-market-order]")).toHaveLength(orders));
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  fireEvent.click(within(document.querySelector<HTMLElement>("[data-market-bulk-bar]")!).getByRole("button", { name: "Delete" }));
  fireEvent.click(await screen.findByText("Confirm"));
  return release;
};
const rows = () => document.querySelectorAll("[data-market-order]").length;

test("Stop ends a bulk delete before the next order and keeps the rest selected", async () => {
  const release = await startDelete(3, 0);
  expect(await screen.findByText("Deleting 0 of 3...")).toBeTruthy();
  fireEvent.click(within(document.querySelector<HTMLElement>("[data-market-bulk-bar]")!).getByRole("button", { name: "Stop" }));
  release();
  await waitFor(() => expect(screen.queryByText(/Deleting/)).toBeNull());
  expect(calls.delete).toBe(1); // the one in flight finished, no further ones started
  await waitFor(() => expect(rows()).toBe(2));
  expect(screen.getByText("2 of 2 selected")).toBeTruthy();
});

test("a deleted row disappears before the bulk run ends", async () => {
  const release = await startDelete(3, 1);
  await waitFor(() => expect(rows()).toBe(2));
  expect(screen.getByText("Deleting 1 of 3...")).toBeTruthy();
  release();
  await waitFor(() => expect(rows()).toBe(0));
});

test("a hidden row flips its switch before the bulk run ends", async () => {
  for (const platinum of [5, 6]) await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum, quantity: 1 } as never);
  let release!: () => void;
  updateGate = new Promise<void>((r) => (release = r));
  through = 1;
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(rows()).toBe(2));
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  fireEvent.click(await screen.findByRole("button", { name: "Hide" }));
  await waitFor(() => expect(screen.getAllByRole("switch").filter((x) => (x as HTMLInputElement).checked)).toHaveLength(1));
  expect(screen.getByText("Hiding 1 of 2...")).toBeTruthy();
  release();
  await waitFor(() => expect(screen.queryByText(/Hiding/)).toBeNull());
});
