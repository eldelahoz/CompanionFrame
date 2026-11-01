// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { translator } from "../lib/i18n";
import WikiView, { formatDropChance } from "./WikiView";

const tr = translator("en");

test("formats chances without trailing zeroes", () => {
  expect(formatDropChance(11.5)).toBe("11.5%");
  expect(formatDropChance(null)).toBe("");
});

test("typing an unknown query lists no-result text from the mock search", async () => {
  vi.useRealTimers();
  render(<WikiView tr={tr} />);
  expect(screen.getByText(tr("wiki.typeToSearch"))).toBeTruthy();
  fireEvent.change(screen.getByPlaceholderText(tr("wiki.searchPlaceholderItem")), { target: { value: "zzzz" } });
  await waitFor(() => expect(screen.getByText(tr("wiki.noResults", { query: "zzzz" }))).toBeTruthy());
});

test("saving a search lists it as a chip that reruns the query", async () => {
  cleanup();
  localStorage.clear();
  render(<WikiView tr={tr} />);
  const input = screen.getByPlaceholderText(tr("wiki.searchPlaceholderItem"));
  fireEvent.change(input, { target: { value: "forma" } });
  fireEvent.click(screen.getByLabelText(tr("wiki.saveSearch")));
  fireEvent.change(input, { target: { value: "" } });
  fireEvent.click(screen.getByTitle(tr("wiki.searchThis")));
  expect((input as HTMLInputElement).value).toBe("forma");
  fireEvent.click(screen.getByLabelText(tr("wiki.removeSearch")));
  expect(screen.queryByTitle(tr("wiki.searchThis"))).toBeNull();
});
