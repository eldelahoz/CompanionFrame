import { useEffect, useSyncExternalStore } from "react";
import { NOTIFICATION_LOG_MAX_ENTRIES } from "../../config/shared/notifications";
import { notifLogClear, notifLogGet, notifLogRemove, onNotificationAdded, type NotificationEntry } from "../lib/ipc";

const SEEN_STORAGE_KEY = "wf_notifications_seen_at";

function readSeenAt(): string {
  try {
    return localStorage.getItem(SEEN_STORAGE_KEY) ?? "";
  } catch {
    // no localStorage (tests, hardened webview)
    return "";
  }
}

// Module-level store so the badge and the history list agree without a provider.
let history: NotificationEntry[] = [];
let seenAt = readSeenAt();
let unread = 0;
const listeners = new Set<() => void>();

const publish = (): void => {
  unread = history.filter((e) => e.at > seenAt).length;
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void): (() => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};

/** Newest first. */
export const useNotificationHistory = (): NotificationEntry[] => useSyncExternalStore(subscribe, () => history);

/** Arrivals since the history was last opened. The badge counts these rather than the whole log,
 *  which sits at the cap for anyone who plays a while. */
export const useNotificationsUnread = (): number => useSyncExternalStore(subscribe, () => unread);

export function markNotificationsSeen(): void {
  seenAt = new Date().toISOString();
  try {
    localStorage.setItem(SEEN_STORAGE_KEY, seenAt);
  } catch {
    // no localStorage (tests, hardened webview)
  }
  publish();
}

export async function loadNotificationHistory(): Promise<void> {
  try {
    history = await notifLogGet();
    publish();
  } catch (err) {
    console.warn("[Notifications] history load failed:", err);
  }
}

export function addNotificationEntry(entry: NotificationEntry): void {
  history = [entry, ...history].slice(0, NOTIFICATION_LOG_MAX_ENTRIES);
  publish();
}

export async function removeNotificationEntry(id: string): Promise<void> {
  try {
    await notifLogRemove(id);
    history = history.filter((e) => e.id !== id);
    publish();
  } catch (err) {
    console.warn("[Notifications] history remove failed:", err);
  }
}

export async function clearNotificationHistory(): Promise<void> {
  try {
    await notifLogClear();
    history = [];
    publish();
  } catch (err) {
    console.warn("[Notifications] history clear failed:", err);
  }
}

/** Seeds the list and follows backend pushes while mounted (the badge host mounts it once). */
export function useNotificationFeed(): void {
  useEffect(() => {
    void loadNotificationHistory();
    const off = onNotificationAdded(addNotificationEntry);
    return () => void off.then((f) => f());
  }, []);
}

/** Test hook: back to an empty, fully-read store. */
export function resetNotificationsForTest(): void {
  history = [];
  seenAt = "";
  listeners.clear();
  publish();
}
