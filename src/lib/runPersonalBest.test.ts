import { expect, it } from "vitest";
import { percentDelta, runPersonalBest } from "./runPersonalBest";

it("percentDelta is null without a positive reference", () => {
  expect(percentDelta(5, null)).toBeNull();
  expect(percentDelta(5, 0)).toBeNull();
  expect(percentDelta(110, 100)).toBeCloseTo(10);
});

it("ranks a run among the others in either direction", () => {
  const best = runPersonalBest(120, [100, 90, Number.NaN], true);
  expect(best).toMatchObject({ rank: 1, poolSize: 3, isPb: true });
  expect(best.vsSecondPct).toBeCloseTo(20);

  const mid = runPersonalBest(50, [40, 60], false);
  expect(mid).toMatchObject({ rank: 2, isPb: false, vsSecondPct: null });
  expect(mid.vsBestPct).toBeCloseTo(25);
});

it("is alone in the pool without other runs", () => {
  expect(runPersonalBest(1, [], true)).toMatchObject({ rank: 1, poolSize: 1, vsBestPct: null });
});
