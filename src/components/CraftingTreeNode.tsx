import { Check, ChevronDown, ExternalLink } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { resolveComponentByUniqueName } from "../lib/componentResolution";
import { MAX_EXPAND_DEPTH, canExpandCraftingNode, expandCraftingNode, expandedChildAncestors, filterExpandedChildren, type CraftingTreeFilters, type CraftingTreeNode as TreeNode } from "../lib/craftingTree";
import { formatInt, formatBuildTime, formatNumber } from "../lib/format";
import { itemLabel } from "../lib/itemLabel";
import type { ModalCtx } from "../modals/types";
import { setActiveComponent } from "../stores/modals";
import ItemImage from "./ItemImage";

interface Props {
  ctx: ModalCtx;
  node: TreeNode;
  /** uniqueName path above this node, root first. Guards expansion against loops. */
  ancestors?: string[];
  /** Lazily expanded levels between this node and the eagerly built tree. */
  expandDepth?: number;
  filters?: CraftingTreeFilters;
  /** Same "open the recipe" callback the "Used for crafting" cards fire. */
  onOpenItem?: ((uniqueName: string) => void) | null;
  /** Fired after an expand/collapse changes the tree footprint. */
  onLayoutChange?: () => void;
}

const NO_FILTERS: CraftingTreeFilters = { hideCompleted: false, hideBlueprints: false };

export default function CraftingTreeNodeCard({ ctx, node, ancestors = [], expandDepth = 0, filters = NO_FILTERS, onOpenItem = null, onLayoutChange }: Props) {
  const { tr, db, ownership } = ctx;
  const [expanded, setExpanded] = useState(false);
  // Collapse when the card is reused for a different item.
  useEffect(() => setExpanded(false), [node.uniqueName]);

  // A blueprint you hold is not the part built, so it earns no check here.
  const gotEnough = node.built >= node.count;
  const qtyLabel = node.count >= 1000 ? formatNumber(node.count) : node.count > 1 ? `${node.count}x` : "";
  const hasBuiltChildren = node.children.length > 0;
  const childAncestors = expandedChildAncestors(node, db, ancestors);
  const canExpand = !hasBuiltChildren && canExpandCraftingNode(node, db, ancestors);
  // At the cap the sub-recipe is still reachable, just in its own modal.
  const atExpandCap = canExpand && expandDepth >= MAX_EXPAND_DEPTH;
  const canOpenComponent = !hasBuiltChildren && !canExpand && Boolean(db[node.uniqueName]?.name);
  const interactive = (canExpand && !atExpandCap) || (atExpandCap && !!onOpenItem) || canOpenComponent;
  const cardTitle = !interactive
    ? undefined
    : atExpandCap
      ? tr("common.open", { name: itemLabel(node.uniqueName, node) })
      : canExpand
        ? expanded
          ? tr("crafting.collapseRecipe")
          : tr("crafting.expandRecipe")
        : tr("common.openDetailsFor", { name: itemLabel(node.uniqueName, node) });

  // Nothing resolves until the user opens the node; ownership edits stay live after.
  const expandedChildren = useMemo(() => (expanded && canExpand ? filterExpandedChildren(expandCraftingNode(node, db, ownership, ancestors), filters) : []), [expanded, canExpand, node, db, ownership, ancestors, filters]);
  const shown = hasBuiltChildren ? node.children : expandedChildren;

  const onCardClick = () => {
    if (atExpandCap) {
      onOpenItem?.(node.uniqueName);
      return;
    }
    if (canExpand) {
      setExpanded(!expanded);
      onLayoutChange?.();
      return;
    }
    if (!canOpenComponent) return;
    // The node carries its own uniqueName, so a shared display name cannot send the panel to the wrong component.
    const resolved = resolveComponentByUniqueName(node.uniqueName, db, ownership);
    if (resolved) setActiveComponent(resolved);
  };

  const recipe = node.recipe;
  return (
    <div className="tree-node flex flex-col items-center">
      <div className="flex flex-col items-center px-1">
        <button
          type="button"
          disabled={!interactive}
          aria-expanded={canExpand && !atExpandCap ? expanded : undefined}
          title={cardTitle}
          className={`node-card ct-node-card group/node ${gotEnough ? "is-done" : ""}`}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={onCardClick}
        >
          {qtyLabel && <span className="node-qty ct-badge -left-1 -top-1.5 rounded-sm px-1 text-xs font-semibold leading-snug text-text-primary">{qtyLabel}</span>}
          {gotEnough && (
            <span className="node-check ct-badge -bottom-1 -right-1 h-4 w-4 bg-success text-text-on-accent">
              <Check aria-hidden="true" />
            </span>
          )}
          <ItemImage src={node.imageUrl} alt={itemLabel(node.uniqueName, node)} cls="h-12 w-12 object-contain" tr={tr} />
          {canExpand && interactive && (
            <span className={`node-expand ct-badge -bottom-2 left-1/2 h-4 w-4 -translate-x-1/2 ${expanded && !atExpandCap ? "rotate-180" : ""}`}>
              {atExpandCap ? <ExternalLink aria-hidden="true" /> : <ChevronDown aria-hidden="true" />}
            </span>
          )}
          {recipe && (
            // A span, not a div: only phrasing content is valid inside the button.
            <span className="node-tooltip pointer-events-none absolute -bottom-9 left-1/2 z-10 block -translate-x-1/2 whitespace-nowrap rounded-sm bg-surface-tooltip px-2 py-1 text-xs tabular-nums text-text-primary opacity-0 transition-opacity duration-100 group-hover/node:opacity-100">
              {recipe.buildPrice > 0 && `${formatInt(recipe.buildPrice)} cr`}
              {recipe.buildPrice > 0 && recipe.buildTime > 0 && ", "}
              {recipe.buildTime > 0 && formatBuildTime(recipe.buildTime)}
            </span>
          )}
        </button>
        <span className={`mt-0.5 max-w-[90px] break-words text-center font-display text-xs font-semibold leading-tight text-text-primary ${gotEnough ? "opacity-40" : ""}`}>{itemLabel(node.uniqueName, node)}</span>
      </div>

      {shown.length > 0 && (
        <>
          <div className="mx-auto h-4 w-0.5 bg-border-subtle" />
          <div className="flex items-start">
            {shown.map((child, i) => (
              <div key={child.uniqueName} className="flex flex-col items-center">
                <div className="relative flex h-4 w-full">
                  <div className={`h-0 flex-1 ${i !== 0 ? "border-t-2 border-border-subtle" : ""}`} />
                  <div className="h-full w-0.5 shrink-0 bg-border-subtle" />
                  <div className={`h-0 flex-1 ${i !== shown.length - 1 ? "border-t-2 border-border-subtle" : ""}`} />
                </div>
                <CraftingTreeNodeCard ctx={ctx} node={child} ancestors={childAncestors} expandDepth={hasBuiltChildren ? expandDepth : expandDepth + 1} filters={filters} onOpenItem={onOpenItem} onLayoutChange={onLayoutChange} />
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
