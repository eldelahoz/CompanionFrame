import { describe, expect, it } from "vitest";
import type { MarketStatPoint } from "../../bindings";
import { buildChart, collectRanks, MARGIN, monotonePath, niceTicks, segmentedMonotonePath, selectRows, type StatRow } from "./statsChart";

const DAY = 86_400_000;
const NOW = 100 * DAY;
const pt = (daysAgo: number, median: number, extra: Partial<MarketStatPoint> = {}): MarketStatPoint => ({
  source: "wfm",
  time: NOW - daysAgo * DAY,
  volume: 10,
  median,
  movingAvg: null,
  avgPrice: null,
  openPrice: null,
  closedPrice: null,
  minPrice: null,
  maxPrice: null,
  donchTop: null,
  donchBot: null,
  rank: null,
  ...extra,
});

describe("selectRows", () => {
  const all = [pt(1, 5), pt(40, 6), pt(80, 7), pt(2, 9, { time: null }), pt(3, 9, { median: null })];
  it("keeps the period, drops unplottable rows and sorts oldest first", () => {
    expect(selectRows(all, "30days", null, NOW).map((r) => r.median)).toEqual([5]);
    expect(selectRows(all, "90days", null, NOW).map((r) => r.median)).toEqual([7, 6, 5]);
  });
  it("partitions a ranked item by rank", () => {
    const ranked = [pt(1, 5, { rank: 0 }), pt(1, 30, { rank: 10 })];
    expect(selectRows(ranked, "latest", 10, NOW).map((r) => r.median)).toEqual([30]);
    expect(collectRanks(ranked)).toEqual([0, 10]);
    expect(collectRanks([pt(1, 5)])).toEqual([]);
  });
});

describe("paths", () => {
  it("draws one point, two points and a curve through every sample", () => {
    expect(monotonePath([])).toBe("");
    expect(monotonePath([{ x: 0, y: 1 }])).toBe("M0.0,1.0");
    expect(monotonePath([{ x: 0, y: 1 }, { x: 2, y: 3 }])).toBe("M0.0,1.0L2.0,3.0");
    expect(monotonePath([{ x: 0, y: 0 }, { x: 1, y: 5 }, { x: 2, y: 0 }]).match(/C/g)).toHaveLength(2);
  });
  it("breaks a line at a gap and skips single-point runs", () => {
    const p = (x: number) => ({ x, y: x });
    expect(segmentedMonotonePath([p(0), p(1), null, p(3)])).toBe("M0.0,0.0L1.0,1.0");
    expect(segmentedMonotonePath([null, p(1), null])).toBe("");
  });
});

describe("niceTicks", () => {
  it("picks round steps and survives a flat domain", () => {
    expect(niceTicks(0, 100, 5)).toEqual([0, 20, 40, 60, 80, 100]);
    expect(niceTicks(5, 5, 5)).toEqual([5]);
  });
});

describe("buildChart", () => {
  const rows = [
    pt(2, 10, { donchTop: 14, donchBot: 8, movingAvg: 11 }),
    pt(1, 12, { donchTop: 15, donchBot: 9 }),
    pt(0, 11, { donchTop: 14, donchBot: 9, movingAvg: 12, openPrice: 10, closedPrice: 12, minPrice: 9, maxPrice: 13 }),
  ] as StatRow[];
  const chart = buildChart(rows, 800, "en");
  it("lays one marker and bar per row inside the plot", () => {
    expect(chart.markers).toHaveLength(3);
    expect(chart.bars).toHaveLength(3);
    expect(chart.markers[0].x).toBeGreaterThan(MARGIN.left);
    expect(chart.markers[2].x).toBeLessThan(800 - MARGIN.right);
    expect(chart.band.startsWith("M")).toBe(true);
  });
  it("only draws a candle for a complete row and flags rising ones", () => {
    expect(chart.candles).toHaveLength(1);
    expect(chart.candles[0].rising).toBe(true);
  });
  it("breaks the moving average where it is missing and anchors both end labels", () => {
    expect(chart.movingAvg).toBe("");
    expect(chart.xLabels[0].anchor).toBe("start");
    expect(chart.xLabels[chart.xLabels.length - 1].anchor).toBe("end");
  });
});
