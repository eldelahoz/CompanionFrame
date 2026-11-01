import type { TradeEvent, TradeItem, TradeType } from "../../types/stats";
import { fallbackNameFromUniqueName } from "../../../config/shared/displayName";
import type { StatsImportEntry } from "../ipc";

export const MAX_STATS_IMPORT_FILE_BYTES = 50 * 1024 * 1024;
export const MAX_TRADE_IMPORT_ROWS = 10_000;
const MAX_STATS_IMPORT_ROWS = 10_000;

export function assertStatsImportFileSize(bytes: number): void {
  if (!Number.isFinite(bytes) || bytes < 0 || bytes > MAX_STATS_IMPORT_FILE_BYTES) {
    throw new Error("Stats import file exceeds 50 MB.");
  }
}

const num = (value: unknown): number | null => (typeof value === "number" && Number.isFinite(value) ? value : null);

/** AlecaFrame writes raw ids for anything it has no name for ("/AF_Special/Imprint/Bibou"), so the ledger stores a readable name. */
function alecaDisplayName(internalName: string, rawDisplay: string): string {
  const display = rawDisplay.trim();
  if (display && !display.startsWith("/")) return display;
  const source = internalName || display;
  return source ? fallbackNameFromUniqueName(source) : "";
}

export function parseAlecaFrameTrades(parsed: unknown): TradeEvent[] {
  if (!parsed || typeof parsed !== "object") return [];
  const rawTrades = (parsed as Record<string, unknown>).trades;
  if (!Array.isArray(rawTrades)) return [];

  const imported: TradeEvent[] = [];
  // Ids come from the trade itself, never its position: a newer export adds rows on top, and a positional id would re-import the same trade.
  const seenIds = new Map<string, number>();
  for (const entry of rawTrades.slice(0, MAX_TRADE_IMPORT_ROWS)) {
    if (!entry || typeof entry !== "object") continue;
    const t = entry as Record<string, unknown>;

    // Our own export: already a TradeEvent.
    if (
      typeof t.id === "string" &&
      typeof t.date === "string" &&
      (t.type === "sale" || t.type === "purchase" || t.type === "trade") &&
      Array.isArray(t.items)
    ) {
      imported.push(t as unknown as TradeEvent);
      continue;
    }

    const ts = typeof t.ts === "string" ? t.ts : null;
    if (!ts) continue;
    const afType = typeof t.type === "number" ? t.type : -1;
    if (afType < 0 || afType > 2) continue;
    const type: TradeType = afType === 1 ? "purchase" : afType === 0 ? "sale" : "trade";
    const totalPlat = num(t.totalPlat) ?? 0;
    const rawUser = typeof t.user === "string" ? t.user : "";
    const partner = rawUser.replace(/[\u{E000}-\u{F8FF}\u{F0000}-\u{FFFFD}]+$/u, "").trim();

    const items: TradeItem[] = [];
    const pushItems = (arr: unknown, direction: TradeItem["direction"]) => {
      if (!Array.isArray(arr)) return;
      for (const raw of arr as Record<string, unknown>[]) {
        const name = typeof raw.name === "string" ? raw.name : "";
        if (name === "/AF_Special/Platinum") continue;
        items.push({
          internalName: name,
          displayName: alecaDisplayName(name, typeof raw.displayName === "string" ? raw.displayName : ""),
          count: num(raw.cnt) ?? 1,
          direction,
        });
      }
    };
    pushItems(t.tx, "given");
    pushItems(t.rx, "received");

    const base = `af-${ts}-${totalPlat}-${partner}`;
    const repeat = seenIds.get(base) ?? 0;
    seenIds.set(base, repeat + 1);
    // Two rows on one stamp with the same plat and partner stay distinct rows.
    imported.push({ id: repeat === 0 ? base : `${base}#${repeat + 1}`, date: ts, type, platChange: totalPlat, items, ...(partner ? { partner } : {}) });
  }
  return imported;
}

/** A delta column, else the difference to the previous row's absolute balance. */
function readDelta(r: Record<string, unknown>, prev: Record<string, unknown> | null, deltaKeys: string[], absKey: string): number {
  for (const k of deltaKeys) {
    const v = num(r[k]);
    if (v !== null) return v;
  }
  const cur = num(r[absKey]);
  const before = prev ? num(prev[absKey]) : null;
  return cur !== null && before !== null ? cur - before : 0;
}

const isDay = (v: unknown): v is string => {
  if (typeof v !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(v)) return false;
  const [y, m, d] = v.split("-").map(Number);
  const t = new Date(Date.UTC(y, m - 1, d));
  return t.getUTCFullYear() === y && t.getUTCMonth() === m - 1 && t.getUTCDate() === d;
};

/** Daily stat rows of an AlecaFrame / WFHelper export (the backend re-validates them). */
export function normalizeAlecaFrameStats(parsed: unknown): StatsImportEntry[] {
  const p = parsed as Record<string, unknown> | null;
  const rows: unknown[] = Array.isArray(parsed) ? parsed : [p?.generalDataPoints, p?.history, p?.data].find(Array.isArray) ?? [];
  if (rows.length > MAX_STATS_IMPORT_ROWS) throw new Error(`Stats import exceeds ${MAX_STATS_IMPORT_ROWS} rows.`);
  const out: StatsImportEntry[] = [];
  rows.forEach((item, i) => {
    if (!item || typeof item !== "object") return;
    const r = item as Record<string, unknown>;
    const prev = i > 0 ? (rows[i - 1] as Record<string, unknown>) : null;
    const raw = typeof r.ts === "string" ? r.ts : typeof r.date === "string" ? r.date : null;
    const date = raw ? raw.slice(0, 10) : null;
    if (!isDay(date)) return;
    const entry: StatsImportEntry = {
      date,
      platDelta: readDelta(r, prev, ["platGain", "platDelta"], "plat"),
      creditsDelta: readDelta(r, prev, ["creditsDelta"], "credits"),
      endoDelta: readDelta(r, prev, ["endoDelta"], "endo"),
      ducatsDelta: readDelta(r, prev, ["ducatsDelta"], "ducats"),
      ayaDelta: readDelta(r, prev, ["ayaDelta"], "aya"),
      vitusDelta: readDelta(r, prev, ["vitusDelta"], "vitus"),
      relicsOpened: num(r.relicsOpened) ?? num(r.relicOpened) ?? 0,
      daysPlayed: num(r.daysPlayed) ?? 1,
      dailyTrades: num(r.trades) ?? num(r.dailyTrades) ?? 0,
    };
    for (const [key, source] of [["absPlat", "plat"], ["absCredits", "credits"], ["absEndo", "endo"], ["absDucats", "ducats"], ["absAya", "aya"], ["absVitus", "vitus"]] as const) {
      const v = num(r[key]) ?? num(r[source]);
      if (v !== null && v >= 0) entry[key] = v;
    }
    out.push(entry);
  });
  return out;
}
