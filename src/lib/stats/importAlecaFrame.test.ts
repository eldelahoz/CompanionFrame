import { describe, expect, it } from "vitest";
import { assertStatsImportFileSize, MAX_STATS_IMPORT_FILE_BYTES, MAX_TRADE_IMPORT_ROWS, normalizeAlecaFrameStats, parseAlecaFrameTrades } from "./importAlecaFrame";

describe("stats import limits", () => {
  it("accepts 50 MB and rejects one byte more", () => {
    expect(() => assertStatsImportFileSize(MAX_STATS_IMPORT_FILE_BYTES)).not.toThrow();
    expect(() => assertStatsImportFileSize(MAX_STATS_IMPORT_FILE_BYTES + 1)).toThrow("50 MB");
  });

  it("imports the app's own stats export back", () => {
    const trades = [{ id: "trade-1", date: "2026-01-01T10:00:00.000Z", type: "sale", platChange: 50, items: [] }];
    expect(parseAlecaFrameTrades({ exportedAt: "2026-08-13T00:00:00.000Z", history: [], trades })).toEqual(trades);
  });

  it("keeps AlecaFrame ids stable when a newer export adds rows on top", () => {
    const sobek = { ts: "2026-02-07T07:47:27.146Z", type: 0, totalPlat: 1500, user: "ZeusPrime22", tx: [{ name: "/AF_Special/Riven/Sobek/Visi-toxican", cnt: 1 }] };
    const older = { trades: [sobek, { ...sobek, ts: "2026-02-05T02:01:01.714Z", totalPlat: 300 }] };
    const newer = { trades: [{ ...sobek, ts: "2026-02-09T10:00:00.000Z", user: "Nova" }, ...older.trades] };

    const before = parseAlecaFrameTrades(older).map((t) => t.id);
    expect(before[0]).toBe("af-2026-02-07T07:47:27.146Z-1500-ZeusPrime22");
    expect(parseAlecaFrameTrades(newer).map((t) => t.id).slice(1)).toEqual(before);
  });

  it("keeps two rows on one stamp apart inside a single export", () => {
    const row = { ts: "2026-02-07T07:47:27.146Z", type: 0, totalPlat: 10, user: "Kestrel" };
    const ids = parseAlecaFrameTrades({ trades: [row, row] }).map((t) => t.id);
    expect(new Set(ids).size).toBe(2);
    expect(ids[0]).toBe("af-2026-02-07T07:47:27.146Z-10-Kestrel");
  });

  it("stops parsing trades at the import limit", () => {
    const trades = Array.from({ length: MAX_TRADE_IMPORT_ROWS + 1 }, (_, i) => ({ ts: `2026-01-01T00:00:${i}Z`, type: 0, totalPlat: i, user: "Trader" }));
    expect(parseAlecaFrameTrades({ trades })).toHaveLength(MAX_TRADE_IMPORT_ROWS);
  });
});

describe("parseAlecaFrameTrades", () => {
  it("maps types, directions, plat and the partner's trailing platform glyph", () => {
    const [sale, purchase, swap, bad] = parseAlecaFrameTrades({
      trades: [
        { ts: "2026-03-01T00:00:00Z", type: 0, totalPlat: 100, user: "Ash", tx: [{ name: "/Lotus/Forma", displayName: "Forma", cnt: 2 }, { name: "/AF_Special/Platinum", cnt: 100 }] },
        { ts: "2026-03-02T00:00:00Z", type: 1, totalPlat: 5, user: "Bo", rx: [{ name: "/Lotus/X", displayName: "X" }] },
        { ts: "2026-03-03T00:00:00Z", type: 2 },
        { ts: "2026-03-04T00:00:00Z", type: 9 },
      ],
    });
    expect(sale).toMatchObject({ type: "sale", platChange: 100, partner: "Ash", items: [{ displayName: "Forma", count: 2, direction: "given" }] });
    expect(sale.items).toHaveLength(1);
    expect(purchase).toMatchObject({ type: "purchase", items: [{ direction: "received", count: 1 }] });
    expect(swap).toMatchObject({ type: "trade", platChange: 0, items: [] });
    expect(swap).not.toHaveProperty("partner");
    expect(bad).toBeUndefined();
  });

  it("names raw AlecaFrame ids readably and ignores malformed input", () => {
    const [t] = parseAlecaFrameTrades({ trades: [{ ts: "2026-03-01T00:00:00Z", type: 0, tx: [{ name: "/AF_Special/Imprint/Bibou", displayName: "/AF_Special/Imprint/Bibou" }] }] });
    expect(t.items[0].displayName).toBe("Bibou");
    for (const junk of [null, 5, "x", {}, { trades: "no" }, { trades: [null, 3, { type: 0 }] }]) expect(parseAlecaFrameTrades(junk)).toEqual([]);
  });
});

describe("normalizeAlecaFrameStats", () => {
  it("derives deltas from balances and drops invalid dates", () => {
    const rows = normalizeAlecaFrameStats({ generalDataPoints: [{ ts: "2026-01-01T10:00:00Z", plat: 100 }, { ts: "2026-01-02", plat: 130, trades: 2 }, { ts: "nope" }] });
    expect(rows.map((r) => [r.date, r.platDelta, r.absPlat, r.dailyTrades])).toEqual([["2026-01-01", 0, 100, 0], ["2026-01-02", 30, 130, 2]]);
  });
});
