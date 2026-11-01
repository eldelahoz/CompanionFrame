// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, test } from "vitest";
import { translator } from "../../lib/i18n";
import { ShowMore, useBatch } from "./ShowMore";

afterEach(cleanup);
const tr = translator("en");

function List({ total = 200 }) {
  const [q, setQ] = useState("");
  const { limit, more } = useBatch(q, 10);
  const shown = Math.min(limit, total);
  return (
    <>
      <input aria-label="q" value={q} onChange={(e) => setQ(e.target.value)} />
      <ShowMore shown={shown} total={total} onMore={more} tr={tr} step={10} />
    </>
  );
}

test("loads a step at a time and starts over when the filter changes", () => {
  render(<List />);
  expect(screen.getByText("Showing 10 of 200")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Show 10 more" }));
  expect(screen.getByText("Showing 20 of 200")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("q"), { target: { value: "x" } });
  expect(screen.getByText("Showing 10 of 200")).toBeTruthy();
});

test("no button once everything is shown", () => {
  render(<List total={5} />);
  expect(screen.queryByRole("button")).toBeNull();
});
