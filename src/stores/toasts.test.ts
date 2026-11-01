// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { addToast, clearToasts, removeToast, useToasts } from "./toasts";

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  clearToasts();
  vi.useRealTimers();
});

describe("toasts", () => {
  it("adds newest first with defaults and expires after the duration", () => {
    const { result } = renderHook(() => useToasts());
    act(() => void addToast({ message: "a" }));
    act(() => void addToast({ message: "b", level: "error", durationMs: 100 }));
    expect(result.current.map((t) => [t.message, t.level, t.durationMs])).toEqual([
      ["b", "error", 500],
      ["a", "info", 6000],
    ]);
    act(() => void vi.advanceTimersByTime(500));
    expect(result.current.map((t) => t.message)).toEqual(["a"]);
    act(() => void vi.advanceTimersByTime(5500));
    expect(result.current).toEqual([]);
  });

  it("keeps sticky toasts until dismissed and caps the stack at six", () => {
    const { result } = renderHook(() => useToasts());
    const id = addToast({ message: "stay", sticky: true });
    act(() => {
      for (let i = 0; i < 6; i++) addToast({ message: `n${i}`, sticky: true });
    });
    expect(result.current).toHaveLength(6);
    expect(result.current.some((t) => t.id === id)).toBe(false);
    const newest = result.current[0]!.id;
    act(() => void vi.advanceTimersByTime(60_000));
    expect(result.current).toHaveLength(6);
    act(() => removeToast(newest));
    expect(result.current).toHaveLength(5);
  });
});
