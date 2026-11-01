import { RotateCcw } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { applyCraftingTreeFilters, computeCraftingSummary, type CraftingTreeNode } from "../lib/craftingTree";
import { formatBuildTime } from "../lib/format";
import { itemLabel } from "../lib/itemLabel";
import type { ModalCtx } from "../modals/types";
import CraftingTreeNodeCard from "./CraftingTreeNode";
import ItemImage from "./ItemImage";
import { IconButton, Readout, Switch } from "./ui";

interface Props {
  ctx: ModalCtx;
  tree: CraftingTreeNode;
  onOpenItem?: ((uniqueName: string) => void) | null;
}

export default function CraftingTree({ ctx, tree, onOpenItem = null }: Props) {
  const { tr } = ctx;
  const [hideCompleted, setHideCompleted] = useState(false);
  const [hideBlueprints, setHideBlueprints] = useState(false);
  const [scale, setScale] = useState(1);
  const [panning, setPanning] = useState(false);
  const pan = useRef({ x: 0, y: 0, startX: 0, startY: 0 });
  const panEl = useRef<HTMLDivElement>(null);
  const zoomEl = useRef<HTMLDivElement>(null);
  const canvasEl = useRef<HTMLDivElement>(null);
  const frame = useRef(0);

  const summary = useMemo(() => computeCraftingSummary(tree), [tree]);
  const filters = useMemo(() => ({ hideCompleted, hideBlueprints }), [hideCompleted, hideBlueprints]);
  const visibleTree = useMemo(() => applyCraftingTreeFilters(tree, filters), [tree, filters]);
  const usedFor = tree.usedFor || [];
  const missingBlueprints = summary.blueprints.filter((b) => b.owned < b.count);
  const missingResources = summary.resources.filter((r) => r.owned < r.count);

  const applyPan = () => {
    if (panEl.current) panEl.current.style.transform = `translate(${pan.current.x}px, ${pan.current.y}px)`;
  };
  // Wide trees (necramechs) start unreadably cropped at 100%; fit whenever the footprint changes.
  const fitToView = useCallback(() => {
    const canvas = canvasEl.current;
    const zoom = zoomEl.current;
    if (!canvas || !zoom) return;
    const [cw, ch, tw, th] = [canvas.clientWidth, canvas.clientHeight, zoom.offsetWidth, zoom.offsetHeight];
    if (!cw || !ch || !tw || !th) return;
    const next = Math.max(0.15, Math.min(1, cw / tw, ch / th));
    zoom.style.zoom = String(next);
    setScale(next);
    pan.current.x = Math.max(0, (cw - tw * next) / 2);
    pan.current.y = 0;
    applyPan();
  }, []);
  useLayoutEffect(fitToView, [visibleTree, fitToView]);
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  // React registers wheel listeners as passive, so preventDefault needs a native one.
  useEffect(() => {
    const canvas = canvasEl.current;
    if (!canvas) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      setScale((s) => {
        const next = Math.max(0.15, Math.min(3, s * (e.deltaY > 0 ? 0.9 : 1.1)));
        if (zoomEl.current) zoomEl.current.style.zoom = String(next);
        return next;
      });
    };
    canvas.addEventListener("wheel", onWheel, { passive: false });
    return () => canvas.removeEventListener("wheel", onWheel);
  }, []);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border-subtle px-3 py-2">
        <div className="flex items-center gap-4">
          <Switch label={tr("crafting.hideCompleted")} aria-label={tr("crafting.hideCompletedAria")} checked={hideCompleted} onChange={() => setHideCompleted(!hideCompleted)} />
          <Switch label={tr("crafting.hideBlueprints")} aria-label={tr("crafting.hideBlueprintsAria")} checked={hideBlueprints} onChange={() => setHideBlueprints(!hideBlueprints)} />
        </div>
        <div className="flex items-center gap-1.5">
          <IconButton label={tr("crafting.fitToView")} onClick={fitToView}>
            <RotateCcw />
          </IconButton>
          <span className="w-10 text-center text-xs tabular-nums text-text-muted">{Math.round(scale * 100)}%</span>
        </div>
      </div>

      <div
        ref={canvasEl}
        className={`relative min-h-0 flex-1 touch-none select-none overflow-hidden ${panning ? "cursor-grabbing" : "cursor-grab"}`}
        onPointerDown={(e) => {
          if (e.button !== 0) return;
          setPanning(true);
          pan.current.startX = e.clientX - pan.current.x;
          pan.current.startY = e.clientY - pan.current.y;
          e.currentTarget.setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (!panning) return;
          pan.current.x = e.clientX - pan.current.startX;
          pan.current.y = e.clientY - pan.current.startY;
          if (!frame.current)
            frame.current = requestAnimationFrame(() => {
              frame.current = 0;
              applyPan();
            });
        }}
        onPointerUp={() => setPanning(false)}
        onPointerCancel={() => setPanning(false)}
      >
        <div ref={panEl} className="inline-flex will-change-transform">
          <div ref={zoomEl} className="inline-flex origin-top-left flex-col items-center gap-8 p-8">
            {usedFor.length > 0 && (
              <div className="flex flex-col items-center gap-2">
                <span className="ms-eyebrow">{tr("crafting.usedFor")}</span>
                <div className="flex max-w-[680px] flex-wrap items-start justify-center gap-3">
                  {usedFor.map((usage) => (
                    <button
                      key={usage.uniqueName}
                      type="button"
                      className="flex w-16 cursor-pointer flex-col items-center gap-1 rounded-lg border border-border bg-bg-raised/80 px-3 py-2 text-inherit transition-colors duration-150 hover:border-accent-dim hover:bg-surface-hover focus-visible:outline focus-visible:outline-2 focus-visible:outline-accent"
                      title={tr("common.open", { name: usage.name })}
                      onPointerDown={(e) => e.stopPropagation()}
                      onClick={() => onOpenItem?.(usage.uniqueName)}
                    >
                      <ItemImage src={usage.imageUrl} alt={itemLabel(usage.uniqueName, usage)} cls="h-14 w-14 object-contain" tr={tr} />
                      <span className="max-w-full break-words text-center font-display text-xs font-semibold leading-tight text-text-primary">{itemLabel(usage.uniqueName, usage)}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
            {visibleTree && <CraftingTreeNodeCard ctx={ctx} node={visibleTree} filters={filters} onOpenItem={onOpenItem} onLayoutChange={() => requestAnimationFrame(fitToView)} />}
          </div>
        </div>
      </div>

      <div className="shrink-0 border-t border-border-subtle px-4 py-3 text-xs">
        <div className="grid gap-x-6 gap-y-1" style={{ gridTemplateColumns: "1fr auto" }}>
          <div className="max-h-36 overflow-y-auto pr-1">
            <div className="ms-eyebrow mb-1">{tr("crafting.blueprintsNeeded")}</div>
            {missingBlueprints.length === 0 ? (
              <span className="text-success">{tr("crafting.noBlueprintsMissing")}</span>
            ) : (
              missingBlueprints.map((bp) => (
                <div key={bp.uniqueName} className="text-text-muted">
                  {itemLabel(bp.uniqueName, bp)} ({bp.owned}/{bp.count})
                </div>
              ))
            )}
            <div className="ms-eyebrow mb-1 mt-2">{tr("crafting.resourcesNeeded")}</div>
            {missingResources.length === 0 ? (
              <span className="text-success">{tr("crafting.noResourcesMissing")}</span>
            ) : (
              missingResources.map((res) => (
                <div key={res.uniqueName} className="text-text-muted">
                  {itemLabel(res.uniqueName, res)} ({res.owned}/{res.count})
                </div>
              ))
            )}
          </div>
          <div className="flex flex-col items-end justify-end">
            <div className="px-3 py-1.5 text-xs text-text-secondary">
              <div className="flex items-center gap-2">
                {tr("common.total")}
                <Readout currency="credits" value={summary.totalCredits} />
              </div>
              <div>
                {tr("crafting.minTime")} <strong className="text-text-primary">{formatBuildTime(summary.minBuildTime)}</strong>
              </div>
              <div>
                {tr("crafting.maxTime")} <strong className="text-text-primary">{formatBuildTime(summary.maxBuildTime)}</strong>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
