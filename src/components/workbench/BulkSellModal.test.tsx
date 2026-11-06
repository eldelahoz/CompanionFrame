// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import * as ipc from "../../lib/ipc";
import { resetMock, wfmSignIn, type CatalogItem } from "../../lib/ipc";
import { resetJobs } from "../../stores/jobs";
import { writeCachedQueueRows } from "../../lib/tradeWorkbench/queueCache";
import { selectionKeyFor } from "../../lib/tradeWorkbench/queueModel";
import { buildCatalogIndex } from "../../lib/wfm/catalog";
import type { ParsedItem } from "../../types/inventory";
import ConfirmHost from "../shell/ConfirmHost";
import BulkSellModal from "./BulkSellModal";

vi.mock("../../lib/ipc", async (orig) => {
  const actual = await orig<typeof import("../../lib/ipc")>();
  return { ...actual, wfmOrdersMy: vi.fn(actual.wfmOrdersMy), getMasteryProgress: vi.fn(actual.getMasteryProgress), jobsStartMarketLoad: vi.fn(actual.jobsStartMarketLoad) };
});

const tr = translator("en");

const item = (name: string, amount: number): ParsedItem =>
  ({
    name,
    displayName: name,
    internalName: `/Lotus/Test/${name.replace(/\s+/g, "")}`,
    inventoryKey: name,
    category: "Misc",
    categoryLabel: "Misc",
    rank: 0,
    maxRank: 0,
    imageUrl: null,
    isPrime: false,
    masteryReq: 0,
    vaulted: false,
    tradable: true,
    description: "",
    components: [],
    drops: [],
    wikiaUrl: null,
    amount,
    inventoryGroup: "all_parts",
  }) as unknown as ParsedItem;

const cat = (name: string, slug: string): CatalogItem => ({ id: slug, slug, name, thumb: null, icon: null, maxRank: null, gameRef: null });

const ITEMS = [item("Lex Prime Barrel", 4), item("Boltor Prime Receiver", 2), item("Unlisted Thing", 3)];
const catalog = buildCatalogIndex([cat("Lex Prime Barrel", "lex_prime_barrel"), cat("Boltor Prime Receiver", "boltor_prime_receiver")]);

const open = (items: ParsedItem[] = ITEMS) =>
  render(
    <>
      <BulkSellModal items={items} selection={new Set(items.map(selectionKeyFor))} inventory={null} itemDb={{}} catalog={catalog} locale="en" tr={tr} onClose={() => {}} />
      <ConfirmHost tr={tr} />
    </>,
  );

beforeEach(() => {
  vi.mocked(ipc.wfmOrdersMy).mockReset();
  vi.mocked(ipc.getMasteryProgress).mockReset();
  vi.mocked(ipc.jobsStartMarketLoad).mockClear();
  resetMock();
  resetJobs();
  writeCachedQueueRows([]);
  localStorage.clear();
});
afterEach(cleanup);

test("signed out: the queue shows catalog items only, with a sign-in hint and Execute disabled", async () => {
  open();
  expect(await screen.findByText(tr("workbench.signInHint"), { selector: "[data-workbench-signin-hint] span" })).toBeTruthy();
  const queue = await screen.findByText("Lex Prime Barrel", { selector: "span" }).then(() => document.querySelector("[data-workbench-queue]")!);
  expect(within(queue as HTMLElement).queryByText("Unlisted Thing")).toBeNull(); // no catalog slug, never sellable
  expect(within(queue as HTMLElement).getAllByText(/Prime/)).toHaveLength(2);
  expect((document.querySelector("[data-workbench-execute]") as HTMLButtonElement).disabled).toBe(true);
});

test("nothing sellable shows the empty state", async () => {
  open([item("Unlisted Thing", 3)]);
  expect(await screen.findByText(tr("workbench.queueEmpty"))).toBeTruthy();
});

test("signed in: load prices, tick a row, preview, then execute after the confirmation", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  open();
  await screen.findByText("Lex Prime Barrel", { selector: "span" });
  // Execute stays blocked until a row is ticked and priced.
  const execute = document.querySelector("[data-workbench-execute]") as HTMLButtonElement;
  expect(execute.disabled).toBe(true);

  fireEvent.click(document.querySelector("[data-workbench-select-all]")!);
  fireEvent.click(document.querySelector("[data-workbench-load-market]")!);
  await waitFor(() => expect(document.querySelector("[data-workbench-price-progress='2/2']")).toBeTruthy(), { timeout: 5000 });
  fireEvent.click(document.querySelector("[data-workbench-apply-strategy]")!);
  await waitFor(() => expect(execute.disabled).toBe(false));

  fireEvent.click(document.querySelector("[data-workbench-preview-btn]")!);
  expect(await screen.findByText(/./, { selector: "[data-workbench-preview-summary='ok']" })).toBeTruthy();

  fireEvent.click(execute);
  fireEvent.click(await screen.findByText(tr("common.confirm")));
  const progress = await waitFor(() => {
    const el = document.querySelector("[data-workbench-run-state='finished']");
    expect(el).toBeTruthy();
    return el!;
  });
  expect(progress.textContent).toContain("Lex Prime Barrel");
  expect(progress.textContent).toContain(tr("workbench.stopReason.completed"));
});

