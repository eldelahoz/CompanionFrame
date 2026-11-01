import { ArrowLeft, Network, X } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import DropsList from "../components/DropsList";
import ComponentPanel from "../components/ComponentPanel";
import ComponentStatusList from "../components/ComponentStatusList";
import CraftingTree from "../components/CraftingTree";
import ItemImage from "../components/ItemImage";
import MarketPrice from "../components/MarketPrice";
import WikiButton from "../components/WikiButton";
import { Button, Chip, IconButton } from "../components/ui";
import BaroLastSeen from "../components/world/BaroLastSeen";
import { enrichComponents, resolveItemPriceLookup } from "../lib/componentResolution";
import { buildCraftingTree } from "../lib/craftingTree";
import { itemLabel } from "../lib/itemLabel";
import { useItemPrice } from "../lib/itemPrice";
import { buildParsedItemFromDb } from "../lib/parsedItemFromDb";
import { resolveDrops } from "../lib/resolveDrops";
import { buildBaroCatalog } from "../lib/world/baroPlanner";
import { useBaroHistory, useBaroWishlist } from "../stores/baro";
import { relicGroupForUniqueName } from "../lib/relic/relicInventory";
import { relicHasRewards } from "../lib/relic/relicView";
import { openRelicDetailed, setActiveItem } from "../stores/modals";
import { useWorldState } from "../stores/world";
import type { ComponentInfo, ParsedItem } from "../types/inventory";
import DetailModalBase from "./DetailModalBase";
import RelicViewSwitch from "./RelicViewSwitch";
import type { ModalCtx } from "./types";

interface Props {
  ctx: ModalCtx;
  item: ParsedItem;
}

const compKey = (c: ComponentInfo) => c.uniqueName || c.name;

