import {
  beginUndoGroup,
  endUndoGroup,
  layoutState,
  moveSection,
  sectionsOf,
} from "../../stores/layout";
import { createSignal } from "../signal";
import { columnOf } from "./plan";
import type { LayoutBreakpoint, LayoutColumn, LayoutView, SectionState } from "./types";

const draggingId = createSignal<string | null>(null);

/** Section the pointer is dragging. Crossing a column destroys and recreates the
    handle, so the grabbing state cannot live inside the component. */
export const useDraggingSectionId = draggingId.use;

interface ActiveDrag {
  view: LayoutView;
  breakpoint: LayoutBreakpoint;
  id: string;
  pointerId: number;
  /** Ids this screen renders; a hit outside them belongs to another grid. */
  scope: readonly string[] | null;
  /** Grid the drag started in, or null for a section rendered outside one. */
  grid: Element | null;
  lastCell: string | null;
}

let active: ActiveDrag | null = null;

interface DropHit {
  id: string;
  after: boolean;
}

function cellOf(hit: DropHit): string {
  return `${hit.id}|${hit.after ? "after" : "before"}`;
}

/** Drop rule for one pointer position. A null target leaves the layout alone;
    landing back on the dragged section forgets the last drop cell so the user
    can re-enter it. */
export function resolveDropTarget(
  drag: { id: string; scope: readonly string[] | null; lastCell: string | null },
  hit: DropHit | null,
  sameGrid: boolean,
): { targetId: string | null; after: boolean; lastCell: string | null } {
  const keep = { targetId: null, after: false, lastCell: drag.lastCell };
  if (!hit) return keep;
  if (hit.id === drag.id) return { targetId: null, after: false, lastCell: null };
  const cell = cellOf(hit);
  if (cell === drag.lastCell || !sameGrid) return keep;
  if (drag.scope && !drag.scope.includes(hit.id)) return keep;
  return { targetId: hit.id, after: hit.after, lastCell: cell };
}

/** The dragged section is spliced out before the insert, so a target below it
    has already shifted up. */
export function dropPlacement(
  sections: readonly SectionState[],
  id: string,
  targetId: string,
  after: boolean,
): { index: number; column: LayoutColumn } | null {
  const from = sections.findIndex((section) => section.id === id);
  const to = sections.findIndex((section) => section.id === targetId);
  const moving = sections[from];
  const landing = sections[to];
  if (!moving || !landing) return null;
  const column = landing.span === 1 ? columnOf(landing) : columnOf(moving);
  const shifted = from < to ? to - 1 : to;
  const index = after ? shifted + 1 : shifted;
  if (index === from && column === columnOf(moving)) return null;
  return { index, column };
}

export function columnPlacement(
  sections: readonly SectionState[],
  id: string,
  column: LayoutColumn,
): { index: number; column: LayoutColumn } | null {
  const from = sections.findIndex((section) => section.id === id);
  const moving = sections[from];
  if (!moving || columnOf(moving) === column) return null;
  return { index: from, column };
}

function isBelowMidpoint(section: Element, clientY: number): boolean {
  const rect = section.getBoundingClientRect();
  if (rect.height <= 0) return false;
  return clientY > rect.top + rect.height / 2;
}

function columnUnder(drag: ActiveDrag, hit: Element | null): LayoutColumn | null {
  if (!drag.grid) return null;
  const column = hit?.closest("[data-layout-column]") ?? null;
  if (!column || column.closest("[data-layout-grid]") !== drag.grid) return null;
  const index = Number(column.getAttribute("data-layout-column"));
  return index === 0 || index === 1 ? index : null;
}

function onPointerMove(event: PointerEvent): void {
  if (!active || event.pointerId !== active.pointerId) return;
  const drag = active;
  const hit = document.elementFromPoint(event.clientX, event.clientY);
  const section = hit?.closest("[data-layout-section]") ?? null;
  const targetId = section?.getAttribute("data-layout-section") ?? null;
  const apply = (placement: { index: number; column: LayoutColumn } | null): void => {
    if (placement) moveSection(drag.view, drag.breakpoint, drag.id, placement);
  };
  const sections = (): SectionState[] => sectionsOf(layoutState.get(), drag.view, drag.breakpoint);

  if (!section || !targetId) {
    const column = columnUnder(drag, hit);
    if (column === null) return;
    const cell = `column|${String(column)}`;
    if (cell === drag.lastCell) return;
    drag.lastCell = cell;
    apply(columnPlacement(sections(), drag.id, column));
    return;
  }

  // A section rendered outside a grid (the stats trade rail) has no grid to
  // compare, so its id scope is the only fence.
  const sameGrid = drag.grid === null || section.closest("[data-layout-grid]") === drag.grid;
  const decision = resolveDropTarget(
    drag,
    { id: targetId, after: isBelowMidpoint(section, event.clientY) },
    sameGrid,
  );
  drag.lastCell = decision.lastCell;
  if (!decision.targetId) return;
  apply(dropPlacement(sections(), drag.id, decision.targetId, decision.after));
}

function onPointerUp(event: PointerEvent): void {
  if (active && event.pointerId !== active.pointerId) return;
  endDrag();
}

// Pointer capture dies with the handle the first move remounts, and without it
// dragging over the page paints a text selection across every section.
function suppressSelection(on: boolean): void {
  document.body.style.userSelect = on ? "none" : "";
}

function endDrag(): void {
  if (!active) return;
  active = null;
  draggingId.set(null);
  suppressSelection(false);
  endUndoGroup();
  window.removeEventListener("pointermove", onPointerMove, true);
  window.removeEventListener("pointerup", onPointerUp, true);
  window.removeEventListener("pointercancel", onPointerUp, true);
  window.removeEventListener("blur", endDrag);
}

/** Runs the whole gesture off window, so the remount a cross-column move causes
    cannot end the drag with the handle that started it. */
export function beginSectionDrag(options: {
  view: LayoutView;
  breakpoint: LayoutBreakpoint;
  id: string;
  pointerId: number;
  scope: readonly string[] | null;
  /** Handle that was grabbed; the drag is fenced to the grid it sits in. */
  from: Element | null;
}): void {
  endDrag();
  if (typeof window === "undefined") return;
  active = {
    view: options.view,
    breakpoint: options.breakpoint,
    id: options.id,
    pointerId: options.pointerId,
    scope: options.scope && options.scope.length > 0 ? options.scope : null,
    grid: options.from?.closest("[data-layout-grid]") ?? null,
    lastCell: null,
  };
  draggingId.set(options.id);
  suppressSelection(true);
  beginUndoGroup(options.view, options.breakpoint, options.id);
  window.addEventListener("pointermove", onPointerMove, true);
  window.addEventListener("pointerup", onPointerUp, true);
  window.addEventListener("pointercancel", onPointerUp, true);
  window.addEventListener("blur", endDrag);
}
