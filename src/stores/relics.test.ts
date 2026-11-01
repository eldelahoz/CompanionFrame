// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { resetRelicView, RELIC_TABS, setRelicFilter, useRelicView } from "./relics";
import { renderHook, act } from "@testing-library/react";

beforeEach(() => {
  localStorage.clear();
  resetRelicView();
});

describe("relic view state", () => {
  it("starts on all tiers, owned relics only, solo", () => {
    const { result } = renderHook(() => useRelicView());
    expect(result.current).toMatchObject({ tierFilter: "all", ownershipMode: "owned", squadSize: 1, qualityMode: "owned", sortMode: "tier" });
  });

  it("merges a patch and notifies subscribers", () => {
    const { result } = renderHook(() => useRelicView());
    act(() => setRelicFilter({ search: "forma", vaultedMode: "vaulted" }));
    expect(result.current).toMatchObject({ search: "forma", vaultedMode: "vaulted", tierFilter: "all" });
  });

  it("remembers the tier tab across a reset, and ignores an unknown one", () => {
    setRelicFilter({ tierFilter: "Axi" });
    resetRelicView();
    expect(renderHook(() => useRelicView()).result.current.tierFilter).toBe("Axi");
    setRelicFilter({ tierFilter: "Omnia" });
    resetRelicView();
    expect(renderHook(() => useRelicView()).result.current.tierFilter).toBe("Axi");
    expect(RELIC_TABS).toContain("Requiem");
  });
});
