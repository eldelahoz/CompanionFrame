import { useEffect, useState } from "react";
import type { DropInfo } from "../../bindings";
import type { MarketAcquisition } from "../../../config/shared/marketAcquisition";
import { resolveComponentWikiFallback } from "../../lib/componentResolution";
import { dropTableQuery, latestDropTableSources, resolveDrops } from "../../lib/resolveDrops";
import { buildWikiUrl, toOfficialWikiUrl } from "../../lib/wikiUrl";
import type { ItemDbEntry } from "../../types/inventory";
import DropsList from "../DropsList";
import { Badge, Readout } from "../ui";
import type { MessageKey, Translator } from "../../lib/i18n";
import { missingPartsPrice, type IncompleteSetRow, type MissingPart, type RelicSource } from "../../lib/inventory/incompleteSets";
import type { PriceInfo } from "../../lib/inventory/itemPrices";
import { getRelicDatabase, onRelicDbUpdated } from "../../lib/ipc";
import { RELIC_QUALITY_LABEL_KEY } from "../../lib/relic/relicConstants";
import type { RelicDatabase } from "../../lib/relic/types";
import { setRelicFilter } from "../../stores/relics";
import { openView } from "../../stores/nav";

/** Relics shown per missing part before the rest fold away. */
const RELICS_SHOWN = 6;

const partName = (p: { name: string; displayName?: string }): string => p.displayName || p.name;

/** Loaded only while `enabled`, so the inventory never pays for a relic database nobody looks at. */
export function useRelicDb(enabled: boolean): RelicDatabase | null {
  const [db, setDb] = useState<RelicDatabase | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    const load = () => void getRelicDatabase().then((next) => live && setDb(next), () => {});
    load();
    const off = onRelicDbUpdated(load);
    return () => {
      live = false;
      void off.then((f) => f());
    };
  }, [enabled]);
  return db;
}

function Relic({ source, tr }: { source: RelicSource; tr: Translator }) {
  const rarity = source.rarity.toLowerCase();
  const rarityLabel = ["common", "uncommon", "rare"].includes(rarity) ? tr(`overlay.reward.rarity.${rarity}` as MessageKey) : source.rarity;
  const owned = source.ownedQualities.map((q) => `${q.count} ${tr(RELIC_QUALITY_LABEL_KEY[q.quality])}`).join(", ");
  return (
    <li className="sets-relic" data-set-relic={source.group.key} data-owned={source.ownedTotal > 0 || undefined}>
      <span className="sets-relic-name">{source.group.name}</span>
      <Badge tone={rarity === "rare" ? "warning" : rarity === "uncommon" ? "info" : "neutral"}>{rarityLabel}</Badge>
      {source.group.vaulted && <Badge tone="danger">{tr("common.vaulted")}</Badge>}
      {source.ownedTotal > 0 && (
        <span className="sets-relic-owned" title={owned}>
          {tr("inventory.sets.relicOwned", { count: source.ownedTotal, refinements: owned })}
        </span>
      )}
    </li>
  );
}

type ItemDb = Record<string, ItemDbEntry>;
const NO_RELICS_OWNED = {};

/** A part no relic drops: its boss or mission drops, else a way to learn where it comes from. Drop tables load only once `open`. */
function OtherSources({ m, setName, itemDb, open, tr }: { m: MissingPart; setName: string; itemDb: ItemDb; open: boolean; tr: Translator }) {
  const entry = m.part.uniqueName ? itemDb[m.part.uniqueName] : null;
  const query = open ? dropTableQuery(m.part as never, itemDb as never, null) : null;
  const [table, setTable] = useState<{ query: string; drops: DropInfo[] } | null>(null);
  useEffect(() => {
    if (!query) return;
    let live = true;
    void latestDropTableSources((q, drops) => live && setTable({ query: q, drops }))(query);
    return () => {
      live = false;
    };
  }, [query]);
  const drops = query && table?.query === query ? table.drops : resolveDrops(m.part as never, itemDb as never, null);
  if (!open) return null;
  if (drops.length > 0) return <DropsList drops={drops} relicDb={null} relicOwned={NO_RELICS_OWNED} tr={tr} title={tr("detail.dropSources")} initialLimit={3} />;
  if (query && table?.query !== query) return <span className="inv-muted">{tr("inventory.sets.relicsLoading")}</span>;
  const acq: MarketAcquisition = { ...(m.part as MarketAcquisition), ...(entry as MarketAcquisition | null) };
  const href = entry?.wikiaUrl ? toOfficialWikiUrl(entry.wikiaUrl) : buildWikiUrl(resolveComponentWikiFallback(m.part, setName, entry));
  return (
    <div className="sets-sources" data-set-no-relics>
      <span className="inv-muted">{tr("inventory.sets.notFromRelics")}</span>
      {acq.marketBuyable && (
        <span className="inv-muted" data-set-market>
          {acq.marketCredits ? tr("inventory.sets.marketCredits", { credits: acq.marketCredits.toLocaleString() }) : tr("inventory.sets.market")}
        </span>
      )}
      {acq.dojoResearch && <span className="inv-muted">{tr("inventory.sets.clanResearch")}</span>}
      <a className="sets-link" data-set-wiki href={href} target="_blank" rel="noreferrer noopener">
        {tr("inventory.sets.howToGet")}
      </a>
    </div>
  );
}

