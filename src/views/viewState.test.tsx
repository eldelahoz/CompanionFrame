// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../lib/i18n";
import { resetMock, wfmOrderCreate, wfmOrderDelete, wfmOrdersMy } from "../lib/ipc";
import { usePresence } from "../lib/wfm/presence";
import { resetMyOrders } from "../stores/myOrders";
import { useWfmSession } from "../stores/wfmSession";
import MarketAnalysisView from "./MarketAnalysisView";
import MarketView from "./MarketView";
import MasteryView from "./MasteryView";
import WikiView from "./WikiView";

// Each view unmounts on a switch; these check that coming back finds it as it was left (state resets between tests in test-setup).
const tr = translator("en");

beforeEach(() => {
  localStorage.clear();
  resetMyOrders();
  resetMock();
});
afterEach(cleanup);

test("Wiki keeps its query and mode, and searches again on return", async () => {
  const first = render(<WikiView tr={tr} />);
  fireEvent.click(document.querySelector("[data-wiki-mode=place]")!);
  fireEvent.change(screen.getByPlaceholderText(tr("wiki.searchPlaceholderPlace")), { target: { value: "zzzz" } });
  first.unmount();
  render(<WikiView tr={tr} />);
  expect((screen.getByPlaceholderText(tr("wiki.searchPlaceholderPlace")) as HTMLInputElement).value).toBe("zzzz");
  await waitFor(() => expect(screen.getByText(tr("wiki.noResults", { query: "zzzz" }))).toBeTruthy());
});

test("Mastery keeps its search", async () => {
  const first = render(<MasteryView tr={tr} locale="en" />);
  fireEvent.change(await screen.findByLabelText(tr("common.searchPlaceholder")), { target: { value: "volt" } });
  first.unmount();
  render(<MasteryView tr={tr} locale="en" />);
  expect(((await screen.findByLabelText(tr("common.searchPlaceholder"))) as HTMLInputElement).value).toBe("volt");
});

test("Market Analysis keeps its type filter and search, and the ledger is filtered on return", async () => {
  const rows = () => document.querySelectorAll("[data-analysis-table] tbody tr");
  const first = render(<MarketAnalysisView tr={tr} locale="en" items={{}} />);
  fireEvent.click(document.querySelector("[data-analysis-range-preset=all] button") as HTMLElement);
  await waitFor(() => expect(rows()).toHaveLength(36));
  fireEvent.change(screen.getByLabelText(tr("common.type")), { target: { value: "purchase" } });
  await waitFor(() => expect(rows()).toHaveLength(9));
  first.unmount();
  render(<MarketAnalysisView tr={tr} locale="en" items={{}} />);
  await waitFor(() => expect(rows()).toHaveLength(9));
  expect((screen.getByLabelText(tr("common.type")) as HTMLSelectElement).value).toBe("purchase");
});

function Market() {
  const wfm = useWfmSession();
  const presence = usePresence(wfm.session?.logged_in === true);
  return <MarketView tr={tr} items={{}} session={wfm.session} signIn={wfm.signIn} signOut={wfm.signOut} presence={presence} />;
}

test("Market keeps the selected orders, and drops the ones deleted meanwhile", async () => {
  for (const platinum of [5, 6]) await wfmOrderCreate({ itemId: "mock_serration", orderType: "SELL", platinum, quantity: 1 } as never);
  const first = render(<Market />);
  fireEvent.change(await screen.findByLabelText("Email"), { target: { value: "tenno@example.com" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "hunter2" } });
  fireEvent.click(screen.getByText("Sign In"));
  await waitFor(() => expect(document.querySelectorAll("[data-market-order]")).toHaveLength(2));
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  expect(screen.getByText("2 of 2 selected")).toBeTruthy();
  first.unmount();

  await wfmOrderDelete((await wfmOrdersMy()).sell[0].id);
  resetMyOrders();
  render(<Market />);
  expect(await screen.findByText("1 of 1 selected")).toBeTruthy();
});
