// @vitest-environment jsdom
import { expect, it } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useGame } from "./game";

it("reports an idle game outside tauri", async () => {
  const { result } = renderHook(() => useGame());
  await waitFor(() => expect(result.current).toEqual({ running: false, logged_in: false, mission_tag: null }));
});
