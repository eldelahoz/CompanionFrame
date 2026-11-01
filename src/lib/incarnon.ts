import type { ItemDbEntry, RawInventoryData } from "../types/inventory";
import type { Translator } from "./i18n";

export const INCARNON_MAX_EVOLUTION = 5;

type IncarnonKind = "genesis" | "native";
type IncarnonStatus = "unlocked" | "adapter" | "missing";
type IncarnonSlot = "primary" | "secondary" | "melee";
export type IncarnonFilter = "all" | IncarnonStatus;

export const INCARNON_FILTERS: readonly IncarnonFilter[] = ["all", "unlocked", "adapter", "missing"];

export interface IncarnonWeapon {
  kind: IncarnonKind;
  /** English base-weapon name, the join key for the Circuit strip. */
  name: string;
  displayName?: string;
  /** Base weapon, or the adapter when the item DB has no picture of the weapon. */
  uniqueName: string;
  imageUrl: string;
  /** The weapon art, for when the adapter icon is missing on browse.wf. */
  fallbackImageUrl?: string;
  slot?: IncarnonSlot;
  /** Any weapon of the family (Prime, Vandal, MK1 and the like) is in the arsenal. */
  weaponOwned: boolean;
  /** Native weapons only: its blueprint waits in Recipes while the weapon is not owned. */
  blueprintOwned?: true;
  /** Spare Genesis adapters in MiscItems. */
  adapterCount: number;
  /** Genesis: the adapter sits on a family weapon. Native: the weapon is owned. */
  unlocked: boolean;
  /** Highest unlocked evolution, 1 to 5, absent without an EvolutionProgress entry. */
  evolution?: number;
  /** Weeks until the Steel Path Circuit offers the adapter again, 0 = this week. */
  circuitWeeks?: number;
  status: IncarnonStatus;
}

// warframestat spells some names differently from DE's export ("Ack And Brunt" vs "Ack & Brunt").
const circuitNameKey = (name: string): string =>
  name.toLowerCase().replace(/\s*&\s*/g, " and ").replace(/\s+/g, " ").trim();

/** Weekly Incarnon Genesis groups of the Steel Path Circuit, in cycle order. */
export const CIRCUIT_HARD_ROTATION: string[][] = [
  ["Braton", "Lato", "Skana", "Paris", "Kunai"],
  ["Boar", "Gammacor", "Angstrum", "Gorgon", "Anku"],
  ["Bo", "Latron", "Furis", "Furax", "Strun"],
  ["Lex", "Magistar", "Boltor", "Bronco", "Ceramic Dagger"],
  ["Torid", "Dual Toxocyst", "Dual Ichor", "Miter", "Atomos"],
  ["Ack & Brunt", "Soma", "Vasto", "Nami Solo", "Burston"],
  ["Zylok", "Sibear", "Dread", "Despair", "Hate"],
  ["Dera", "Sybaris", "Cestra", "Sicarus", "Okina"],
  ["Vectis", "Stug", "Ballistica", "Destreza", "Obex"],
];

/** Index of the rotation week the live choices belong to, or -1 when no known week matches. */
function circuitRotationIndex(rotation: string[][], choices: string[]): number {
  const set = new Set(choices.map(circuitNameKey));
  let best = -1;
  let bestHits = 0;
  rotation.forEach((week, i) => {
    const hits = week.filter((n) => set.has(circuitNameKey(n))).length;
    if (hits > bestHits) {
      bestHits = hits;
      best = i;
    }
  });
  return bestHits >= Math.ceil((rotation[best]?.length ?? 2) / 2) ? best : -1;
}

/** The rotation from the current week on, so a week's index is how many weeks away it is. */
const circuitWeeksFrom = (rotation: string[][], current: number): string[][] =>
  current < 0 ? [] : [...rotation.slice(current), ...rotation.slice(0, current)];

export function circuitWeeksLabel(weeks: number, t: Translator): string {
  if (weeks === 0) return t("world.thisWeek");
  if (weeks === 1) return t("world.nextWeek");
  return t("world.inWeeks", { n: weeks });
}

const GENESIS_SUFFIX = " Incarnon Genesis";
const SLOT_ORDER: Record<IncarnonSlot, number> = { primary: 0, secondary: 1, melee: 2 };
const ADAPTER_SLOT_RE = /\/IncarnonAdapters\/(Primary|Secondary|Melee)\//;
const WEAPON_COLLECTIONS = ["LongGuns", "Pistols", "Melee"] as const;
// EquipmentFeatures.INCARNON_GENESIS: set on a weapon whose adapter is installed.
const INCARNON_GENESIS_FEATURE = 512;

/** Adapters fit every variant of their weapon, so installed detection folds variants. */
const baseName = (name: string): string =>
  name.replace(/^(MK1-|Prisma |Mara |Dex |Telos |Rakta |Synoid |Sancti )/i, "").replace(/ (Prime|Vandal|Wraith)$/i, "");

