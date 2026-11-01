// @vitest-environment jsdom
import { beforeEach, expect, it } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { resetMock } from "../lib/ipc";
import { useWfmPresence } from "./wfmPresence";

beforeEach(resetMock);

it("starts empty and stopped, then reflects a manual pick", async () => {
  const { result } = renderHook(() => useWfmPresence());
  await waitFor(() => expect(result.current.presence).toMatchObject({ status: null, auto_active: false }));
  expect(result.current.ws).toBe("stopped");
  await act(() => result.current.setStatus("ingame"));
  expect(result.current.presence?.status).toBe("ingame");
});

it("hears presence pushed after mount and stops listening on unmount", async () => {
  const a = renderHook(() => useWfmPresence());
  const b = renderHook(() => useWfmPresence());
  await waitFor(() => expect(a.result.current.presence).not.toBeNull());
  await act(() => a.result.current.setStatus("online"));
  await waitFor(() => expect(b.result.current.presence?.status).toBe("online"));
  b.unmount();
  await act(() => a.result.current.setStatus("invisible"));
  expect(b.result.current.presence?.status).toBe("online");
});
