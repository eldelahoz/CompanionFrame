// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { createViewState, resetViewStates } from "./viewState";

const store = createViewState({ query: "", page: 0 });

describe("view state", () => {
  it("is back after the view unmounts and mounts again, and a reset clears it", () => {
    const first = renderHook(() => store.useField("query"));
    act(() => first.result.current[1]("rift"));
    first.unmount();
    expect(renderHook(() => store.useField("query")).result.current[0]).toBe("rift");
    act(() => resetViewStates());
    expect(store.get().query).toBe("");
  });

  it("supports functional updates and keeps the setter stable", () => {
    const { result } = renderHook(() => store.useField("page"));
    const setter = result.current[1];
    act(() => setter((p) => p + 2));
    expect(result.current[0]).toBe(2);
    expect(result.current[1]).toBe(setter);
  });
});
