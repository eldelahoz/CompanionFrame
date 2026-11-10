// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import { resetMock, wfmOrderCreate, wfmOrdersMy, type MyOrders } from "../lib/ipc";
import { resetMyOrders } from "../stores/myOrders";
import { resetViewStates } from "../stores/viewState";
import { resetJobs } from "../stores/jobs";
import { useWfmSession } from "../stores/wfmSession";
import { usePresence } from "../lib/wfm/presence";
import ConfirmHost from "../components/shell/ConfirmHost";
import MarketView from "./MarketView";

// While set, the backend's cached copy answers with this and its refresh never lands.
let cached: { orders: MyOrders | null; refreshing: boolean } | null = null;
vi.mock("../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../lib/ipc")>()),
  wfmMyReputation: async () => null,
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
    </>
  );
}

beforeEach(() => {
  cached = null;
  resetMyOrders();
  resetMock();
  resetJobs();
  resetViewStates();
  localStorage.clear();
});
afterEach(cleanup);

const signIn = async () => {
  fireEvent.change(await screen.findByLabelText("Email"), { target: { value: "tenno@example.com" } });
  fireEvent.change(screen.getByLabelText("Password"), { target: { value: "pw" } });
  fireEvent.click(screen.getByText("Sign In"));
};
const rows = () => document.querySelectorAll("[data-market-order]").length;
const names = () => [...document.querySelectorAll("[data-market-order]")].map((r) => r.textContent ?? "");
const searchBox = () => screen.getByLabelText(tr("market.searchOrders")) as HTMLInputElement;
const type = (value: string) => fireEvent.change(searchBox(), { target: { value } });
const create = (itemId: string, orderType = "SELL", extra: object = {}) => wfmOrderCreate({ itemId, orderType, platinum: 50, quantity: 1, ...extra } as never);
const bar = () => within(document.querySelector<HTMLElement>("[data-market-bulk-bar]")!);
const open = async (count: number) => {
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(rows()).toBe(count));
};

test("search filters rows by name, ignoring case and accents", async () => {
  for (const id of ["mock_lex_prime", "mock_boltor_prime", "mock_serration"]) await create(id);
  await open(3);
  type("lex");
  expect(names()).toHaveLength(1);
  expect(names()[0]).toContain("Lex Prime");
  type("SÉRRATIÓN");
  expect(names()).toHaveLength(1);
  expect(names()[0]).toContain("Serration");
  type("PRIME");
  expect(rows()).toBe(2);
});

test("an accented display name matches without the accent, and the English catalog name matches too", async () => {
  await create("mock_boltor_prime");
  await create("mock_lex_prime");
  const live = await wfmOrdersMy();
  cached = { orders: { ...live, sell: live.sell.map((o) => (o.itemSlug === "boltor_prime" ? { ...o, itemName: "Ensemble Éclat" } : o)) }, refreshing: true };
  await open(2);
  type("eclat");
  expect(rows()).toBe(1);
  type("boltor"); // not in the display name: only the catalog's English name has it
  await waitFor(() => expect(rows()).toBe(1)); // the catalog loads after the rows
  expect(names()[0]).toContain("Ensemble Éclat");
});

test("with a search active, select all ticks only the matches", async () => {
  for (const id of ["mock_lex_prime", "mock_boltor_prime", "mock_serration"]) await create(id);
  await open(3);
  type("prime");
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  expect(screen.getByText("2 of 2 selected")).toBeTruthy();
  type("");
  const ticked = (screen.getAllByLabelText(/^Select /) as HTMLInputElement[]).filter((b) => b.checked);
  expect(ticked).toHaveLength(2);
  expect((screen.getByLabelText(/^Select Serration/) as HTMLInputElement).checked).toBe(false);
});

test("a selected order hidden by the search is not shown nor deleted by the bulk bar", async () => {
  for (const id of ["mock_lex_prime", "mock_boltor_prime", "mock_serration"]) await create(id, "SELL", { visible: false });
  await open(3);
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  type("lex");
  expect(screen.getByText("1 of 1 selected")).toBeTruthy();

  fireEvent.click(bar().getByRole("button", { name: "Show" }));
  await waitFor(async () => expect((await wfmOrdersMy()).sell.filter((o) => o.visible).map((o) => o.itemSlug)).toEqual(["lex_prime"]));

  fireEvent.click(bar().getByRole("button", { name: "Delete" }));
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(async () => expect((await wfmOrdersMy()).sell.map((o) => o.itemSlug).sort()).toEqual(["boltor_prime", "serration"]));
});

