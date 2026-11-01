// @vitest-environment jsdom
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { mockNotify, notifGetState, notifSetDiscordPing, notifSetWebhook, notifTestWebhook, resetMock } from "../lib/ipc";
import type { NotificationEntry } from "../lib/ipc";
import {
  addNotificationEntry,
  clearNotificationHistory,
  loadNotificationHistory,
  markNotificationsSeen,
  removeNotificationEntry,
  resetNotificationsForTest,
  useNotificationFeed,
  useNotificationHistory,
  useNotificationsUnread,
} from "./notifications";

const entry = (id: string, at = "2026-08-30T10:00:00.000Z"): NotificationEntry => ({ id, at, kind: "trade", title: `T${id}`, body: "" });
const ids = () => renderHook(() => useNotificationHistory()).result.current.map((e) => e.id);

beforeEach(() => {
  resetMock();
  resetNotificationsForTest();
  try {
    localStorage.clear();
  } catch {
    // no localStorage in this environment
  }
});

describe("notification history store", () => {
  it("seeds from the backend and keeps the newest push first", async () => {
    mockNotify("trade", "T1");
    mockNotify("trade", "T2");
    await loadNotificationHistory();
    act(() => addNotificationEntry(entry("3")));
    const titles = renderHook(() => useNotificationHistory()).result.current.map((e) => e.title);
    expect(titles).toEqual(["T3", "T2", "T1"]);
  });

  it("mirrors the backend cap of 200", () => {
    for (let i = 0; i < 205; i += 1) addNotificationEntry(entry(`n${i}`));
    const all = ids();
    expect(all).toHaveLength(200);
    expect(all[0]).toBe("n204");
  });

  // The badge counts arrivals, not the log: a long session sits at the cap.
  it("counts only what arrived since the history was last opened", () => {
    const { result } = renderHook(() => useNotificationsUnread());
    act(() => addNotificationEntry(entry("old")));
    expect(result.current).toBe(1);
    act(() => markNotificationsSeen());
    expect(result.current).toBe(0);
    act(() => addNotificationEntry(entry("new", "2099-01-01T00:00:00.000Z")));
    expect(result.current).toBe(1);
  });

  it("drops one entry, or all of them, once the backend call resolves", async () => {
    mockNotify("app", "Keep");
    const gone = mockNotify("app", "Gone");
    await loadNotificationHistory();
    await act(() => removeNotificationEntry(gone.id));
    expect(ids()).toHaveLength(1);
    await act(() => clearNotificationHistory());
    expect(ids()).toEqual([]);
  });

  it("follows pushes while the feed is mounted and stops after unmount", async () => {
    const feed = renderHook(() => useNotificationFeed());
    const list = renderHook(() => useNotificationHistory());
    await waitFor(() => expect(list.result.current).toEqual([]));
    act(() => void mockNotify("world", "Baro"));
    await waitFor(() => expect(list.result.current.map((e) => e.title)).toEqual(["Baro"]));
    await act(async () => feed.unmount());
    act(() => void mockNotify("world", "Late"));
    expect(list.result.current).toHaveLength(1);
  });
});

describe("channel settings mock", () => {
  it("never hands back the URL, only a mask", async () => {
    const saved = await notifSetWebhook("discord", "https://discord.com/api/webhooks/1234/abcdefgh");
    expect(saved).toMatchObject({ ok: true });
    const state = await notifGetState();
    expect(state.webhooks.discord).toEqual({ configured: true, masked: "https://discord.com/...efgh" });
    expect(JSON.stringify(state)).not.toContain("webhooks/1234");
    expect(await notifTestWebhook("discord")).toEqual({ ok: true });
    expect(await notifTestWebhook("generic")).toEqual({ ok: false, error: "not-configured" });
  });

  it("rejects an http URL and a malformed ping target", async () => {
    expect(await notifSetWebhook("generic", "http://example.com/hook")).toEqual({ ok: false, error: "not-https" });
    expect(await notifSetDiscordPing("@everyone")).toEqual({ ok: false, error: "invalid-user-id" });
    expect(await notifSetDiscordPing("123456789012345678")).toMatchObject({ ok: true });
  });
});