const familyKey = (name: string): string => circuitNameKey(baseName(name));

function slotOfEntry(entry: ItemDbEntry | undefined): IncarnonSlot | undefined {
  const product = (entry?.productCategory || "").toLowerCase();
  const category = (entry?.category || "").toLowerCase();
  if (product === "longguns" || category === "primary") return "primary";
  if (product === "pistols" || category === "secondary") return "secondary";
  if (product === "melee" || category === "melee") return "melee";
  return undefined;
}

function slotOfAdapter(uniqueName: string): IncarnonSlot | undefined {
  const m = ADAPTER_SLOT_RE.exec(uniqueName);
  return m ? (m[1].toLowerCase() as IncarnonSlot) : undefined;
}

interface Hit {
  uniqueName: string;
  entry: ItemDbEntry;
}

/** One walk of the item DB: base weapons and adapters by name key, plus natively-Incarnon weapons. */
function indexItemDb(itemDb: Record<string, ItemDbEntry>) {
  const byName = new Map<string, Hit>();
  const adapters = new Map<string, Hit>();
  const natives: Hit[] = [];
  for (const [uniqueName, entry] of Object.entries(itemDb)) {
    if (!entry?.name || !entry.imageUrl) continue;
    const key = circuitNameKey(entry.name);
    if (!byName.has(key)) byName.set(key, { uniqueName, entry });
    if (key.endsWith(GENESIS_SUFFIX.toLowerCase())) {
      const base = key.slice(0, -GENESIS_SUFFIX.length);
      if (!adapters.has(base)) adapters.set(base, { uniqueName, entry });
    }
    if (entry.incarnon === true) natives.push({ uniqueName, entry });
  }
  return { byName, adapters, natives };
}

function ownedSets(itemDb: Record<string, ItemDbEntry>, inv: RawInventoryData | null) {
  const adapterCounts = new Map<string, number>();
  const installed = new Set<string>();
  const ownedWeaponKeys = new Set<string>();
  for (const misc of inv?.MiscItems ?? []) {
    const count = typeof misc.ItemCount === "number" && Number.isFinite(misc.ItemCount) ? Math.floor(misc.ItemCount) : 0;
    if (misc.ItemType?.includes("/IncarnonAdapters/") && count > 0) {
      adapterCounts.set(misc.ItemType, (adapterCounts.get(misc.ItemType) ?? 0) + count);
    }
  }
  for (const collection of WEAPON_COLLECTIONS) {
    const rows = (inv?.[collection] ?? []) as Array<{ ItemType?: string; Features?: unknown }>;
    for (const weapon of rows) {
      const name = weapon.ItemType ? itemDb[weapon.ItemType]?.name : undefined;
      if (!name) continue;
      const key = familyKey(name);
      ownedWeaponKeys.add(key);
      if (typeof weapon.Features === "number" && weapon.Features & INCARNON_GENESIS_FEATURE) installed.add(key);
    }
  }
  return { adapterCounts, installed, ownedWeaponKeys };
}

/** Rank is 0-based: Rank 4 is the fifth and last evolution. */
function evolutionByFamily(itemDb: Record<string, ItemDbEntry>, inv: RawInventoryData | null) {
  const out = new Map<string, { itemType: string; tier: number }>();
  const rows = inv?.EvolutionProgress;
  if (!Array.isArray(rows)) return out;
  for (const row of rows as Array<{ ItemType?: unknown; Rank?: unknown }>) {
    if (typeof row?.ItemType !== "string" || typeof row.Rank !== "number" || !Number.isFinite(row.Rank)) continue;
    const name = itemDb[row.ItemType]?.name;
    if (!name) continue;
    const tier = Math.min(INCARNON_MAX_EVOLUTION, Math.max(1, Math.floor(row.Rank) + 1));
    const key = familyKey(name);
    if (tier > (out.get(key)?.tier ?? 0)) out.set(key, { itemType: row.ItemType, tier });
  }
  return out;
}

/** Name key to weeks until the Steel Path Circuit offers that adapter; live choices read as this week. */
function circuitWeeksByKey(hardChoices: string[]): Map<string, number> {
  const out = new Map<string, number>();
  circuitWeeksFrom(CIRCUIT_HARD_ROTATION, circuitRotationIndex(CIRCUIT_HARD_ROTATION, hardChoices)).forEach((week, weeks) => {
    for (const name of week) out.set(circuitNameKey(name), weeks);
  });
  for (const name of hardChoices) out.set(circuitNameKey(name), 0);
  return out;
}

function compareWeapons(a: IncarnonWeapon, b: IncarnonWeapon): number {
  if (a.kind !== b.kind) return a.kind === "genesis" ? -1 : 1;
  const slotA = a.slot ? SLOT_ORDER[a.slot] : 3;
  const slotB = b.slot ? SLOT_ORDER[b.slot] : 3;
  return slotA - slotB || a.name.localeCompare(b.name);
}

