// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import * as ipc from "../../lib/ipc";
import { resetMock, wfmOrderCreate, wfmOrdersMy, wfmSignIn, type Job, type JobUpdate, type MyOrder } from "../../lib/ipc";
import { mockBook } from "../../lib/wfm/mockMarket";
import { resetJobs } from "../../stores/jobs";
import RepriceOrdersModal from "./RepriceOrdersModal";

// The test owns the jobs: it lists them, emits their updates and decides which books the backend "has".
let live: Job[] = [];
let listeners: ((u: JobUpdate) => void)[] = [];
let stored = new Set<string>();
// Overrides the book a slug reads back, to tell a fresh read from a cached one.
let bumped = 0;
let seq = 0;
const newJob = (slugs: string[], state: Job["state"] = "running"): Job => {
  const job = { id: `ml-${++seq}`, kind: { type: "marketLoad" }, state, total: slugs.length, done: 0, failed: 0, createdAt: Date.now(), finishedAt: null, items: slugs.map((id) => ({ id, name: id, platinum: null, status: "pending", reason: null })) } as Job;
  live.push(job);
  return job;
};
const emit = (u: JobUpdate) => listeners.forEach((l) => l(u));
vi.mock("../../lib/ipc", async (orig) => {
  const actual = await orig<typeof import("../../lib/ipc")>();
  return {
    ...actual,
    jobsList: vi.fn(async () => live),
    onJobUpdated: async (cb: (u: JobUpdate) => void) => {
      listeners.push(cb);
      return () => void (listeners = listeners.filter((l) => l !== cb));
    },
    jobsDismiss: vi.fn(async () => {}),
    jobsMarketBooks: vi.fn(async (slugs: string[]) => slugs.filter((s) => stored.has(s)).map((slug) => ({ slug, orders: mockBook(slug).map((o) => ({ ...o, platinum: o.platinum + bumped })) }))),
    jobsStartMarketLoad: vi.fn(async (rows: { slug: string; name: string }[]) => {
      // Like the backend: one market load at a time.
      if (live.some((j) => j.state === "running")) throw new Error("Another job is still running.");
      const job = newJob(rows.map((r) => r.slug));
      emit({ id: job.id, state: job.state, total: job.total, done: 0, failed: 0, item: null, job });
      return job.id;
    }),
  };
});

const tr = translator("en");
const NO_BOOK = "No market data";
// Mirrors the backend: the book is stored first, then the item is reported as done.
const finish = (job: Job, slug: string) => {
  stored.add(slug);
  job.items.find((i) => i.id === slug)!.status = "done";
  job.done++;
  emit({ id: job.id, state: "running", total: job.total, done: job.done, failed: 0, item: { id: slug, status: "done", reason: null }, job: null });
};
const end = (job: Job) => {
  job.state = "done";
  emit({ id: job.id, state: "done", total: job.total, done: job.done, failed: job.failed, item: null, job: null });
};

beforeEach(() => {
  live = [];
  listeners = [];
  stored = new Set();
  bumped = 0;
  vi.mocked(ipc.jobsDismiss).mockClear();
  vi.mocked(ipc.jobsStartMarketLoad).mockClear();
  resetMock();
  resetJobs();
});
afterEach(cleanup);

const sell = (price: number, itemId: string) => wfmOrderCreate({ itemId, orderType: "SELL", platinum: price, quantity: 1 } as never);
const ordersOf = async (): Promise<MyOrder[]> => (await wfmOrdersMy()).sell;
const open = (orders: MyOrder[]) => render(<RepriceOrdersModal orders={orders} ownUserName={null} tr={tr} onClose={() => {}} />);
const nextOf = (label: string) => screen.getByText(label).closest("tr")!.querySelector("td:last-child")!.textContent;
const loadBtn = () => screen.getByText("Load prices") as HTMLButtonElement;

async function twoOrders() {
  await wfmSignIn("tenno@example.com", "pw");
  await sell(5, "mock_serration");
  await sell(5, "mock_vitality");
  return ordersOf();
}

