// @vitest-environment jsdom
import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import * as ipc from "../lib/ipc";
import { useInventorySafety } from "./inventorySafety";

afterEach(() => vi.restoreAllMocks());

it("feeds the real mastery data into the rules, so the unmastered-recipe rule stops reading as degraded", async () => {
  vi.spyOn(ipc, "getMasteryProgress").mockResolvedValue({ items: [{ uniqueName: "/Lotus/Weapons/A", status: "mastered", currentlyOwned: true }] } as never);
  const { result } = renderHook(() => useInventorySafety(null, {}));
  expect(result.current.context.degradedRules).toContain("unmasteredRecipe");
  await waitFor(() => expect(result.current.context.mastered.has("/Lotus/Weapons/A")).toBe(true));
  expect(result.current.context.degradedRules).not.toContain("unmasteredRecipe");
  expect(result.current.context.degradedRules).not.toContain("pinnedGoal");
});

it("stays degraded when no mastery data exists", async () => {
  vi.spyOn(ipc, "getMasteryProgress").mockResolvedValue(null);
  const { result } = renderHook(() => useInventorySafety(null, {}));
  await waitFor(() => expect(ipc.getMasteryProgress).toHaveBeenCalled());
  expect(result.current.context.degradedRules).toContain("unmasteredRecipe");
});