function Missing({ m, setKey, setName, itemDb, open, prices, tr }: { m: MissingPart; setKey: string; setName: string; itemDb: ItemDb; open: boolean; prices: ReadonlyMap<string, PriceInfo>; tr: Translator }) {
  const name = partName(m.part);
  const sources = m.sources ?? [];
  const price = m.slug ? prices.get(m.slug)?.price : null;
  const ownedRelics = sources.filter((s) => s.ownedTotal > 0).length;
  const openRelics = () => {
    setRelicFilter({ search: name, tierFilter: "all", ownershipMode: "all", qualityMode: "intact" });
    openView("relics");
  };
  return (
    <div className="sets-missing" data-set-missing={`${setKey}|${m.part.uniqueName ?? m.part.name}`}>
      <div className="sets-missing-head">
        <strong>{name}</strong>
        {price != null && <Readout currency="platinum" chip value={price} />}
        {m.need > 1 && <span className="inv-muted">{tr("inventory.sets.partCount", { have: m.have, need: m.need })}</span>}
        {sources.length > 0 && (
          <>
            <span className="inv-muted">{tr("inventory.sets.relicsOwned", { owned: ownedRelics, total: sources.length })}</span>
            <button type="button" className="sets-link" data-set-open-relics onClick={openRelics}>
              {tr("inventory.sets.openRelics")}
            </button>
          </>
        )}
      </div>
      {m.sources === null ? (
        <span className="inv-muted">{tr("inventory.sets.relicsLoading")}</span>
      ) : sources.length === 0 ? (
        <OtherSources m={m} setName={setName} itemDb={itemDb} open={open} tr={tr} />
      ) : (
        <ul className="sets-relics">
          {sources.slice(0, RELICS_SHOWN).map((s) => (
            <Relic key={s.group.key} source={s} tr={tr} />
          ))}
          {sources.length > RELICS_SHOWN && (
            <li>
              <details>
                <summary>{tr("inventory.sets.moreRelics", { count: sources.length - RELICS_SHOWN })}</summary>
                <ul className="sets-relics">
                  {sources.slice(RELICS_SHOWN).map((s) => (
                    <Relic key={s.group.key} source={s} tr={tr} />
                  ))}
                </ul>
              </details>
            </li>
          )}
        </ul>
      )}
    </div>
  );
}

/** Under an incomplete set's card or row: what the missing parts cost, and (folded away) each part with its relics. */
export default function SetExtra({ entry, prices, itemDb, tr }: { entry: IncompleteSetRow; prices: ReadonlyMap<string, PriceInfo>; itemDb: ItemDb; tr: Translator }) {
  const { set, missing } = entry;
  const [open, setOpen] = useState(false);
  const { total, priced } = missingPartsPrice(missing, (slug) => prices.get(slug)?.price);
  return (
    <div className="sets-extra" data-set-extra={set.internalName}>
      {priced > 0 && (
        <span className="sets-missing-price" data-set-missing-price title={tr("inventory.sets.missingPriceHint")}>
          {tr("inventory.sets.missingPrice")} <Readout currency="platinum" chip value={total} />
        </span>
      )}
      <details className="sets-detail" onToggle={(e) => setOpen(e.currentTarget.open)}>
        <summary>{tr("inventory.sets.missingParts", { count: missing.length })}</summary>
        <div className="sets-detail-body">
          {missing.map((m) => (
            <Missing key={m.part.uniqueName ?? m.part.name} m={m} setKey={set.internalName} setName={set.name} itemDb={itemDb} open={open} prices={prices} tr={tr} />
          ))}
        </div>
      </details>
    </div>
  );
}
