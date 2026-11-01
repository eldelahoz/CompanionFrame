// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { resetMock, wfmOrderCreate, wfmOrdersMy } from "../lib/ipc";
import { useWfmSession } from "../stores/wfmSession";
import { usePresence } from "../lib/wfm/presence";
import ConfirmHost from "../components/shell/ConfirmHost";
import ToastHost from "../components/shell/ToastHost";
import { clearToasts } from "../stores/toasts";
import MarketView from "./MarketView";

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