test("declining the confirmation sends nothing", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  open();
  await screen.findByText("Lex Prime Barrel", { selector: "span" });
  fireEvent.click(document.querySelector("[data-workbench-select-all]")!);
  // Every ticked row needs a price before Execute opens.
  document.querySelectorAll<HTMLInputElement>("[data-workbench-price]").forEach((price) => fireEvent.blur(price, { target: { value: "25" } }));
  const execute = document.querySelector("[data-workbench-execute]") as HTMLButtonElement;
  await waitFor(() => expect(execute.disabled).toBe(false));
  fireEvent.click(execute);
  fireEvent.click(await screen.findByText(tr("common.cancel")));
  expect(document.querySelector("[data-workbench-progress]")).toBeNull();
});

test("signed out: a neutral sign-in notice, never the orders error, even while the session loads", async () => {
  open();
  expect(document.querySelector("[data-workbench-orders-error]")).toBeNull();
  expect(await screen.findByText(tr("workbench.signIn"))).toBeTruthy();
  expect(document.querySelector("[data-workbench-orders-error]")).toBeNull();
});

test("signed in with a failing orders call: the error shows with Retry and stays", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  vi.mocked(ipc.wfmOrdersMy).mockRejectedValue(new Error("network down"));
  open();
  expect(await screen.findByText(tr("workbench.ordersFetchFailed"))).toBeTruthy();
  expect(screen.getByText(tr("common.retry"))).toBeTruthy();
  await new Promise((r) => setTimeout(r, 300));
  expect(document.querySelector("[data-workbench-orders-error]")).toBeTruthy();
  expect(document.querySelector("[data-workbench-signin-hint]")).toBeNull();
});

test("degraded protection rules are named in words, not ids, and stay put", async () => {
  vi.mocked(ipc.getMasteryProgress).mockResolvedValue(null);
  open();
  const note = await waitFor(() => {
    const el = document.querySelector("[data-workbench-degraded]");
    expect(el).toBeTruthy();
    return el!;
  });
  expect(note.textContent).toContain(tr("workbench.safety.degraded.unmasteredRecipe"));
  expect(note.textContent).not.toContain("unmasteredRecipe");
  await new Promise((r) => setTimeout(r, 300));
  expect(document.querySelector("[data-workbench-degraded]")).toBeTruthy();
});

test("a mod and an arcane keep one copy at item level, so two copies queue one", async () => {
  const upgrade = (name: string, group: "mods" | "arcanes", rank: number, amount: number) => ({ ...item(name, amount), inventoryGroup: group, rank, inventoryKey: `${name}#r${rank}` }) as ParsedItem;
  const items = [upgrade("Hornet Strike", "mods", 0, 2), upgrade("Arcane Energize", "arcanes", 0, 2), upgrade("Arcane Energize", "arcanes", 3, 1)];
  const upgradeCatalog = buildCatalogIndex([cat("Hornet Strike", "hornet_strike"), cat("Arcane Energize", "arcane_energize")]);
  render(<BulkSellModal items={items} selection={new Set(items.slice(0, 2).map(selectionKeyFor))} inventory={null} itemDb={{}} catalog={upgradeCatalog} locale="en" tr={tr} onClose={() => {}} />);
  await screen.findByText("Hornet Strike", { selector: "span" });
  // The R3 arcane is not picked but still holds the kept copy, so the whole R0 stack of two sells.
  const qty = [...document.querySelectorAll<HTMLInputElement>("[data-workbench-qty]")].map((i) => i.value);
  expect(qty).toEqual(["1", "2"]);
});

test("shift-click on a queue checkbox ticks the rows between two clicks", async () => {
  open();
  await screen.findByText("Lex Prime Barrel", { selector: "span" });
  const boxes = () => [...document.querySelectorAll<HTMLInputElement>("[data-workbench-select]")];
  expect(boxes().length).toBeGreaterThan(1);
  fireEvent.click(boxes()[0]);
  fireEvent.click(boxes()[boxes().length - 1], { shiftKey: true });
  expect(new Set(boxes().map((b) => b.checked)).size).toBe(1); // the range takes one state
});

test("the load button starts one market-load job with the selected slugs, once each", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  open();
  await screen.findByText("Lex Prime Barrel", { selector: "span" });
  fireEvent.click(document.querySelector("[data-workbench-select-all]")!);
  fireEvent.click(document.querySelector("[data-workbench-load-market]")!);
  await waitFor(() => expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(1));
  const rows = vi.mocked(ipc.jobsStartMarketLoad).mock.calls[0][0];
  expect(rows.map((r) => r.slug).sort()).toEqual(["boltor_prime_receiver", "lex_prime_barrel"]);
});

test("closing the modal does not stop the load, and reopening applies the books that landed meanwhile", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  open();
  await screen.findByText("Lex Prime Barrel", { selector: "span" });
  fireEvent.click(document.querySelector("[data-workbench-select-all]")!);
  fireEvent.click(document.querySelector("[data-workbench-load-market]")!);
  await waitFor(() => expect(ipc.jobsStartMarketLoad).toHaveBeenCalled());
  cleanup();
  // The job went on without the modal.
  await waitFor(async () => expect(await ipc.jobsMarketBooks(["lex_prime_barrel", "boltor_prime_receiver"])).toHaveLength(2), { timeout: 5000 });

  open();
  await screen.findByText("Lex Prime Barrel", { selector: "span" });
  await waitFor(() => {
    const prices = [...document.querySelectorAll<HTMLInputElement>("[data-workbench-price]")];
    expect(prices.length).toBeGreaterThan(0);
    expect(prices.every((p) => p.placeholder !== "")).toBe(true);
  });
  expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(1);
});
