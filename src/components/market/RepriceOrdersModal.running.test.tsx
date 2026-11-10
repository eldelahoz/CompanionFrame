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

test("a running Bulk sell load of other items does not make the modal busy", async () => {
  const orders = await twoOrders();
  const foreign = newJob(["lex_prime", "boltor_prime"]);
  open(orders);
  await waitFor(() => expect(ipc.jobsList).toHaveBeenCalled());
  expect(loadBtn().disabled).toBe(false);
  expect(screen.queryByRole("status")).toBeNull();
  expect((screen.getByLabelText("Strategy") as HTMLSelectElement).disabled).toBe(false);

  fireEvent.click(loadBtn());
  await waitFor(() => expect(ipc.jobsStartMarketLoad).toHaveBeenCalledTimes(1));
  expect(foreign.state).toBe("running");
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
