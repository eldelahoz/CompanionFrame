// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test } from "vitest";
import { translator } from "../lib/i18n";
import RivensView from "./RivensView";

// Non-Tauri ipc serves 3 mock roll sessions: Braton Prime (pinned, 14 rolls), Kuva Bramma, Gram Prime.
const tr = translator("en");
const rows = () => [...document.querySelectorAll("[data-riven-session]")].map((r) => r.querySelector("strong")?.textContent);

beforeEach(() => localStorage.clear());
const openTab = (tab: string) => localStorage.setItem("cf_rivens_tab", tab);
afterEach(cleanup);

test("lists the stored sessions, pinned first, with their totals", async () => {
  openTab("sessions");
  render(<RivensView tr={tr} locale="en" />);
  await waitFor(() => expect(rows()).toEqual(["Braton Prime", "Kuva Bramma", "Gram Prime"]));
  expect(document.querySelector("[data-riven-totals]")?.textContent).toBe(
    tr("rivens.sessions.totals", { sessions: 3, rolls: 23, kuva: "27.6K" }),
  );
});

test("the search box filters by weapon or note", async () => {
  openTab("sessions");
  render(<RivensView tr={tr} locale="en" />);
  await waitFor(() => expect(rows()).toHaveLength(3));
  fireEvent.change(screen.getByPlaceholderText(tr("common.searchPlaceholder")), { target: { value: "gram" } });
  expect(rows()).toEqual(["Gram Prime"]);
  fireEvent.change(screen.getByPlaceholderText(tr("common.searchPlaceholder")), { target: { value: "chasing" } });
  expect(rows()).toEqual(["Braton Prime"]);
});

test("pinning lifts a session above the unpinned ones and deleting removes it", async () => {
  openTab("sessions");
  render(<RivensView tr={tr} locale="en" />);
  await waitFor(() => expect(rows()).toHaveLength(3));
  fireEvent.click(document.querySelectorAll("[data-riven-session]")[2].querySelector("[data-riven-pin]")!);
  await waitFor(() => expect(rows()).toEqual(["Braton Prime", "Gram Prime", "Kuva Bramma"]));
  fireEvent.click(document.querySelectorAll("[data-riven-delete]")[1]);
  await waitFor(() => expect(rows()).toEqual(["Braton Prime", "Kuva Bramma"]));
});

test("the finder tab asks for a weapon before it can search", async () => {
  render(<RivensView tr={tr} locale="en" />);
  fireEvent.click(screen.getByText(tr("rivens.tab.finder")));
  expect((document.querySelector("[data-riven-search]") as HTMLButtonElement).disabled).toBe(true);
});

// Non-Tauri ipc serves 4 decoded rivens (Braton Prime graded A+, Gram Prime C, Soma Prime B+, Acceltra S) and 2 veiled ones plus 2 unseen.
test("the unveiled tab shows the decoded inventory rivens and filters them by grade", async () => {
  render(<RivensView tr={tr} locale="en" />);
  await waitFor(() => expect(document.querySelectorAll("[data-riven-card]")).toHaveLength(4));
  expect(document.querySelector("[data-riven-card=riven-a] [data-riven-grade]")?.textContent).toBe("A+");
  fireEvent.change(document.querySelector("[data-riven-grade-select]")!, { target: { value: "C" } });
  expect([...document.querySelectorAll("[data-riven-card]")].map((c) => c.getAttribute("data-riven-card"))).toEqual(["riven-b"]);
  expect(screen.getByText(tr("rivens.tab.unveiled"))).toBeTruthy();
});

test("the veiled tab groups veiled rivens by challenge and lists the unseen ones", async () => {
  openTab("veiled");
  render(<RivensView tr={tr} locale="en" />);
  await waitFor(() => expect(document.querySelectorAll("[data-riven-veiled-group]")).toHaveLength(2));
  expect(document.querySelector("[data-riven-veiled-group='Kill enemies with a headshot'] [data-riven-veiled-challenge]")?.textContent).toContain("40 / 150");
  expect(screen.getByText(tr("rivens.unseenTitle"))).toBeTruthy();
});
