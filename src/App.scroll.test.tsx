// @vitest-environment jsdom
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import App from "./App";
import { resetMock } from "./lib/ipc";

afterEach(cleanup);

test("a view starts at the top and gets back its own scroll offset, never another view's", async () => {
  resetMock();
  localStorage.clear();
  render(<App />);
  const content = await waitFor(() => {
    const el = document.getElementById("content");
    if (!el) throw new Error("no shell yet");
    return el;
  });
  // jsdom has no layout: a scroll offset is just a number the container remembers
  let top = 0;
  Object.defineProperty(content, "scrollTop", { get: () => top, set: (v: number) => (top = v), configurable: true });
  const go = (name: string) => fireEvent.click(document.querySelector(`.nav-btn[data-view="${name}"]`) as Element);
  const scrollTo = (v: number) => {
    top = v;
    fireEvent.scroll(content);
  };

  go("inventory");
  scrollTo(900);
  go("dashboard");
  expect(top).toBe(0);
  scrollTo(300);
  go("inventory");
  expect(top).toBe(900);
  go("dashboard");
  expect(top).toBe(300);
});
