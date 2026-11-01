export function percentDelta(value: number, reference: number | null): number | null {
  if (reference === null || reference <= 0) return null;
  const result = ((value - reference) / reference) * 100;
  return Number.isFinite(result) ? result : null;
}

export function runPersonalBest(value: number, otherValues: readonly number[], higherIsBetter: boolean) {
  const others = otherValues.filter(Number.isFinite);
  const better = (candidate: number) => (higherIsBetter ? candidate > value : candidate < value);
  const bestOther = others.length
    ? others.reduce((best, c) => (higherIsBetter ? Math.max(best, c) : Math.min(best, c)))
    : null;
  const rank = others.filter(better).length + 1;
  return {
    value,
    rank,
    poolSize: others.length + 1,
    isPb: rank === 1,
    vsBestPct: percentDelta(value, bestOther),
    // When this run leads, the best OTHER run is second overall, including ties.
    vsSecondPct: rank === 1 ? percentDelta(value, bestOther) : null,
  };
}
