// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { SendRepResult, TradeOverlayView_Serialize as View } from "../../bindings";
import { translator } from "../../lib/i18n";
import type { OverlayIpc } from "../common/overlayIpc";
import { TRADE_OVERLAY_EVENT, TradeOverlay, type TradeApi } from "./TradeOverlay";

afterEach(cleanup);

const view = (over: Partial<View> = {}, matches: View["current"] extends infer C ? (C extends { matches: infer M } ? M : never) : never = []): View => ({
  current: {
    trade: {
      id: "t1", date: "", type: "sale", platChange: 45, partner: "Kestrel", schemaVersion: 2, source: "live",
      items: [
        { internalName: "", displayName: "Serration", count: 2, direction: "given", wfmSlug: "serration", wfmThumb: undefined },
        { internalName: "", displayName: "Platinum", count: 45, direction: "received", wfmSlug: undefined, wfmThumb: undefined },
      ],
    },
    matches,
  },
  queued: 0, token: 7, autoCloseMs: 20000, ...over,
});
const match = { kind: "order", orderId: "o1", itemName: "Serration", itemUrlName: "serration", itemThumb: null, quantity: 2, platinum: 45, partner: "Kestrel", type: "sale", perTrade: null } as never;

function mount(closeMatch = async () => true, rep: { signedIn?: boolean; result?: SendRepResult } = {}) {
  let push: (p: View) => void = () => {};
  const ipc: OverlayIpc = {
    settings: async () => ({}) as never,
    placement: async () => [],
    layout: { get: async () => null, save: async () => {}, reset: async () => {} },
    on: (e, cb) => {
      if (e === TRADE_OVERLAY_EVENT) push = cb as typeof push;
      return () => {};
    },
  };
  const api = { dismiss: vi.fn(async () => null), closeMatch: vi.fn(closeMatch), setRegions: vi.fn(async () => null), signedIn: vi.fn(async () => rep.signedIn ?? true), plusRep: vi.fn(async () => rep.result ?? "sent") } satisfies TradeApi;
  const { container } = render(<TradeOverlay tr={translator("en")} ipc={ipc} interactive={false} api={api} />);
  return { api, container, emit: (p: View) => act(async () => push(p)) };
}

it("is blank and click-through until a trade arrives", async () => {
  const { container, api, emit } = mount();
  expect(container.textContent).toBe("");
  await emit(view({ current: null }));
  expect(container.textContent).toBe("");
  expect(api.setRegions).toHaveBeenLastCalledWith([]);
});

it("shows the trade, makes its box clickable and counts the queue", async () => {
  const { api, emit } = mount();
  await emit(view({ queued: 2 }, [match]));
  expect(screen.getByText("− 2x Serration")).toBeTruthy();
  expect(screen.getByText("+ 45x Platinum")).toBeTruthy();
  expect(screen.getByText(/Kestrel/)).toBeTruthy();
  expect(screen.getByText("+2 more")).toBeTruthy();
  expect(api.setRegions).toHaveBeenLastCalledWith([expect.objectContaining({ x: expect.any(Number), w: expect.any(Number) })]);
});

it("closes a matched listing quoting the trade's token", async () => {
  const { api, emit } = mount();
  await emit(view({}, [match]));
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Close listing" })));
  expect(api.closeMatch).toHaveBeenCalledWith(7, "o1");
  expect(screen.queryByRole("alert")).toBeNull();
});

it("keeps the popup and says so when WFM refuses the close", async () => {
  const { emit } = mount(async () => false);
  await emit(view({}, [match]));
  await act(async () => fireEvent.click(screen.getByText(/Close listing/)));
  expect(screen.getByRole("alert").textContent).toBe("Closing Failed");
});

it("says no listing matched and dismisses with the token", async () => {
  const { api, emit } = mount();
  await emit(view());
  expect(screen.getByText("No Listing Matched")).toBeTruthy();
  await act(async () => fireEvent.click(screen.getByText("Dismiss")));
  expect(api.dismiss).toHaveBeenCalledWith(7);
});

it("sends +rep to the partner only on click and says it was sent", async () => {
  const { api, emit } = mount();
  await emit(view());
  expect(api.plusRep).not.toHaveBeenCalled();
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "+rep" })));
  expect(api.plusRep).toHaveBeenCalledWith("Kestrel");
  expect(screen.getByRole("status").textContent).toBe("Rep sent");
  expect(screen.queryByRole("button", { name: "+rep" })).toBeNull();
});

it.each([
  ["already-exists", "Rep already given", false],
  ["user-not-found", "User not found", false],
  ["failed", "Rep failed", true],
] as const)("reports %s and %s retry", async (result, text, retry) => {
  const { emit } = mount(undefined, { result });
  await emit(view());
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "+rep" })));
  expect(screen.getByRole("status").textContent).toBe(text);
  expect(!!screen.queryByRole("button", { name: "+rep" })).toBe(retry);
});

it("hides +rep when signed out or the partner is unknown", async () => {
  const out = mount(undefined, { signedIn: false });
  await out.emit(view());
  expect(screen.queryByRole("button", { name: "+rep" })).toBeNull();
  cleanup();
  const anon = mount();
  const v = view();
  v.current!.trade.partner = undefined;
  await anon.emit(v);
  expect(screen.queryByRole("button", { name: "+rep" })).toBeNull();
});