test("a selected order hidden by the search is not hidden nor sent to Reprice", async () => {
  for (const id of ["mock_lex_prime", "mock_serration"]) await create(id);
  await open(2);
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  type("lex");
  fireEvent.click(bar().getByRole("button", { name: "Hide" }));
  await waitFor(async () => expect((await wfmOrdersMy()).sell.filter((o) => !o.visible).map((o) => o.itemSlug)).toEqual(["lex_prime"]));
  fireEvent.click(bar().getByRole("button", { name: tr("market.repriceSelected") }));
  expect(document.querySelectorAll("[data-reprice-row]")).toHaveLength(1);
  expect(document.querySelector("[data-reprice-modal]")!.textContent).toContain("Lex Prime");
});

test("the whole-side hide button still acts on the whole side while a search is active", async () => {
  for (const id of ["mock_lex_prime", "mock_serration"]) await create(id);
  await open(2);
  type("lex");
  fireEvent.click(document.querySelector("[data-market-side-visibility]")!);
  await waitFor(async () => expect((await wfmOrdersMy()).sell.every((o) => !o.visible)).toBe(true));
});

test("no matches shows the empty state and keeps the search box", async () => {
  await create("mock_lex_prime");
  await open(1);
  type("zzz");
  expect(rows()).toBe(0);
  expect(screen.getByText(tr("market.noOrdersMatch", { query: "zzz" }))).toBeTruthy();
  expect(searchBox().value).toBe("zzz");
});

test("the search text survives Sell to Buy and clears on leaving Market", async () => {
  await create("mock_lex_prime");
  await create("mock_lex_prime", "BUY");
  render(<Harness />);
  await signIn();
  await waitFor(() => expect(rows()).toBe(1));
  type("lex");
  fireEvent.click(screen.getByText("Buy Orders"));
  await waitFor(() => expect(rows()).toBe(1));
  expect(searchBox().value).toBe("lex");
  fireEvent.click(screen.getByText("Sell Orders"));
  expect(searchBox().value).toBe("lex");
  cleanup();
  render(<Harness />); // still signed in
  await waitFor(() => expect(rows()).toBe(1));
  expect(searchBox().value).toBe("");
});

test("Update prices in the header sends only the shown orders while a search is active", async () => {
  for (const id of ["mock_lex_prime", "mock_serration"]) await create(id);
  await open(2);
  type("lex");
  fireEvent.click(document.querySelector("[data-market-reprice]")!);
  expect(document.querySelectorAll("[data-reprice-row]")).toHaveLength(1);
  expect(document.querySelector("[data-reprice-modal]")!.textContent).toContain("Lex Prime");
  expect(document.querySelector("[data-reprice-modal]")!.textContent).not.toContain("Serration");
});

// user-event is not installed: each keystroke is a change event, and focus is checked after every one.
test("the search input keeps focus going from matches to no matches and back", async () => {
  await create("mock_lex_prime");
  await open(1);
  searchBox().focus();
  for (const value of ["l", "le", "lex", "lexz", "lex", "le", "l", ""]) {
    type(value);
    expect(searchBox().value).toBe(value);
    expect(document.activeElement).toBe(searchBox());
  }
});

test("deleting the shown orders keeps the selection hidden by the search", async () => {
  for (const id of ["mock_lex_prime", "mock_boltor_prime", "mock_serration"]) await create(id);
  await open(3);
  fireEvent.click(screen.getByLabelText(tr("market.selectAllOrders")));
  type("lex");
  fireEvent.click(bar().getByRole("button", { name: "Delete" }));
  fireEvent.click(await screen.findByText("Confirm"));
  await waitFor(async () => expect((await wfmOrdersMy()).sell.map((o) => o.itemSlug).sort()).toEqual(["boltor_prime", "serration"]));
  type("");
  await waitFor(() => expect(rows()).toBe(2));
  expect(screen.getByText("2 of 2 selected")).toBeTruthy();
});
