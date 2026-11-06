// @vitest-environment jsdom
import { afterEach, expect, test, vi } from "vitest";

const ev = vi.hoisted(() => ({ cbs: [] as Array<(e: { payload: string | null }) => void> }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_name: string, cb: (e: { payload: string | null }) => void) => (ev.cbs.push(cb), () => {}),
}));

afterEach(() => {
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

test("one inventory-updated payload is parsed once, however many listeners hear it", async () => {
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  const { onInventoryUpdated } = await import("./ipc");
  const spy = vi.spyOn(JSON, "parse");
  const heard: unknown[] = [];
  await Promise.all([1, 2, 3].map(() => onInventoryUpdated((d) => heard.push(d))));
  const text = JSON.stringify({ MiscItems: [{ ItemType: "/x", ItemCount: 1 }] });
  ev.cbs.forEach((cb) => cb({ payload: text }));
  expect(spy.mock.calls.filter(([t]) => t === text)).toHaveLength(1);
  expect(new Set(heard).size).toBe(1);
  expect(heard).toHaveLength(3);
});
