import { ExternalLink, Star, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { stripQuantityPrefix } from "../../config/shared/quantityPrefix";
import EnemySpawnPanel from "../components/enemies/EnemySpawnPanel";
import { Badge, Chip, EmptyState, IconButton, PageHeader, SearchInput, Segmented, Skeleton, Table } from "../components/ui";
import type { DropKind } from "../bindings";
import { buildItemNameIndex } from "../lib/componentResolution";
import { dropRarityColour, formatDropChance } from "../lib/dropDisplay";
import { findEnemyByPartialName, type EnemyInfo } from "../lib/enemies/enemyInfo";
import type { Translator } from "../lib/i18n";
import { dropSearch, getRelicDatabase, type DropRow, type DropSearchMode } from "../lib/ipc";
import { relicGroupForDisplayName } from "../lib/relic/relicInventory";
import type { RelicDatabase } from "../lib/relic/types";
import { useSavedSearches } from "../lib/savedSearches";
import { buildWikiUrl } from "../lib/wikiUrl";
import { useItemDatabase } from "../stores/items";
import { openItemBy, openRelicDetailed } from "../stores/modals";

export { formatDropChance };

const MODES: DropSearchMode[] = ["item", "place", "enemy"];
const MODE_LABEL = { item: "wiki.byItem", place: "wiki.byLocation", enemy: "wiki.byEnemy" } as const;
const MODE_PLACEHOLDER = { item: "wiki.searchPlaceholderItem", place: "wiki.searchPlaceholderPlace", enemy: "wiki.searchPlaceholderEnemy" } as const;
// The dictionary rejects duplicate values, so some tags borrow the key that already owns the word.
const KIND_LABEL = {
  enemy: "common.enemy",
  mission: "common.mission",
  bounty: "world.bountyLabel",
  relic: "drops.relicSuffix",
  sortie: "dailies.task.sortie",
  quest: "common.quest",
  syndicate: "common.syndicate",
  dojo: "drops.kind.dojo",
  other: null,
} as const satisfies Record<DropKind, string | null>;

const LINK = "cursor-pointer border-0 bg-transparent p-0 text-left text-inherit hover:text-accent hover:underline";

export default function WikiView({ tr }: { tr: Translator }) {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<DropSearchMode>("item");
  const [rows, setRows] = useState<DropRow[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [searched, setSearched] = useState(false);
  // The query a search actually ran with, so the header link and the spawn panel follow results, not keystrokes.
  const [ran, setRan] = useState("");
  const [relicDb, setRelicDb] = useState<RelicDatabase | null>(null);
  const token = useRef(0);
  const db = useItemDatabase();
  const saved = useSavedSearches(`wiki:${mode}`);
  const nameIndex = useMemo(() => buildItemNameIndex(db as never), [db]);

  useEffect(() => {
    void getRelicDatabase().then(setRelicDb, () => undefined);
  }, []);

  useEffect(() => {
    const q = query.trim();
    const mine = ++token.current;
    if (!q) {
      setRows([]);
      setTotal(0);
      setSearched(false);
      setRan("");
      setLoading(false);
      return;
    }
    setLoading(true);
    const timer = setTimeout(() => {
      void dropSearch(q, mode)
        .then((r) => {
          if (mine !== token.current) return; // a newer search superseded this one
          setRows(r.rows as DropRow[]);
          setTotal(r.total);
          setSearched(true);
          setRan(q);
        })
        .finally(() => mine === token.current && setLoading(false));
    }, 250);
    return () => clearTimeout(timer);
  }, [query, mode]);

  // Enemy drops are listed under "location", so a location search naming one shows where it spawns.
  const spawn: EnemyInfo | null = useMemo(() => (mode === "place" && ran ? findEnemyByPartialName(ran) : null), [mode, ran]);
  const wikiName = useMemo(() => {
    const lower = ran.toLowerCase();
    if (ran.length < 2) return ran;
    let best: string | null = null;
    for (const name of nameIndex.keys()) {
      const low = name.toLowerCase();
      if (low === lower) return name;
      if (low.startsWith(lower) && (best === null || name.length < best.length)) best = name;
    }
    return best ?? ran;
  }, [nameIndex, ran]);

  const openItem = (name: string) => {
    const relic = relicGroupForDisplayName(relicDb, name);
    if (relic) return openRelicDetailed(relic);
    // Bundled rows like "2X Orokin Cell" carry a quantity prefix the db lacks.
    openItemBy({ name: nameIndex.has(name) ? name : stripQuantityPrefix(name) });
  };
  const linkable = (name: string) => nameIndex.has(name) || nameIndex.has(stripQuantityPrefix(name)) || relicGroupForDisplayName(relicDb, name) !== null;
  const trimmed = query.trim();
  const isSaved = saved.list.some((s) => s.toLowerCase() === trimmed.toLowerCase());

  const placeCell = (row: DropRow) => {
    const relic = row.kind === "relic" ? relicGroupForDisplayName(relicDb, row.place) : null;
    if (relic) return <button type="button" className={LINK} onClick={() => openRelicDetailed(relic)}>{row.place}</button>;
    // An enemy row hands its name back as a location search, which opens the spawn panel.
    if (row.kind === "enemy")
      return (
        <button type="button" className={LINK} data-wiki-enemy-link onClick={() => { setMode("place"); setQuery(row.place); }}>
          {row.place}
        </button>
      );
    return row.place;
  };

  return (
    <section className="flex flex-col gap-4" data-wiki-view>
      <PageHeader title={tr("wiki.title")} sub={tr("wiki.description")} />
      <div className="flex flex-wrap items-center gap-2">
        <SearchInput
          label={tr(MODE_PLACEHOLDER[mode])}
          className="min-w-[14rem] flex-1"
          autoComplete="off"
          spellCheck={false}
          data-search-focus
          placeholder={tr(MODE_PLACEHOLDER[mode])}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <IconButton label={isSaved ? tr("wiki.searchAlreadySaved") : tr("wiki.saveSearch")} disabled={!trimmed || isSaved} onClick={() => saved.add(trimmed)} data-wiki-save>
          <Star aria-hidden="true" fill={isSaved ? "currentColor" : "none"} />
        </IconButton>
        {ran && (
          <a className="ui-btn ui-btn--sm" href={buildWikiUrl(wikiName)} target="_blank" rel="noreferrer noopener" title={tr("wiki.openOnWikiTitle")} data-wiki-search-link={wikiName}>
            <ExternalLink aria-hidden="true" />
            {tr("common.wiki")}
          </a>
        )}
        <Segmented
          label={tr("wiki.title")}
          active={mode}
          onSelect={setMode}
          items={MODES.map((m) => ({ key: m, label: <span data-wiki-mode={m}>{tr(MODE_LABEL[m])}</span> }))}
        />
      </div>

      {saved.list.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5" data-wiki-saved>
          <span className="text-xs text-text-muted">{tr("common.saved")}</span>
          {saved.list.map((s) => (
            <span key={s} className="inline-flex items-center gap-0.5">
              <Chip pressed={trimmed === s} title={tr("wiki.searchThis")} onClick={() => setQuery(s)}>{s}</Chip>
              <IconButton label={tr("wiki.removeSearch")} onClick={() => saved.remove(s)}>
                <X aria-hidden="true" />
              </IconButton>
            </span>
          ))}
        </div>
      )}

      {spawn && <EnemySpawnPanel info={spawn} tr={tr} />}

      {loading && rows.length === 0 ? (
        <div className="flex flex-col gap-2" aria-label={tr("common.searching")}>
          <Skeleton height="2.5rem" />
          <Skeleton height="2.5rem" />
          <Skeleton height="2.5rem" />
        </div>
      ) : !searched ? (
        <EmptyState title={tr("wiki.typeToSearch")} />
      ) : rows.length === 0 ? (
        <EmptyState title={tr("wiki.noResults", { query: trimmed })} />
      ) : (
        <>
          <Table mode="stack" label={tr("wiki.title")}>
            <thead>
              <tr>
                <th>{tr("common.item")}</th>
                <th>{tr("wiki.col.dropsFrom")}</th>
                <th className="ui-num">{tr("wiki.col.rarity")}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const kindKey = KIND_LABEL[row.kind];
                return (
                  <tr key={`${row.item}|${row.place}|${row.kind}|${row.rarity}|${row.chance}`}>
                    <td data-primary>
                      {linkable(row.item) ? <button type="button" className={LINK} onClick={() => openItem(row.item)}>{row.item}</button> : row.item}
                    </td>
                    <td data-label={tr("wiki.col.dropsFrom")}>
                      <span className="inline-flex flex-wrap items-center gap-1.5">
                        {kindKey && <Badge>{tr(kindKey)}</Badge>}
                        {placeCell(row)}
                      </span>
                    </td>
                    <td className="ui-num" data-label={tr("wiki.col.rarity")}>
                      {row.chance == null && row.kind === "quest" ? (
                        <span className="text-text-secondary">{tr("wiki.questReward")}</span>
                      ) : (
                        <>
                          <span className="font-semibold" style={{ color: dropRarityColour(row.rarity) }}>{row.rarity}</span>
                          {row.chance != null && <span className="ml-1.5 text-text-muted">{formatDropChance(row.chance)}</span>}
                        </>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </Table>
          {total > rows.length && <p className="m-0 text-xs text-text-muted">{tr("wiki.showingResults", { shown: rows.length, total })}</p>}
        </>
      )}
    </section>
  );
}
