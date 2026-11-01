// Typed shim over the codex scan table (wiki.warframe.com Module:Enemies/data/*, CC BY-SA, plus DE PublicExport).
import data from "./codexScanRequirements.json";

export interface CodexRequirement {
  name: string;
  scans: number;
  faction: string;
  image?: string;
  eximusScans?: number;
}

export const CODEX_SCAN_REQUIREMENTS = data.requirements as Record<string, CodexRequirement>;

/** Lowercased avatar paths a profile records scans against, mapped to the entry they credit. */
export const CODEX_SCAN_AVATARS = data.avatars as Record<string, { key: string; eximus?: true }>;

/** Profile-only scans from DE PublicExport. */
export const CODEX_EXTRA_INFO = data.extraInfo as Record<
  string,
  { name?: string; icon?: string; faction: string; scans?: number; eximusScans?: number }
>;

/** Spawn context the wiki rows state; absent on export-only entries. */
export interface CodexSpawnInfo {
  planets?: string[];
  tileSets?: string[];
  missions?: string[];
  type?: string;
  description?: string;
  link?: string;
  baseLevel?: number;
}
/** Planets each faction holds on the star chart, keyed by codex faction. */
export const CODEX_FACTION_PLANETS = data.factionPlanets as Record<string, string[]>;
/** Planets each tileset sits on, keyed by wiki tileset name. */
export const CODEX_TILE_SET_PLANETS = data.tileSetPlanets as Record<string, string[]>;
