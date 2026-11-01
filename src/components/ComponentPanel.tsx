import { X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { DropInfo } from "../bindings";
import { resolveComponentLocation, resolveComponentPriceLookup, resolveComponentWikiFallback } from "../lib/componentResolution";
import { itemLabel } from "../lib/itemLabel";
import { useItemPrice } from "../lib/itemPrice";
import { dropTableQuery, latestDropTableSources, resolveDrops } from "../lib/resolveDrops";
import type { ModalCtx } from "../modals/types";
import type { ComponentInfo } from "../types/inventory";
import DropsList from "./DropsList";
import MarketPrice from "./MarketPrice";
import WikiButton from "./WikiButton";
import { IconButton } from "./ui";

interface Props {
  ctx: ModalCtx;
  comp: ComponentInfo;
  /** Parent item name: builds the full tradeable name, the wiki fallback and the meta row. */
  parentName?: string;
  onClose?: () => void;
  /** Extra class for the outer .detail-panel ("comp-inline-panel", "comp-panel"). */
  panelClass?: string;
}

export default function ComponentPanel({ ctx, comp, parentName = "", onClose, panelClass = "" }: Props) {
  const { tr, db, relicDb, catalog } = ctx;
  const dbEntry = comp.uniqueName ? db[comp.uniqueName] : null;
  const lookup = useMemo(() => Object.fromEntries((catalog?.items ?? []).map((i) => [i.name.toLowerCase(), { url_name: i.slug }])), [catalog]);
  const price = useItemPrice(resolveComponentPriceLookup(comp, parentName, dbEntry, lookup), catalog);

  // Ingredients such as Orokin Cell carry no drops in the item data.
  const tableQuery = dropTableQuery(comp as never, db as never, relicDb);
  const [tableDrops, setTableDrops] = useState<{ query: string; drops: DropInfo[] } | null>(null);
  useEffect(() => {
    if (!tableQuery) return;
    let live = true;
    void latestDropTableSources((query, drops) => live && setTableDrops({ query, drops }))(tableQuery);
    return () => {
      live = false;
    };
  }, [tableQuery]);
  const compDrops = tableQuery && tableDrops?.query === tableQuery ? tableDrops.drops : resolveDrops(comp as never, db as never, relicDb);

  const parentEntry = dbEntry?.componentOf ? db[dbEntry.componentOf] : null;
  // Keep the English lookup key for prices and the wiki; translate only its matching label.
  const parentLabel = parentEntry && parentEntry.name === parentName ? itemLabel(dbEntry?.componentOf ?? "", parentEntry as never) : parentName;
  const location = resolveComponentLocation(dbEntry);
  const heading = itemLabel(comp.uniqueName ?? "", comp) || tr("detail.unknownComponent");

  return (
    <div className={`detail-panel ${panelClass}`}>
      <div className="detail-panel-top-actions">
        <WikiButton wikiUrl={dbEntry?.wikiaUrl ?? null} fallbackName={resolveComponentWikiFallback(comp, parentName, dbEntry)} tr={tr} />
        {onClose && (
          <IconButton label={tr("common.close")} onClick={onClose}>
            <X />
          </IconButton>
        )}
      </div>

      <div className="detail-header">
        {dbEntry?.imageUrl && (
          <div className="detail-img-wrap">
            <img className="item-img" src={dbEntry.imageUrl} alt={heading} />
          </div>
        )}
        <div className="detail-title-area">
          <h2>{heading}</h2>
          <div className="comp-meta-stack">
            {parentName && <div className="detail-meta">{parentLabel}</div>}
            {comp.tradable && <div className="detail-meta">{tr("detail.tradable")}</div>}
            <div className="detail-meta" title={comp.blueprintHeld ? tr("common.blueprintOwnedNotBuilt") : undefined}>
              {tr("detail.owned", { owned: comp.built ?? comp.ownedCount ?? 0, needed: comp.itemCount || 1 })}
            </div>
          </div>
        </div>
      </div>

      <div className="detail-body">
        {location && <div className="detail-desc">{location}</div>}
        <DropsList drops={compDrops} relicDb={relicDb} relicOwned={ctx.relicOwned} tr={tr} title={tr("detail.dropSources")} />
        <MarketPrice text={price.messageKey ? tr(price.messageKey, price.messageParams) : ""} slug={price.slug} tr={tr} />
      </div>
    </div>
  );
}
