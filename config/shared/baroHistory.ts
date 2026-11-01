import { asRecord as record } from "./objectValidation";
import { storeItemPath } from "./itemPath";

interface BaroHistoryItem {
  uniqueName: string;
  ducats: number | null;
  credits: number | null;
}

interface BaroHistoryVisit {
  id: string;
  activation: number;
  expiry: number;
  node: string;
  items: BaroHistoryItem[];
}

interface BaroLastSeen extends BaroHistoryItem {
  visitId: string;
  lastSeen: number;
}

export interface BaroHistory {
  version: 1;
  updatedAt: number;
  coverageStart: number | null;
  visits: BaroHistoryVisit[];
  lastSeen: BaroLastSeen[];
}

export const BARO_HISTORY_MAX_VISITS = 128;
export const BARO_HISTORY_MAX_ITEMS = 5000;

function timestamp(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0 && value <= 8.64e15;
}

function cost(value: unknown): value is number | null {
  return value === null || (typeof value === "number" && Number.isSafeInteger(value) && value >= 0);
}

function item(value: unknown): BaroHistoryItem | null {
  const raw = record(value);
  if (
    !raw ||
    typeof raw.uniqueName !== "string" ||
    !raw.uniqueName.startsWith("/Lotus/") ||
    raw.uniqueName.length > 512 ||
    !cost(raw.ducats) ||
    !cost(raw.credits)
  )
    return null;
  return { uniqueName: storeItemPath(raw.uniqueName), ducats: raw.ducats, credits: raw.credits };
}

export function normalizeBaroHistory(value: unknown): BaroHistory | null {
  const raw = record(value);
  if (
    !raw ||
    raw.version !== 1 ||
    !timestamp(raw.updatedAt) ||
    (raw.coverageStart !== null && !timestamp(raw.coverageStart)) ||
    !Array.isArray(raw.visits) ||
    raw.visits.length > BARO_HISTORY_MAX_VISITS ||
    !Array.isArray(raw.lastSeen) ||
    raw.lastSeen.length > BARO_HISTORY_MAX_ITEMS
  )
    return null;
  const visits: BaroHistoryVisit[] = [];
  const visitIds = new Set<string>();
  for (const value of raw.visits) {
    const visit = record(value);
    if (
      !visit ||
      typeof visit.id !== "string" ||
      !/^[A-Za-z0-9_-]{1,64}$/.test(visit.id) ||
      visitIds.has(visit.id) ||
      !timestamp(visit.activation) ||
      !timestamp(visit.expiry) ||
      visit.expiry <= visit.activation ||
      typeof visit.node !== "string" ||
      visit.node.length > 128 ||
      !Array.isArray(visit.items) ||
      visit.items.length > 500
    )
      return null;
    const items: BaroHistoryItem[] = [];
    const names = new Set<string>();
    for (const entry of visit.items) {
      const parsed = item(entry);
      if (!parsed || names.has(parsed.uniqueName)) return null;
      names.add(parsed.uniqueName);
      items.push(parsed);
    }
    visitIds.add(visit.id);
    visits.push({
      id: visit.id,
      activation: visit.activation,
      expiry: visit.expiry,
      node: visit.node,
      items,
    });
  }
  const lastSeen: BaroLastSeen[] = [];
  const names = new Set<string>();
  for (const value of raw.lastSeen) {
    const rawItem = record(value);
    const parsed = item(value);
    if (
      !rawItem ||
      !parsed ||
      names.has(parsed.uniqueName) ||
      !timestamp(rawItem.lastSeen) ||
      typeof rawItem.visitId !== "string" ||
      !/^[A-Za-z0-9_-]{1,64}$/.test(rawItem.visitId)
    )
      return null;
    names.add(parsed.uniqueName);
    lastSeen.push({ ...parsed, visitId: rawItem.visitId, lastSeen: rawItem.lastSeen });
  }
  return {
    version: 1,
    updatedAt: raw.updatedAt,
    coverageStart: raw.coverageStart,
    visits,
    lastSeen,
  };
}