test("reopening while the load is still running: the reads that finish afterwards reach the rows", async () => {
  const orders = await twoOrders();
  open(orders);
  fireEvent.click(loadBtn());
  await waitFor(() => expect(live).toHaveLength(1));
  const job = live[0];
  finish(job, "serration");
  cleanup();

  open(orders); // the job is still running
  await waitFor(() => expect(nextOf("Serration")).not.toContain(NO_BOOK));
  expect(nextOf("Vitality")).toContain(NO_BOOK);
  expect(loadBtn().disabled).toBe(true);

  finish(job, "vitality");
  await waitFor(() => expect(nextOf("Vitality")).not.toContain(NO_BOOK));
  end(job);
  await waitFor(() => expect(loadBtn().disabled).toBe(false));
  expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(1);
});

test("a running Bulk sell load is not adopted: Load prices is disabled, the modal is not busy and Apply stays enabled", async () => {
  const orders = await twoOrders();
  const foreign = newJob(["serration", "vitality", "boltor_prime"]); // even one covering every row
  open(orders);
  await waitFor(() => expect(loadBtn().disabled).toBe(true));
  expect(loadBtn().title).toBe("A price load is already running");
  expect(screen.queryByRole("status")).toBeNull();
  expect((screen.getByLabelText("Strategy") as HTMLSelectElement).disabled).toBe(false);
  expect((document.querySelector("[data-reprice-apply]") as HTMLButtonElement).disabled).toBe(true); // no books yet: nothing to send, but not because of the load

  fireEvent.click(loadBtn());
  expect(ipc.jobsStartMarketLoad).not.toHaveBeenCalled();

  // Once the foreign load ends the button is back, and the rows were never fed by it.
  end(foreign);
  await waitFor(() => expect(loadBtn().disabled).toBe(false));
  expect(nextOf("Serration")).toContain(NO_BOOK);
});

test("with a Bulk sell load running, Apply is still enabled for the rows that have a book", async () => {
  const orders = await twoOrders();
  stored.add("serration");
  newJob(["lex_prime"]);
  open(orders);
  await waitFor(() => expect(nextOf("Serration")).not.toContain(NO_BOOK));
  expect(loadBtn().disabled).toBe(true);
  expect((document.querySelector("[data-reprice-apply]") as HTMLButtonElement).disabled).toBe(false);
});

test("the mock rejects a second market load while one runs, like the backend", async () => {
  newJob(["lex_prime"]);
  await expect(ipc.jobsStartMarketLoad([{ slug: "serration", name: "Serration" }])).rejects.toThrow("Another job is still running.");
});

test("partial reopen: 4 of 6 rows loading, close mid-load, reopen: the loaded rows get their books and Load prices is not rejected", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  const slugs = ["serration", "vitality", "ash_prime_set", "boltor_prime", "lex_prime", "point_strike"];
  for (const s of slugs) await sell(5, s === "ash_prime_set" ? "mock_ash_set" : `mock_${s}`);
  const orders = await ordersOf();
  const label = (s: string) => orders.find((o) => o.itemId === (s === "ash_prime_set" ? "mock_ash_set" : `mock_${s}`))?.itemName ?? s;
  const open10 = () => open(orders);

  open10();
  const unticked = [slugs[4], slugs[5]];
  for (const s of unticked) fireEvent.click(screen.getByLabelText(label(s)));
  fireEvent.click(loadBtn());
  await waitFor(() => expect(live).toHaveLength(1));
  const job = live[0];
  expect(job.items.map((i) => i.id)).toEqual(slugs.slice(0, 4));
  finish(job, slugs[0]);
  cleanup();

  open10(); // reopened mid-load, without the unticked state
  await waitFor(() => expect(nextOf(label(slugs[0]))).not.toContain(NO_BOOK));
  expect(loadBtn().disabled).toBe(true);
  for (const s of slugs.slice(1, 4)) finish(job, s);
  for (const s of slugs.slice(1, 4)) await waitFor(() => expect(nextOf(label(s))).not.toContain(NO_BOOK));
  expect(nextOf(label(slugs[4]))).toContain(NO_BOOK);
  end(job);
  await waitFor(() => expect(loadBtn().disabled).toBe(false));

  // The next load asks only for the two rows still missing and the backend accepts it.
  fireEvent.click(loadBtn());
  await waitFor(() => expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(2));
  expect(vi.mocked(ipc.jobsStartMarketLoad).mock.calls[1][0].map((r) => r.slug)).toEqual(unticked);
});