export default function ItemDetailModal({ ctx, item }: Props) {
  const { tr, db, ownership, relicDb, catalog } = ctx;
  const itemKey = item.uniqueName || item.internalName || "";
  const dbEntry = itemKey ? db[itemKey] : undefined;
  const [selected, setSelected] = useState<ComponentInfo | null>(null);
  const [showTree, setShowTree] = useState(false);
  const [stack, setStack] = useState<Array<{ item: ParsedItem; showTree: boolean }>>([]);
  // Set by in-modal navigation so the item change that follows keeps the stack and the tree view.
  const nav = useRef<{ tree: boolean | null } | null>(null);

  useEffect(() => {
    setSelected(null);
    if (nav.current) setShowTree(nav.current.tree ?? false);
    else {
      setStack([]);
      setShowTree(false);
    }
    nav.current = null;
  }, [itemKey]);

  const lookup = useMemo(() => Object.fromEntries((catalog?.items ?? []).map((i) => [i.name.toLowerCase(), { url_name: i.slug }])), [catalog]);
  const price = useItemPrice(resolveItemPriceLookup(item, lookup), catalog);

  const world = useWorldState().data;
  const history = useBaroHistory(world?.voidTrader ?? null);
  const { wishlist, setQuantity } = useBaroWishlist();
  const baro = useMemo(() => (itemKey ? buildBaroCatalog(history, world?.voidTrader, null, db as never, wishlist, Date.now()).find((r) => r.uniqueName === itemKey) : undefined), [history, world, db, wishlist, itemKey]);

  const drops = useMemo(() => resolveDrops({ drops: item.drops as never, uniqueName: itemKey }, db as never, relicDb), [item, itemKey, db, relicDb]);
  const parentUniqueName = dbEntry?.isBuildComponent ? (dbEntry.componentOf ?? null) : null;
  const parentEntry = parentUniqueName ? db[parentUniqueName] : null;
  // Blueprints have no recipe; root the tree at the product they build.
  const treeRootKey = dbEntry?.recipe ? itemKey : dbEntry?.buildsProduct && db[dbEntry.buildsProduct]?.recipe ? dbEntry.buildsProduct : null;
  const craftingTree = useMemo(() => (treeRootKey && showTree ? buildCraftingTree(treeRootKey, db, ownership) : null), [treeRootKey, showTree, db, ownership]);
  const components = useMemo(() => enrichComponents(item.components || [], ownership, db, itemKey), [item, ownership, db, itemKey]);
  const selectedComp = selected ? (components.find((c) => compKey(c) === compKey(selected)) ?? null) : null;

  const close = () => setActiveItem(null);
  const goTo = (next: ParsedItem, tree: boolean | null) => {
    setStack((s) => [...s, { item, showTree }]);
    nav.current = { tree };
    setActiveItem(next);
  };
  const onModalClose = () => (selectedComp ? setSelected(null) : showTree ? setShowTree(false) : close());
  const openCraftingTreeItem = (uniqueName: string) => {
    const entry = db[uniqueName];
    if (entry) goTo(buildParsedItemFromDb(uniqueName, entry, ownership), !!entry.recipe);
  };
  const goBack = () => {
    const previous = stack[stack.length - 1];
    if (!previous) return;
    setStack(stack.slice(0, -1));
    nav.current = { tree: previous.showTree };
    setActiveItem(previous.item);
  };

  const relicGroup = relicGroupForUniqueName(relicDb, itemKey);
  const relicBreakdown = relicGroup && relicHasRewards(relicGroup) ? relicGroup : null;
  const label = itemLabel(itemKey, item);
  const inWishlist = (wishlist[itemKey] ?? 0) > 0;
  const topActions = (
    <div className="detail-panel-top-actions" data-item-detail>
      {stack.length > 0 && (
        <Button size="sm" icon={<ArrowLeft />} onClick={goBack}>
          {tr("detail.back")}
        </Button>
      )}
      {treeRootKey && (
        <Button size="sm" icon={<Network />} aria-pressed={showTree} onClick={() => setShowTree(!showTree)}>
          {showTree ? tr("detail.backToDetails") : tr("detail.craftingTree")}
        </Button>
      )}
      {relicBreakdown && <RelicViewSwitch view="simple" onSwitch={() => openRelicDetailed(relicBreakdown)} tr={tr} />}
      <WikiButton wikiUrl={item.wikiaUrl} fallbackName={item.name} tr={tr} />
      <IconButton label={tr("common.close")} onClick={close}>
        <X />
      </IconButton>
    </div>
  );

  return (
    <DetailModalBase
      ariaLabel={label}
      tr={tr}
      onClose={onModalClose}
      sideState={selectedComp ? "component" : "none"}
      panelClass={showTree ? "w-[90vw] max-w-[1100px]" : ""}
      sidePanel={selectedComp && !showTree ? <ComponentPanel ctx={ctx} comp={selectedComp} parentName={item.name} panelClass="comp-inline-panel" onClose={() => setSelected(null)} /> : undefined}
    >
      {topActions}

      {showTree && craftingTree ? (
        <>
          <div className="flex items-center gap-3 border-b border-border-subtle px-4 py-2">
            <div className="h-10 w-10 shrink-0">
              <ItemImage src={item.imageUrl} fallback={item.fallbackImageUrl} alt={label} cls="h-10 w-10 object-contain" tr={tr} />
            </div>
            <div>
              <h2 className="m-0 font-display text-base font-bold text-text-primary">{label}</h2>
              <span className="text-xs text-text-muted">{tr("detail.craftingTree")}</span>
            </div>
          </div>
          <div className="flex h-[60vh] min-h-[300px] flex-col">
            <CraftingTree ctx={ctx} tree={craftingTree} onOpenItem={openCraftingTreeItem} />
          </div>
        </>
      ) : (
        <>
          <div className="detail-header">
            <div className="detail-img-wrap">
              <ItemImage src={item.imageUrl} fallback={item.fallbackImageUrl} alt={label} cls="item-img" tr={tr} />
            </div>
            <div className="detail-title-area">
              <h2>{label}</h2>
              <div className="detail-tags">
                {item.isPrime && <span className="ui-badge ui-badge--accent">{tr("common.prime")}</span>}
                {item.vaulted && <span className="ui-badge ui-badge--warning">{tr("common.vaulted")}</span>}
                {item.status === "mastered" && (
                  <span className="ui-badge ui-badge--success" data-item-mark="mastered" title={tr("common.mastered")}>
                    {tr("common.mastered")}
                  </span>
                )}
                {item.status === "progress" && <span className="ui-badge ui-badge--info">{tr("common.inProgress")}</span>}
                {item.status === "missing" && <span className="ui-badge ui-badge--danger">{tr("common.missing")}</span>}
                {(item.categoryLabel || item.category) && (
                  <span className="detail-meta-inline" data-detail-category={item.category}>
                    {item.categoryLabel || item.category}
                  </span>
                )}
                {parentEntry?.name && (
                  <Button
                    size="sm"
                    variant="ghost"
                    title={tr("common.open", { name: itemLabel(parentUniqueName ?? "", parentEntry as never) })}
                    onClick={() => parentUniqueName && goTo(buildParsedItemFromDb(parentUniqueName, parentEntry, ownership), null)}
                  >
                    {tr("common.partOf", { name: itemLabel(parentUniqueName ?? "", parentEntry as never) })}
                  </Button>
                )}
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <span className="text-xs text-text-muted">{tr("marketAlerts.kindBaro")}:</span>
                <BaroLastSeen uniqueName={itemKey} available={baro?.available ?? false} lastRecorded={baro?.lastSeen ?? null} tr={tr} locale={ctx.locale} />
                {itemKey.startsWith("/Lotus/") && (
                  <Chip pressed={inWishlist} title={tr("baro.addItem", { name: label })} data-baro-wishlist={itemKey} onClick={() => setQuantity(itemKey, inWishlist ? 0 : 1)}>
                    {tr("baro.wishlist")}
                  </Chip>
                )}
              </div>
              {item.description && <div className="detail-desc detail-desc-header">{item.description}</div>}
            </div>
          </div>

          <div className="detail-body">
            {components.length > 0 && (
              <div className="detail-section">
                <h3>{tr("detail.components")}</h3>
                <ComponentStatusList components={components} selected={selectedComp} db={db} relicDb={relicDb} tr={tr} onSelect={setSelected} />
              </div>
            )}

            {(item.modularParts || []).length > 0 && (
              <div className="detail-section">
                <h3>{tr("detail.components")}</h3>
                <ul className="m-0 list-none p-0 text-sm text-text-secondary">
                  {(item.modularParts || []).map((part, i) => (
                    <li key={`${part}-${i}`} className="border-b border-dashed border-border-subtle py-1.5 last:border-b-0">
                      {part}
                    </li>
                  ))}
                </ul>
              </div>
            )}

            <DropsList drops={drops} relicDb={relicDb} relicOwned={ctx.relicOwned} tr={tr} />
            <MarketPrice text={price.messageKey ? tr(price.messageKey, price.messageParams) : ""} slug={price.slug} tr={tr} />
          </div>
        </>
      )}
    </DetailModalBase>
  );
}
