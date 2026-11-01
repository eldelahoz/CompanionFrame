// Dev-server (no Tauri) stand-ins for the public order book and the closed-sales history, deterministic per slug.
import type { BookOrder_Serialize as BookOrder, MarketStatPoint_Serialize as MarketStatPoint } from "../../bindings";

const seed = (slug: string): number => [...slug].reduce((n, c) => (n * 31 + c.charCodeAt(0)) % 9973, 7);

export function mockBook(slug: string): BookOrder[] {
  const base = 8 + (seed(slug) % 40);
  const ayatan = slug.startsWith("ayatan_");
  const names = ["Tenno_Ace", "Lotus.Fan", "PlatKing", "VoidTrader7", "Nova_Main", "Stalker99", "Kubrow_Dad", "Orokin_Eve"];
  return Array.from({ length: 16 }, (_, i): BookOrder => {
    const sell = i % 4 !== 3;
    const status = ["ingame", "ingame", "online", "offline"][i % 4];
    return {
      id: `${slug}-${i}`,
      type: sell ? "sell" : "buy",
      platinum: Math.max(1, base + (sell ? i : -i) + (i % 3)),
      quantity: 1 + (i % 4),
      perTrade: i % 5 === 4 ? 2 : null,
      rank: slug === "serration" ? (i % 2 ? 10 : 0) : null,
      subtype: null,
      visible: true,
      amberStars: ayatan ? i % 2 : null,
      cyanStars: ayatan ? i % 3 : null,
      user: { ingameName: names[i % names.length], status, platform: "pc", avatar: null },
    };
  });
}

export function mockHistory(slug: string): MarketStatPoint[] {
  const base = 8 + (seed(slug) % 40);
  const day = 86_400_000;
  const end = Math.floor(Date.now() / day) * day;
  return Array.from({ length: 90 }, (_, i) => {
    const wave = Math.round(Math.sin(i / 6) * (base / 5)) + (i % 5);
    const median = Math.max(1, base + wave);
    return {
      source: "wfm",
      time: end - (89 - i) * day,
      volume: 20 + ((i * 7) % 60),
      median,
      movingAvg: median + 1,
      avgPrice: median + (i % 3),
      openPrice: median - 1,
      closedPrice: median + (i % 2 ? 1 : -1),
      minPrice: median - 3,
      maxPrice: median + 4,
      donchTop: median + 5,
      donchBot: Math.max(0, median - 4),
      rank: null,
    };
  });
}