test("a failed refresh shows the row's 'Not refreshed' hint and keeps the cached price", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  await sell(5, "mock_serration");
  const orders = await ordersOf();
  open(orders);
  fireEvent.change(screen.getByLabelText("Strategy"), { target: { value: "match-cheapest" } });
  fireEvent.click(loadBtn());
  await waitFor(() => expect(live).toHaveLength(1));
  finish(live[0], "serration");
  end(live[0]);
  await waitFor(() => expect(nextOf("Serration")).not.toContain(NO_BOOK));
  const before = nextOf("Serration");
  expect(screen.queryByText("Not refreshed")).toBeNull();

  await waitFor(() => expect(loadBtn().disabled).toBe(false));
  fireEvent.click(loadBtn());
  await waitFor(() => expect(live.filter((j) => j.state === "running")).toHaveLength(1));
  const second = live.find((j) => j.state === "running")!;
  second.items[0].status = "failed";
  second.items[0].reason = "HTTP 503";
  second.failed++;
  second.state = "done";
  emit({ id: second.id, state: "done", total: second.total, done: 0, failed: 1, item: { id: "serration", status: "failed", reason: "HTTP 503" }, job: null });
  await waitFor(() => expect(screen.getByText("Not refreshed")).toBeTruthy());
  expect(nextOf("Serration")).toContain(before!.replace("Not refreshed", ""));
});

test("Load prices does not dismiss a finished Bulk sell load with failures", async () => {
  const orders = await twoOrders();
  const foreign = newJob(["lex_prime"], "done");
  foreign.failed = 1;
  open(orders);
  await waitFor(() => expect(ipc.jobsList).toHaveBeenCalled());
  fireEvent.click(loadBtn());
  await waitFor(() => expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(1));
  expect(ipc.jobsDismiss).not.toHaveBeenCalledWith(foreign.id);
});

test("Load prices with every row already booked reads the selected rows again and keeps the cached price until it lands", async () => {
  await wfmSignIn("tenno@example.com", "pw");
  await sell(5, "mock_serration");
  const orders = await ordersOf();
  open(orders);
  fireEvent.change(screen.getByLabelText("Strategy"), { target: { value: "match-cheapest" } });
  fireEvent.click(loadBtn());
  await waitFor(() => expect(live).toHaveLength(1));
  finish(live[0], "serration");
  end(live[0]);
  await waitFor(() => expect(nextOf("Serration")).not.toContain(NO_BOOK));
  const before = parseInt(nextOf("Serration")!, 10);

  bumped = 30; // the market moved since the first read
  await waitFor(() => expect(loadBtn().disabled).toBe(false));
  fireEvent.click(loadBtn());
  await waitFor(() => expect(live.filter((j) => j.state === "running")).toHaveLength(1));
  expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(2);
  expect(vi.mocked(ipc.jobsStartMarketLoad).mock.calls[1][0].map((r) => r.slug)).toEqual(["serration"]);
  expect(parseInt(nextOf("Serration")!, 10)).toBe(before); // cached book stays while the read runs

  const second = live.find((j) => j.state === "running")!;
  finish(second, "serration");
  end(second);
  await waitFor(() => expect(parseInt(nextOf("Serration")!, 10)).toBeGreaterThan(before)); // damping caps the step, so only the direction is checked
});
