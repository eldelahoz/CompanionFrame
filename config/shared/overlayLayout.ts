import { asRecord } from "./objectValidation";

export const OVERLAY_LAYOUT_KINDS = [
  "reward",
  "planner",
  "rivenLeft",
  "rivenRight",
  "arbiSummary",
  "tradeNotification",
] as const;
export type OverlayLayoutKind = (typeof OVERLAY_LAYOUT_KINDS)[number];
export interface OverlayFieldStyle {
  x: number;
  y: number;
  scale: number;
  color: string | null;
  hidden: boolean;
  /** Offsets for single cards, keyed by slot index; other cards use x and y. */
  cards?: Partial<Record<string, { x: number; y: number }>>;
}
export const DEFAULT_OVERLAY_FIELD_STYLE: Readonly<OverlayFieldStyle> = {
  x: 0,
  y: 0,
  scale: 1,
  color: null,
  hidden: false,
};
export interface OverlayLayout {
  version: 1;
  fields: Partial<Record<string, OverlayFieldStyle>>;
}
export interface OverlayDescriptor {
  kind: OverlayLayoutKind;
  canvas: { width: number; height: number };
  fields: readonly string[];
  hiddenByDefault?: readonly string[];
  /** Repeated cards whose fields can take their own offsets. */
  cardCount?: number;
}
export const REWARD_OVERLAY_FIELDS = [
  "slotLabel",
  "itemName",
  "rarity",
  "platinumIcon",
  "platinumValue",
  "ducatIcon",
  "ducatValue",
  "pricePlaceholder",
  "owned",
  "mastery",
  "foundry",
  "setOwned",
  "setPrice",
  "vaulted",
  "part0Icon",
  "part0Count",
  "part1Icon",
  "part1Count",
  "part2Icon",
  "part2Count",
  "part3Icon",
  "part3Count",
  "part4Icon",
  "part4Count",
  "part5Icon",
  "part5Count",
  "bestLabel",
  "bestName",
  "bestPlatinumIcon",
  "bestPlatinumValue",
  "bestPlaceholder",
  "scanSpinner",
  "scanText",
  "errorText",
  "dragHint",
  "closeButton",
] as const;
const plannerFields = [
  "relicCount",
  "relicName",
  "refinement",
  "vaulted",
  "profitLabel",
  "platinumIcon",
  "platinumValue",
  "ducatIcon",
  "ducatValue",
  "reward0Icon",
  "reward0Name",
  "reward0Chance",
  "reward0Owned",
  "reward1Icon",
  "reward1Name",
  "reward1Chance",
  "reward1Owned",
  "reward2Icon",
  "reward2Name",
  "reward2Chance",
  "reward2Owned",
  "reward3Icon",
  "reward3Name",
  "reward3Chance",
  "reward3Owned",
  "reward4Icon",
  "reward4Name",
  "reward4Chance",
  "reward4Owned",
  "reward5Icon",
  "reward5Name",
  "reward5Chance",
  "reward5Owned",
  "scanSpinner",
  "scanText",
  "errorText",
  "interactionHint",
  "dragHint",
  "closeButton",
] as const;
const rivenFields = [
  "panelLabel",
  "weaponName",
  "rollBadge",
  "rescanButton",
  "closeButton",
  "overallGrade",
  "scanSpinner",
  "scanText",
  "errorText",
  "weaponWarning",
  "emptyText",
  "interactionHint",
  "stat0Dot",
  "stat0Value",
  "stat0Name",
  "stat0Grade",
  "stat0Track",
  "stat1Dot",
  "stat1Value",
  "stat1Name",
  "stat1Grade",
  "stat1Track",
  "stat2Dot",
  "stat2Value",
  "stat2Name",
  "stat2Grade",
  "stat2Track",
  "stat3Dot",
  "stat3Value",
  "stat3Name",
  "stat3Grade",
  "stat3Track",
  "bestPositiveLabel",
  "bestPositiveChip",
  "bestNegativeLabel",
  "bestNegativeChip",
  "listingsLabel",
  "listingMatch",
  "listingPlatinum",
  "listingPlatinumUnit",
  "listingRolls",
  "listingStat0Value",
  "listingStat0Name",
  "listingStat1Value",
  "listingStat1Name",
  "listingStat2Value",
  "listingStat2Name",
  "listingStat3Value",
  "listingStat3Name",
] as const;
const arbiSummaryFields = [
  "summaryLabel",
  "node",
  "missionType",
  "duration",
  "rotations",
  "vitusValue",
  "vitusUncertainty",
  "vitusLabel",
  "dronesValue",
  "dronesLabel",
  "killsValue",
  "killsLabel",
  "saturationValue",
  "saturationLabel",
  "actualVitusValue",
  "actualVitusLabel",
  "vitusRateValue",
  "vitusRateLabel",
  "killRateValue",
  "killRateLabel",
  "killsPerDroneValue",
  "killsPerDroneLabel",
  "droneIntervalValue",
  "droneIntervalLabel",
  "squad",
  "endReason",
  "dragHint",
  "closeButton",
  "detailsButton",
] as const;
const tradeNotificationFields = [
  "thumbnail",
  "statusLabel",
  "tradeBadge",
  "quantity",
  "itemName",
  "platinumValue",
  "platinumUnit",
  "partnerName",
  "reputationText",
] as const;
const descriptors: Record<OverlayLayoutKind, OverlayDescriptor> = {
  reward: {
    kind: "reward",
    canvas: { width: 980, height: 236 },
    fields: REWARD_OVERLAY_FIELDS,
    cardCount: 4,
  },
  planner: {
    kind: "planner",
    canvas: { width: 460, height: 640 },
    fields: plannerFields,
    hiddenByDefault: plannerFields.filter((field) => /^reward\d/.test(field)),
  },
  rivenLeft: {
    kind: "rivenLeft",
    canvas: { width: 420, height: 640 },
    fields: rivenFields,
  },
  rivenRight: {
    kind: "rivenRight",
    canvas: { width: 420, height: 640 },
    fields: rivenFields,
  },
  arbiSummary: {
    kind: "arbiSummary",
    canvas: { width: 420, height: 252 },
    fields: arbiSummaryFields,
    hiddenByDefault: [
      "actualVitusValue",
      "actualVitusLabel",
      "vitusRateValue",
      "vitusRateLabel",
      "killRateValue",
      "killRateLabel",
      "killsPerDroneValue",
      "killsPerDroneLabel",
      "droneIntervalValue",
      "droneIntervalLabel",
      "squad",
      "endReason",
    ],
  },
  tradeNotification: {
    kind: "tradeNotification",
    canvas: { width: 370, height: 104 },
    fields: tradeNotificationFields,
  },
};
export function isOverlayLayoutKind(value: unknown): value is OverlayLayoutKind {
  return typeof value === "string" && (OVERLAY_LAYOUT_KINDS as readonly string[]).includes(value);
}
export function getOverlayDescriptor(kind: OverlayLayoutKind): OverlayDescriptor {
  return descriptors[kind];
}
export function isOverlayField(kind: OverlayLayoutKind, value: unknown): value is string {
  return typeof value === "string" && descriptors[kind].fields.includes(value);
}
export const OVERLAY_FIELD_OFFSET_LIMIT = 10_000;
function bounded(value: unknown, min: number, max: number): number | null {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.round(Math.min(max, Math.max(min, value)) * 100) / 100
    : null;
}
function normalizeCardOffsets(
  kind: OverlayLayoutKind,
  value: unknown,
): OverlayFieldStyle["cards"] | undefined {
  const raw = asRecord(value);
  const count = descriptors[kind].cardCount ?? 0;
  if (!raw || !count) return undefined;
  const cards: NonNullable<OverlayFieldStyle["cards"]> = {};
  for (let index = 0; index < count; index += 1) {
    const key = String(index);
    if (!Object.prototype.hasOwnProperty.call(raw, key)) continue;
    const entry = asRecord(raw[key]);
    const x = bounded(entry?.x, -OVERLAY_FIELD_OFFSET_LIMIT, OVERLAY_FIELD_OFFSET_LIMIT);
    const y = bounded(entry?.y, -OVERLAY_FIELD_OFFSET_LIMIT, OVERLAY_FIELD_OFFSET_LIMIT);
    if (x !== null && y !== null) cards[key] = { x, y };
  }
  return Object.keys(cards).length ? cards : undefined;
}
export function normalizeOverlayFieldStyle(
  kind: OverlayLayoutKind,
  value: unknown,
): OverlayFieldStyle {
  const raw = asRecord(value) ?? {};
  const cards = normalizeCardOffsets(kind, raw.cards);
  return {
    x: bounded(raw.x, -OVERLAY_FIELD_OFFSET_LIMIT, OVERLAY_FIELD_OFFSET_LIMIT) ?? 0,
    y: bounded(raw.y, -OVERLAY_FIELD_OFFSET_LIMIT, OVERLAY_FIELD_OFFSET_LIMIT) ?? 0,
    scale: bounded(raw.scale, 0.5, 3) ?? 1,
    color: typeof raw.color === "string" && /^#[\da-f]{6}$/i.test(raw.color) ? raw.color : null,
    hidden: raw.hidden === true,
    ...(cards ? { cards } : {}),
  };
}
export function normalizeOverlayLayout(kind: OverlayLayoutKind, value: unknown): OverlayLayout {
  const raw = asRecord(value);
  const fields = asRecord(raw?.fields);
  const result: OverlayLayout = { version: 1, fields: {} };
  for (const field of descriptors[kind].hiddenByDefault ?? [])
    result.fields[field] = { ...DEFAULT_OVERLAY_FIELD_STYLE, hidden: true };
  if (raw?.version !== 1 || !fields) return result;
  for (const field of descriptors[kind].fields) {
    if (Object.prototype.hasOwnProperty.call(fields, field))
      result.fields[field] = normalizeOverlayFieldStyle(kind, fields[field]);
  }
  return result;
}