/** Genesis weapons come from item DB adapters plus the Circuit rotation; native ones
 *  from the item DB flag or an EvolutionProgress entry outside the Genesis families. */
export function buildIncarnonWeapons(
  itemDb: Record<string, ItemDbEntry>,
  inv: RawInventoryData | null,
  hardChoices: string[] = [],
): IncarnonWeapon[] {
  const { byName, adapters, natives: nativeHits } = indexItemDb(itemDb);
  const sets = ownedSets(itemDb, inv);
  const evolution = evolutionByFamily(itemDb, inv);
  const circuitWeeks = circuitWeeksByKey(hardChoices);

  const seeds = new Map<string, { fallbackName: string; adapter?: Hit }>();
  for (const [key, adapter] of adapters) {
    seeds.set(key, { fallbackName: (adapter.entry.name ?? "").slice(0, -GENESIS_SUFFIX.length), adapter });
  }
  for (const name of CIRCUIT_HARD_ROTATION.flat()) {
    if (!seeds.has(circuitNameKey(name))) seeds.set(circuitNameKey(name), { fallbackName: name });
  }

  const weapons: IncarnonWeapon[] = [];
  for (const [key, { fallbackName, adapter }] of seeds) {
    const base = byName.get(key);
    const adapterCount = adapter ? (sets.adapterCounts.get(adapter.uniqueName) ?? 0) : 0;
    const unlocked = sets.installed.has(key);
    const tier = evolution.get(key)?.tier;
    const weeks = circuitWeeks.get(key);
    const slot = (adapter && slotOfAdapter(adapter.uniqueName)) || slotOfEntry(base?.entry);
    weapons.push({
      kind: "genesis",
      name: base?.entry.name ?? fallbackName,
      ...(base?.entry.displayName ? { displayName: base.entry.displayName } : {}),
      uniqueName: base?.uniqueName ?? adapter?.uniqueName ?? "",
      imageUrl: adapter?.entry.imageUrl || base?.entry.imageUrl || "",
      ...(base?.entry.imageUrl ? { fallbackImageUrl: base.entry.imageUrl } : {}),
      ...(slot ? { slot } : {}),
      weaponOwned: sets.ownedWeaponKeys.has(key),
      adapterCount,
      unlocked,
      ...(tier !== undefined ? { evolution: tier } : {}),
      ...(weeks !== undefined ? { circuitWeeks: weeks } : {}),
      status: unlocked ? "unlocked" : adapterCount > 0 ? "adapter" : "missing",
    });
  }

  const natives = new Map<string, Hit>();
  for (const hit of nativeHits) {
    const key = familyKey(hit.entry.name ?? "");
    if (!seeds.has(key) && !natives.has(key)) natives.set(key, hit);
  }
  for (const [key, row] of evolution) {
    const entry = itemDb[row.itemType];
    if (seeds.has(key) || natives.has(key) || !slotOfEntry(entry)) continue;
    natives.set(key, { uniqueName: row.itemType, entry });
  }

  const blueprints = new Set<string>();
  if (natives.size > 0) {
    for (const row of inv?.Recipes ?? []) {
      const product = row?.ItemType ? itemDb[row.ItemType]?.buildsProduct : undefined;
      if (product) blueprints.add(product);
    }
  }
  for (const [key, { uniqueName, entry }] of natives) {
    const owned = sets.ownedWeaponKeys.has(key);
    const tier = evolution.get(key)?.tier;
    const slot = slotOfEntry(entry);
    weapons.push({
      kind: "native",
      name: entry.name || uniqueName,
      ...(entry.displayName ? { displayName: entry.displayName } : {}),
      uniqueName,
      imageUrl: entry.imageUrl || "",
      ...(slot ? { slot } : {}),
      weaponOwned: owned,
      ...(!owned && blueprints.has(uniqueName) ? { blueprintOwned: true as const } : {}),
      adapterCount: 0,
      unlocked: owned,
      ...(tier !== undefined ? { evolution: tier } : {}),
      status: owned ? "unlocked" : "missing",
    });
  }
  return weapons.sort(compareWeapons);
}

export const summarizeIncarnon = (weapons: readonly IncarnonWeapon[]) => ({
  total: weapons.length,
  unlocked: weapons.filter((w) => w.status === "unlocked").length,
});

export const incarnonUnlockedLabel = (weapon: Pick<IncarnonWeapon, "evolution">, t: Translator): string =>
  weapon.evolution ? t("incarnon.evolution", { tier: weapon.evolution, max: INCARNON_MAX_EVOLUTION }) : t("incarnon.installed");

export const filterIncarnon = (weapons: readonly IncarnonWeapon[], filter: IncarnonFilter): IncarnonWeapon[] =>
  filter === "all" ? [...weapons] : weapons.filter((w) => w.status === filter);
