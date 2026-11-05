// @vitest-environment jsdom
import { renderHook } from "@testing-library/react";
import { expect, test } from "vitest";
import { applyIds, useRangeSelect } from "./useRangeSelect";

const ids = ["a", "b", "c", "d", "e"];
const setup = (order = ids) => renderHook(({ o }) => useRangeSelect(o), { initialProps: { o: order } });

test("a plain click picks one id", () => {
  const { result } = setup();
  expect(result.current("b", false)).toEqual(["b"]);
});

test("shift-click selects the range downwards and upwards, inclusive", () => {
  const { result } = setup();
  result.current("b", false);
  expect(result.current("d", true)).toEqual(["b", "c", "d"]);
  // the anchor moved to d
  expect(result.current("a", true)).toEqual(["a", "b", "c", "d"]);
});

test("applyIds can unselect a range", () => {
  const { result } = setup();
  result.current("a", false);
  const range = result.current("c", true);
  expect([...applyIds(new Set(ids), range, false)]).toEqual(["d", "e"]);
  expect([...applyIds(new Set(["e"]), range, true)].sort()).toEqual(["a", "b", "c", "e"]);
});

test("a shift-click with no anchor, or an anchor no longer displayed, is a plain click", () => {
  const { result, rerender } = setup();
  expect(result.current("c", true)).toEqual(["c"]);
  result.current("b", false);
  rerender({ o: ["a", "c", "d"] });
  expect(result.current("d", true)).toEqual(["d"]);
});
