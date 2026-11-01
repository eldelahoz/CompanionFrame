// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { translator } from "../../lib/i18n";
import type { TradeEvent } from "../../types/stats";
import ConfirmHost from "../shell/ConfirmHost";
import { MatchModal } from "./ledger";

const ipc = vi.hoisted(() => ({ match: vi.fn(), close: vi.fn() }));
vi.mock("../../lib/ipc", () => ({ workbenchMatchTrade: ipc.match, workbenchCloseMatch: ipc.close }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const tr = translator("en");
const event = { id: "e1", date: "2026-01-01T00:00:00Z", type: "sale", platChange: 50, partner: "Tenno", items: [{ internalName: "x", displayName: "Serration", count: 1, direction: "given" }] } as TradeEvent;
const match = { kind: "order", orderId: "o1", itemName: "Serration", itemUrlName: "serration", itemThumb: null, quantity: 1, platinum: 50, partner: "Tenno", type: "sale", perTrade: null };

const onClosed = vi.fn();
const mount = () => render(<><MatchModal event={event} tr={tr} onClose={() => {}} onClosed={onClosed} /><ConfirmHost tr={tr} /></>);

it("lists the matches and closes one only after the user confirms", async () => {
  ipc.match.mockResolvedValue([match]);
  ipc.close.mockResolvedValue(true);
  mount();
  fireEvent.click(await screen.findByText("Close listing"));
  expect(ipc.close).not.toHaveBeenCalled();
  fireEvent.click(await screen.findByText("Confirm"));
  expect(await screen.findByText("Closed")).toBeTruthy();
  expect(ipc.close).toHaveBeenCalledWith(match, "e1");
  expect(onClosed).toHaveBeenCalledTimes(1);
  expect(ipc.match.mock.calls[0][0]).toMatchObject({ partner: "Tenno", type: "sale", items: [{ displayName: "Serration", direction: "given" }] });
});

it("says so when nothing matches and shows the failure when the close is refused", async () => {
  ipc.match.mockResolvedValueOnce([]);
  const { unmount } = mount();
  expect(await screen.findByText(/No matching listing/)).toBeTruthy();
  unmount();
  ipc.match.mockResolvedValue([match]);
  ipc.close.mockResolvedValue(false);
  mount();
  fireEvent.click(await screen.findByText("Close listing"));
  fireEvent.click(await screen.findByText("Confirm"));
  expect(await screen.findByText(/Could not close it/)).toBeTruthy();
});
