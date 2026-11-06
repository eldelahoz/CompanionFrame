// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { translator } from "../../lib/i18n";
import type { Job, JobUpdate } from "../../lib/ipc";
import { OK_VISIBLE_MS, resetJobs } from "../../stores/jobs";
import JobsPill from "./JobsPill";

// The backend is the test: it feeds the store through `send` and answers the snapshot with `snapshot`.
let send: (u: JobUpdate) => void = () => {};
let snapshot: Job[] = [];
const actions = { stop: vi.fn(), retry: vi.fn(), cont: vi.fn(), dismiss: vi.fn() };
vi.mock("../../lib/ipc", async (orig) => ({
  ...(await orig<typeof import("../../lib/ipc")>()),
  jobsList: async () => snapshot,
  onJobUpdated: async (cb: (u: JobUpdate) => void) => {
    send = cb;
    return () => {};
  },
  jobsStop: (id: string) => Promise.resolve(actions.stop(id)),
  jobsRetryFailed: (id: string) => Promise.resolve(actions.retry(id)),
  jobsContinue: (id: string) => Promise.resolve(actions.cont(id)),
  jobsDismiss: (id: string) => Promise.resolve(actions.dismiss(id)),
}));

const tr = translator("en");
const item = (id: string, status: Job["items"][number]["status"], reason: string | null = null) => ({ id, name: `Item ${id}`, platinum: null, status, reason });
const job = (over: Partial<Job> = {}): Job => ({
  id: "j1",
  kind: { type: "ordersVisibility", visible: false },
  state: "running",
  total: 4,
  done: 0,
  failed: 0,
  items: ["a", "b", "c", "d"].map((i) => item(i, "pending")),
  createdAt: 1,
  finishedAt: null,
  ...over,
});
const update = (j: Job, over: Partial<JobUpdate> = {}): JobUpdate => ({ id: j.id, state: j.state, total: j.total, done: j.done, failed: j.failed, item: null, job: null, ...over });
const open = () => fireEvent.click(screen.getByRole("button", { name: /Background jobs/ }));

beforeEach(() => {
  snapshot = [];
  Object.values(actions).forEach((a) => a.mockReset());
  resetJobs();
  vi.useFakeTimers({ shouldAdvanceTime: true });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

test("shows nothing while there is no job", async () => {
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  expect(screen.queryByRole("button")).toBeNull();
});

test("a running job shows its label and count, and the panel has a progress bar and Stop", async () => {
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  const j = job();
  act(() => send(update(j, { job: j })));
  const running = job({ done: 1 });
  act(() => send(update(running, { item: { id: "a", status: "done", reason: null } })));
  expect(screen.getByRole("button", { name: /Hiding 1 of 4/ })).toBeTruthy();

  open();
  const dialog = screen.getByRole("dialog", { name: "Background jobs" });
  expect(dialog.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("25");
  fireEvent.click(screen.getByRole("button", { name: "Stop" }));
  expect(actions.stop).toHaveBeenCalledWith("j1");
});

test("a stopping job cannot be stopped twice", async () => {
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  const j = job({ state: "stopping" });
  act(() => send(update(j, { job: j })));
  open();
  expect((screen.getByRole("button", { name: "Stopping..." }) as HTMLButtonElement).disabled).toBe(true);
});

test("a finished job with failures lists the reasons and retries only the failed ones", async () => {
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  const j = job({ state: "done", done: 2, failed: 2, items: [item("a", "done"), item("b", "failed", "order not found"), item("c", "done"), item("d", "failed", "HTTP 503")] });
  act(() => send(update(j, { job: j })));
  expect(screen.getByRole("button", { name: /2 done, 2 failed/ })).toBeTruthy();
  open();
  expect(screen.getByText("Item b: order not found")).toBeTruthy();
  expect(screen.getByText("Item d: HTTP 503")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Retry the 2" }));
  expect(actions.retry).toHaveBeenCalledWith("j1");
  fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
  expect(actions.dismiss).toHaveBeenCalledWith("j1");
});

test("an interrupted job says nothing resumes by itself and offers Continue", async () => {
  snapshot = [job({ state: "interrupted", done: 2, items: [item("a", "done"), item("b", "done"), item("c", "pending"), item("d", "pending")] })];
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  expect(screen.getByRole("button", { name: /Interrupted: 2 of 4/ })).toBeTruthy();
  open();
  expect(screen.getByText("2 left. Nothing resumes by itself.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Continue with the 2" }));
  expect(actions.cont).toHaveBeenCalledWith("j1");
});

test("a clean finish shows a check line that goes away after its timeout", async () => {
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  const j = job();
  act(() => send(update(j, { job: j })));
  const done = job({ state: "done", done: 4 });
  act(() => send(update(done)));
  expect(screen.getByRole("button", { name: /4 done/ })).toBeTruthy();
  act(() => void vi.advanceTimersByTime(OK_VISIBLE_MS + 10));
  expect(screen.queryByRole("button")).toBeNull();
});

test("a background job shows muted, without a spinner, when no user job runs; a running user job takes over the pill", async () => {
  render(<JobsPill tr={tr} />);
  await act(async () => {});
  const prices = job({ id: "p", kind: { type: "pricesLoad" }, total: 1493, done: 585, items: [] });
  act(() => send(update(prices, { job: prices })));
  const pill = screen.getByRole("button", { name: /Prices 585 of 1493/ });
  expect(pill.getAttribute("data-jobs-pill")).toBe("background");
  expect(pill.querySelector("svg")).toBeNull();

  const hide = job();
  act(() => send(update(hide, { job: hide })));
  expect(screen.getByRole("button", { name: /Hiding 0 of 4/ }).getAttribute("data-jobs-pill")).toBe("running");
  open();
  // The panel still lists the load, with progress and no Stop of its own.
  expect(screen.getByText("585 of 1493")).toBeTruthy();
  expect(screen.getAllByRole("button", { name: "Stop" })).toHaveLength(1);

  // When the background job ends nothing is left of it.
  act(() => send(update(job({ id: "p", kind: { type: "pricesLoad" }, state: "done", total: 1493, done: 1493, items: [] }))));
  act(() => void vi.advanceTimersByTime(10));
  expect(screen.queryByText("585 of 1493")).toBeNull();
});
