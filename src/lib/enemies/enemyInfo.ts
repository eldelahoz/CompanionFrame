import {
  CODEX_EXTRA_INFO,
  CODEX_FACTION_PLANETS,
  CODEX_SCAN_REQUIREMENTS,
  CODEX_TILE_SET_PLANETS,
  type CodexRequirement,
  type CodexSpawnInfo,
} from "../../data/codexScanRequirements";

/** A codex entry flattened for display: wiki rows carry spawn context, export extras only name and faction. */
export interface EnemyInfo {
  key: string;
  name: string;
  faction: string | null;
  planets: string[];
  tileSets: string[];
  missions: string[];
}

const normalizeEnemyName = (name: string) => name.trim().toLowerCase();

function fromRequirement(key: string): EnemyInfo | null {
  const entry = CODEX_SCAN_REQUIREMENTS[key] as (CodexRequirement & CodexSpawnInfo) | undefined;
  if (!entry) return null;
  return { key, name: entry.name, faction: entry.faction ?? null, planets: entry.planets ?? [], tileSets: entry.tileSets ?? [], missions: entry.missions ?? [] };
}

function fromExtra(key: string): EnemyInfo | null {
  const entry = CODEX_EXTRA_INFO[key];
  return entry?.name ? { key, name: entry.name, faction: entry.faction ?? null, planets: [], tileSets: [], missions: [] } : null;
}

const infoForKey = (key: string | null | undefined) => (key ? (fromRequirement(key) ?? fromExtra(key)) : null);

type NameIndex = Map<string, string | null>;
const NON_ENEMY_FACTIONS: ReadonlySet<string> = new Set(["lore", "objects"]);

// null marks a name two entries of one table claim; the wiki row wins over an export extra.
function buildNameIndex(enemiesOnly: boolean): NameIndex {
  const index: NameIndex = new Map();
  for (const [key, entry] of Object.entries(CODEX_SCAN_REQUIREMENTS)) {
    const name = normalizeEnemyName(entry.name);
    index.set(name, index.has(name) ? null : key);
  }
  for (const [key, entry] of Object.entries(CODEX_EXTRA_INFO)) {
    if (!entry.name || (enemiesOnly && NON_ENEMY_FACTIONS.has(entry.faction))) continue;
    const name = normalizeEnemyName(entry.name);
    if (!index.has(name)) index.set(name, key);
  }
  return index;
}

let byName: NameIndex | null = null;
let enemyByName: NameIndex | null = null;
const MIN_PARTIAL_LENGTH = 3;

function soleMatch(index: NameIndex, matches: (name: string) => boolean): EnemyInfo | null {
  let found: string | null = null;
  for (const [name, key] of index) {
    if (!matches(name)) continue;
    if (key === null || (found !== null && found !== key)) return null;
    found = key;
  }
  return infoForKey(found);
}

/** Lookup for a name the user typed: the exact enemy, then a sole prefix, then a sole substring. Ambiguity resolves to nothing. */
export function findEnemyByPartialName(name: string): EnemyInfo | null {
  byName ??= buildNameIndex(false);
  enemyByName ??= buildNameIndex(true);
  const query = normalizeEnemyName(name);
  if (enemyByName.has(query)) return infoForKey(enemyByName.get(query));
  if (byName.has(query) || query.length < MIN_PARTIAL_LENGTH) return null;
  return soleMatch(enemyByName, (n) => n.startsWith(query)) ?? soleMatch(enemyByName, (n) => n.includes(query));
}

/** Planets the faction holds, as a stand-in for entries that state no spawn context. */
export function factionSpawnPlanets(info: EnemyInfo | null): string[] {
  if (!info?.faction || info.planets.length > 0 || info.tileSets.length > 0 || info.missions.length > 0) return [];
  return CODEX_FACTION_PLANETS[info.faction] ?? [];
}

/** Planets the entry's tilesets sit on, for entries that give a tileset but no planet list. */
export function tileSetSpawnPlanets(info: EnemyInfo | null): string[] {
  if (!info || info.planets.length > 0) return [];
  const planets = new Set<string>();
  for (const tileSet of info.tileSets) for (const p of CODEX_TILE_SET_PLANETS[tileSet] ?? []) planets.add(p);
  return [...planets].sort((a, b) => a.localeCompare(b));
}

export interface SpawnGroup {
  labelKey: "enemy.planets" | "enemy.planetsFromTileSets" | "enemy.tileSets" | "enemy.missions";
  values: readonly string[];
}

/** The spawn lists an entry states plus tileset-derived planets, without the empty ones. */
export function enemySpawnGroups(info: EnemyInfo | null, tileSetPlanets: readonly string[]): SpawnGroup[] {
  if (!info) return [];
  const groups: SpawnGroup[] = [
    { labelKey: "enemy.planets", values: info.planets },
    { labelKey: "enemy.planetsFromTileSets", values: tileSetPlanets },
    { labelKey: "enemy.tileSets", values: info.tileSets },
    { labelKey: "enemy.missions", values: info.missions },
  ];
  return groups.filter((g) => g.values.length > 0);
}
