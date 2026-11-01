import { beforeEach, expect, test } from "vitest";
import { getDataState, resetData, setReady } from "./data";

beforeEach(resetData);

test("setReady updates state immutably", () => {
  const before = getDataState();
  setReady("items");
  expect(getDataState()).toEqual({ items: true });
  expect(before).toEqual({});
});

test("setReady is a no-op when unchanged", () => {
  setReady("relics");
  const s = getDataState();
  setReady("relics");
  expect(getDataState()).toBe(s);
});
