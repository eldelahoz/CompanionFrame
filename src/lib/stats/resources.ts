import type { MessageKey } from "../i18n";

/** Catalog ids in display order; the backend records exactly these (services/stats/tracker.rs). "compact" renders k/M. */
export const STAT_RESOURCES = [
  { id: "plat", format: "plain" },
  { id: "ducats", format: "plain" },
  { id: "aya", format: "plain" },
  { id: "credits", format: "compact" },
  { id: "endo", format: "compact" },
  { id: "vitus", format: "plain" },
  { id: "kuva", format: "compact" },
  { id: "regalAya", format: "plain" },
  { id: "voidTraces", format: "plain" },
  { id: "steelEssence", format: "plain" },
  { id: "rivenSliver", format: "plain" },
  { id: "vosfor", format: "compact" },
  { id: "nitain", format: "plain" },
  { id: "forma", format: "plain" },
  { id: "argonCrystal", format: "plain" },
  { id: "orokinCell", format: "plain" },
  { id: "tellurium", format: "plain" },
  { id: "somaticFibers", format: "plain" },
  { id: "hexenon", format: "plain" },
  { id: "narmerIsoplast", format: "plain" },
  { id: "cetusWisp", format: "plain" },
  { id: "pathosClamp", format: "plain" },
] as const;

export type StatResourceId = (typeof STAT_RESOURCES)[number]["id"];

/** Shown before the user picks: the six oldest resources plus Kuva. */
export const DEFAULT_STAT_RESOURCE_IDS: readonly string[] = ["plat", "ducats", "aya", "credits", "endo", "vitus", "kuva"];

// The six oldest resources keep the keys they already had; the rest follow stats.resource.<id>.
// Spelled out because the i18n scan reads literals only.
const LABEL_KEYS: Record<StatResourceId, MessageKey> = {
  plat: "common.platinum",
  ducats: "common.ducats",
  aya: "stats.aya",
  credits: "common.credits",
  endo: "stats.endo",
  vitus: "stats.vitus",
  kuva: "stats.resource.kuva",
  regalAya: "stats.resource.regalAya",
  voidTraces: "stats.resource.voidTraces",
  steelEssence: "stats.resource.steelEssence",
  rivenSliver: "stats.resource.rivenSliver",
  vosfor: "stats.resource.vosfor",
  nitain: "stats.resource.nitain",
  forma: "stats.resource.forma",
  argonCrystal: "stats.resource.argonCrystal",
  orokinCell: "stats.resource.orokinCell",
  tellurium: "stats.resource.tellurium",
  somaticFibers: "stats.resource.somaticFibers",
  hexenon: "stats.resource.hexenon",
  narmerIsoplast: "stats.resource.narmerIsoplast",
  cetusWisp: "stats.resource.cetusWisp",
  pathosClamp: "stats.resource.pathosClamp",
};

export const statResourceLabelKey = (id: string): MessageKey => LABEL_KEYS[id as StatResourceId] ?? "common.unknown";

export const formatStatAmount = (value: number, format: "plain" | "compact", locale: string): string =>
  new Intl.NumberFormat(locale, format === "compact" ? { notation: "compact", maximumFractionDigits: 1 } : {}).format(value);
