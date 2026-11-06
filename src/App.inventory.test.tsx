// @vitest-environment jsdom
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import App from "./App";
import { resetMock } from "./lib/ipc";
import { resetInventoryStore } from "./stores/inventory";

const calls = vi.hoisted(() => ({ get: 0 }));
vi.mock("./lib/ipc", async (orig) => {
  const real = await orig<typeof import("./lib/ipc")>();
  return {
    ...real,
    getInventory: (...a: Parameters<typeof real.getInventory>) => (calls.get++, real.getInventory(...a)),
  };
});

afterEach(cleanup);

test("the main window reads the inventory once, whoever asks", async () => {
  resetMock();
  resetInventoryStore();
  localStorage.clear();
  render(<App />);
  await waitFor(() => expect(document.getElementById("content")).not.toBeNull());
  await waitFor(() => expect(calls.get).toBeGreaterThan(0));
  expect(calls.get).toBe(1);
});
