// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { resetMock, worldNotifyGet } from "../lib/ipc";
import { useWorldNotify } from "./worldNotify";

beforeEach(resetMock);

it("loads defaults, toggles a cycle bell and persists it", async () => {
  const { result } = renderHook(() => useWorldNotify());
  await waitFor(() => expect(result.current.prefs).not.toBeNull());
  expect(result.current.prefs?.cycleAlerts.earth).toBe(false);
  await act(() => result.current.toggleCycle("earth"));
  expect(result.current.prefs?.cycleAlerts.earth).toBe(true);
  expect((await worldNotifyGet()).cycleAlerts.earth).toBe(true);
});

it("adds and removes a fissure rule, capping the lead time", async () => {
  const { result } = renderHook(() => useWorldNotify());
  await waitFor(() => expect(result.current.prefs).not.toBeNull());
  const prefs = result.current.prefs!;
  const rule = { id: "a", tier: "Axi", missionType: "Capture", steelPath: "any" as const, planet: "" };
  await act(() => result.current.save({ ...prefs, cycleAlertMinutesBefore: 999, fissureAlerts: [rule] }));
  expect(result.current.prefs).toMatchObject({ cycleAlertMinutesBefore: 120, fissureAlerts: [rule] });
  await act(() => result.current.save({ ...result.current.prefs!, fissureAlerts: [] }));
  expect((await worldNotifyGet()).fissureAlerts).toEqual([]);
});
