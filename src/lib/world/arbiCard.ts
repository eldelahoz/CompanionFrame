import type { ArbiScheduleDayGroup } from "./arbiScheduleData";
import { factionBadgeKey } from "./arbiScheduleData";

export interface ArbiCardText {
  header: string;
  zone: string;
  brand: string;
  time: (epochMs: number) => string;
}

const WIDTH = 640;
const PAD = 20;
const ROW = 28;
const DAY = 34;
const SCALE = 2;
const FACTION_TOKEN = { grineer: "--world-faction-grineer", corpus: "--world-faction-corpus", infested: "--world-faction-infested", corrupted: "--warning", other: "--text-secondary" } as const;

/** Local timezone as "Europe/Madrid, UTC+2" for the card's second line. */
export function timezoneLabel(date = new Date()): string {
  const zone = Intl.DateTimeFormat().resolvedOptions().timeZone || "";
  const offset = -date.getTimezoneOffset();
  const abs = Math.abs(offset);
  const utc = `UTC${offset < 0 ? "-" : "+"}${Math.floor(abs / 60)}${abs % 60 ? `:${String(abs % 60).padStart(2, "0")}` : ""}`;
  return zone ? `${zone.replace(/_/g, " ")}, ${utc}` : utc;
}

/** Draws the chosen schedule rows with the active theme's tokens and resolves a PNG. Canvas only: no extra dependency. */
export function renderArbiCard(groups: ArbiScheduleDayGroup[], text: ArbiCardText): Promise<Blob> {
  const css = getComputedStyle(document.documentElement);
  const token = (name: string) => css.getPropertyValue(name).trim() || "#888";
  const font = css.getPropertyValue("--font-sans").trim() || "sans-serif";
  const rows = groups.reduce((n, g) => n + g.entries.length, 0);
  const height = PAD * 2 + 56 + groups.length * DAY + rows * ROW + 32;

  const canvas = document.createElement("canvas");
  canvas.width = WIDTH * SCALE;
  canvas.height = height * SCALE;
  const ctx = canvas.getContext("2d");
  if (!ctx) return Promise.reject(new Error("no 2d context"));
  ctx.scale(SCALE, SCALE);
  ctx.textBaseline = "middle";

  ctx.fillStyle = token("--bg-surface");
  ctx.fillRect(0, 0, WIDTH, height);
  const put = (value: string, x: number, y: number, color: string, weight = 400, size = 13, align: CanvasTextAlign = "left") => {
    ctx.font = `${weight} ${size}px ${font}`;
    ctx.fillStyle = color;
    ctx.textAlign = align;
    ctx.fillText(value, x, y);
  };

  let y = PAD + 12;
  put(text.header, PAD, y, token("--text-primary"), 600, 17);
  y += 24;
  put(text.zone, PAD, y, token("--text-muted"), 400, 12);
  y += 22;
  for (const group of groups) {
    ctx.fillStyle = token("--bg-raised");
    ctx.fillRect(PAD, y, WIDTH - PAD * 2, DAY - 6);
    put(group.dayLabel, PAD + 10, y + (DAY - 6) / 2, token("--text-secondary"), 600, 12);
    y += DAY;
    for (const e of group.entries) {
      const mid = y + ROW / 2;
      put(text.time(e.epochMs), PAD + 10, mid, token("--text-secondary"));
      put(e.node, PAD + 80, mid, token("--text-primary"), 600);
      put(e.mission, PAD + 250, mid, token("--text-secondary"));
      put(e.faction, WIDTH - PAD - 10, mid, token(FACTION_TOKEN[factionBadgeKey(e.faction)]), 600, 12, "right");
      ctx.fillStyle = token("--border");
      ctx.fillRect(PAD, y + ROW - 1, WIDTH - PAD * 2, 1);
      y += ROW;
    }
  }
  put(text.brand, WIDTH - PAD, height - PAD, token("--accent"), 600, 11, "right");

  return new Promise((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("render produced no image"))), "image/png"));
}
