// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { resetMock, wfmOrderCreate } from "../lib/ipc";
import { resetMyOrders } from "../stores/myOrders";
import { useWfmSession } from "../stores/wfmSession";
import { usePresence } from "../lib/wfm/presence";
import ConfirmHost from "../components/shell/ConfirmHost";
import ToastHost from "../components/shell/ToastHost";
import { clearToasts } from "../stores/toasts";
import MarketView from "./MarketView";

const wfmErr = (kind: string, status?: number) => Object.assign(new Error(`boom ${kind} ${status ?? ""}`), { kind, status });
const close = vi.fn();
vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  wfmMyReputation: async () => null,
  wfmOrderClose: (...a: unknown[]) => close(...a),
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
  clearToasts();
  close.mockReset();
  resetMyOrders();
  resetMock();
  localStorage.clear();
});
afterEach(cleanup);

const closeFirstOrder = async () => {
  await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum: 5, quantity: 1 } as never);
  render(<Harness />);
  fireEvent.change(await screen.findByLabelText("Email"), { target: { value: "tenno@example.com" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "pw" } });
  fireEvent.click(screen.getByText("Sign In"));
  await waitFor(() => expect(document.querySelector("[data-market-order]")).toBeTruthy());
  fireEvent.click(screen.getByText("Sold"));
  fireEvent.click(await screen.findByText("Confirm"));
};

test("a close with an unknown outcome shows the warning once, and no error toast", async () => {
  close.mockRejectedValue(wfmErr("network"));
  await closeFirstOrder();
  await screen.findByText(tr("market.closeUnknown"));
  await waitFor(() => expect(close).toHaveBeenCalledTimes(1));
  expect(screen.getAllByText(tr("market.closeUnknown"))).toHaveLength(1);
  expect(screen.queryByText(/boom/)).toBeNull();
});

test("a close refused with a 400 still shows the error toast, not the warning", async () => {
  close.mockRejectedValue(wfmErr("api", 400));
  await closeFirstOrder();
  expect(await screen.findByText(/boom api 400/)).toBeTruthy();
  expect(screen.queryByText(tr("market.closeUnknown"))).toBeNull();
});
