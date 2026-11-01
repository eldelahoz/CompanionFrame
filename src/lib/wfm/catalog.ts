// Name/slug/id/gameRef lookups over the backend's cached warframe.market catalog, and set membership with a memo.
import { titleFromSlug } from "../../../config/shared/wfm";
import type { CatalogItem, SetLookup } from "../ipc";

const SET_SUFFIX = " set";
const SLUG_SET_SUFFIX_RE = /_set$/;
// A few listings append "(Key)" or "(Veiled)" while the game says the bare name.
const NAME_PAREN_SUFFIX_RE = /^(.+?)\s*\([^()]*\)\s*$/;
const SEARCH_MIN_QUERY_LENGTH = 2;
const SEARCH_SCAN_MULTIPLIER = 2;

export interface CatalogIndex {
  items: readonly CatalogItem[];
  byId(id: string): CatalogItem | null;
  bySlug(slug: string): CatalogItem | null;
  /** Case-insensitive; also tries "<name> set" and the name without a trailing " set". */
  byName(name: string): CatalogItem | null;
  byGameRef(gameRef: string): CatalogItem | null;
  /** Names starting with the query first, then names containing it. */
  search(query: string, limit?: number): CatalogItem[];
}

export function buildCatalogIndex(items: readonly CatalogItem[]): CatalogIndex {
  const ids = new Map<string, CatalogItem>();
  const slugs = new Map<string, CatalogItem>();
  const names = new Map<string, CatalogItem>();
  const refs = new Map<string, CatalogItem>();
  for (const item of items) {
    if (item.id) ids.set(item.id, item);
    if (item.slug) slugs.set(item.slug, item);
    const nameLc = item.name.toLowerCase();
    if (nameLc) names.set(nameLc, item);
    if (item.gameRef) refs.set(item.gameRef.toLowerCase(), item);
  }
  // Aliases never shadow a real name, whatever the catalog order.
  for (const item of items) {
    const alias = titleFromSlug(item.slug.replace(SLUG_SET_SUFFIX_RE, "")).toLowerCase();
    if (alias && !names.has(alias)) names.set(alias, item);
    const base = (NAME_PAREN_SUFFIX_RE.exec(item.name.toLowerCase())?.[1] ?? "").trim();
    if (base && !names.has(base)) names.set(base, item);
  }
  const byName = (name: string): CatalogItem | null => {
    const key = String(name ?? "").toLowerCase();
    if (!key) return null;
    return (
      names.get(key) ??
      names.get(`${key}${SET_SUFFIX}`) ??
      (key.endsWith(SET_SUFFIX) ? names.get(key.slice(0, -SET_SUFFIX.length)) : undefined) ??
      null
    );
  };
  return {
    items,
    byId: (id) => ids.get(id) ?? null,
    bySlug: (slug) => slugs.get(slug) ?? null,
    byName,
    byGameRef: (ref) => refs.get(String(ref ?? "").toLowerCase()) ?? null,
    search(query, limit = 20) {
      const q = String(query ?? "").toLowerCase().trim();
      if (q.length < SEARCH_MIN_QUERY_LENGTH) return [];
      const starts: CatalogItem[] = [];
      const contains: CatalogItem[] = [];
      for (const item of items) {
        const name = item.name.toLowerCase();
        if (name.startsWith(q)) starts.push(item);
        else if (name.includes(q)) contains.push(item);
        if (starts.length + contains.length >= limit * SEARCH_SCAN_MULTIPLIER) break;
      }
      return [...starts, ...contains].slice(0, limit);
    },
  };
}

/** Whether an item takes ranks: v2 omits `maxRank` (or sends 0) for rankless ones. */
export const hasRanks = (item: { maxRank?: number | null }): boolean => (item.maxRank ?? 0) > 0;

export type SetFetcher = (slug: string) => Promise<SetLookup>;

/**
 * Set membership with a memo: one answer is stored under the set slug and every part slug, and
 * "not a set" is stored too. A failure is not stored (the next call asks again) and rejects.
 */
export function createSetResolver(fetchSet: SetFetcher): (slug: string) => Promise<SetLookup> {
  const done = new Map<string, SetLookup>();
  const inFlight = new Map<string, Promise<SetLookup>>();
  return (slug) => {
    const hit = done.get(slug);
    if (hit) return Promise.resolve(hit);
    const active = inFlight.get(slug);
    if (active) return active;
    const lookup = fetchSet(slug)
      .then((res) => {
        done.set(slug, res);
        if (res.kind === "set") {
          done.set(res.setSlug, res);
          for (const part of res.parts) done.set(part.slug, res);
        }
        return res;
      })
      .finally(() => inFlight.delete(slug));
    inFlight.set(slug, lookup);
    return lookup;
  };
}
