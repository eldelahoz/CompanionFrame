// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import ToastHost from "../shell/ToastHost";
import { translator } from "../../lib/i18n";
import type { MyOrder } from "../../lib/ipc";
import { clearToasts } from "../../stores/toasts";
import OrderModal from "./OrderModal";

const wfmErr = (kind: string, status?: number) => Object.assign(new Error(`${kind} ${status ?? ""}`), { kind, status });
const create = vi.fn();
const update = vi.fn();
vi.mock("../../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../../lib/ipc")>()),
  wfmOrderCreate: (...a: unknown[]) => create(...a),
  wfmOrderUpdate: (...a: unknown[]) => update(...a),
}));

const tr = translator("en");
const item = { id: "i1", slug: "serration", name: "Serration", thumb: null, icon: null, maxRank: null, gameRef: null };
const existing = { id: "o1", orderType: "sell", platinum: 10, quantity: 1, visible: true, itemName: "Serration" } as unknown as MyOrder;

const open = (order: MyOrder | null) => {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  render(
    <>
      <OrderModal order={order} defaultType="sell" catalog={null} draft={{ item, orderType: "sell", rank: null, subtype: null }} onClose={onClose} onSaved={onSaved} tr={tr} />
      <ToastHost tr={tr} />
    </>,
  );
  fireEvent.change(screen.getByLabelText("Price"), { target: { value: "30" } });
  fireEvent.click(screen.getByText(order ? "Save Changes" : "Create Order"));
  return { onClose, onSaved };
};

beforeEach(() => {
  clearToasts();
  create.mockReset();
  update.mockReset();
});
afterEach(cleanup);

test.each([["network", undefined], ["api", 503]] as const)("a create with an unknown outcome (%s) closes the dialog and warns", async (kind, status) => {
  create.mockRejectedValue(wfmErr(kind, status));
  const { onClose, onSaved } = open(null);
  expect(await screen.findByText(tr("orderModal.createUnknown"))).toBeTruthy();
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(onSaved).not.toHaveBeenCalled();
});

test("a create rejected with a 400 keeps the dialog open and shows the error", async () => {
  create.mockRejectedValue(wfmErr("api", 400));
  const { onClose } = open(null);
  await waitFor(() => expect(screen.getByText(/api 400/)).toBeTruthy());
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.queryByText(tr("orderModal.createUnknown"))).toBeNull();
});

test("an update with a network error keeps today's behavior: dialog open, error shown, no warning", async () => {
  update.mockRejectedValue(wfmErr("network"));
  const { onClose } = open(existing);
  await waitFor(() => expect(screen.getByText(/network/)).toBeTruthy());
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.queryByText(tr("orderModal.createUnknown"))).toBeNull();
});
