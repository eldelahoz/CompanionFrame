import { useContext, type ReactNode } from "react";
import { ToggleIcon } from "../CollapsibleSection";
import type { MessageKey, Translator } from "../../lib/i18n";
import { useDraggingSectionId, beginSectionDrag } from "../../lib/layout/drag";
import { nextSpan } from "../../lib/layout/plan";
import { sectionById } from "../../lib/layout/registry";
import type { LayoutBreakpoint, LayoutView, SectionSpan } from "../../lib/layout/types";
import { editMode, moveSection, setCollapsed, setHidden, setSpan } from "../../stores/layout";
import { PopoutSoloSection, isPopoutWindow } from "../../stores/popout";
import OpenInWindowButton from "./OpenInWindowButton";

interface Props {
  view: LayoutView;
  breakpoint: LayoutBreakpoint;
  id: string;
  span: SectionSpan;
  collapsed?: boolean;
  firstInColumn?: boolean;
  /** Section ids this screen renders; a drag ignores a drop outside them. */
  order?: readonly string[];
  /** Extra wrapper classes for a section used outside a grid. */
  className?: string;
  tr: Translator;
  children: ReactNode;
}

const CHIP = "cursor-pointer rounded border border-border px-1.5 py-0.5 text-text-secondary hover:border-accent hover:text-accent";

export default function LayoutSection({ view, breakpoint, id, span, collapsed = false, firstInColumn = false, order = [], className = "", tr, children }: Props) {
  const solo = useContext(PopoutSoloSection);
  const editing = editMode.use() === view;
  const dragging = useDraggingSectionId() === id;
  if (solo && solo !== id) return null; // a section popout renders one section; siblings outside a grid stay out

  const descriptor = sectionById(id);
  const canCollapse = descriptor?.canCollapse === true;
  const canHide = descriptor?.canHide !== false;
  const labelKey: MessageKey = descriptor?.labelKey ?? "common.unknown";
  // Only a collapsed section grows a header, so an untouched layout keeps the view's own markup.
  const showHeader = canCollapse && collapsed;
  // A popout already is the window, so it never offers to open another one.
  const canPopout = descriptor?.canPopout === true && !isPopoutWindow;
  // A section pinned to one span cycles back onto itself: stay visible but disabled rather than lie about being live.
  const spanLocked = nextSpan(span, descriptor?.minSpan) === span;
  // The dragged section brightens so it stays findable while the grid reflows around it.
  const outline = !editing ? "" : `rounded-[var(--radius-md)] outline-dashed outline-offset-2 ${dragging ? "outline-2 outline-accent" : "outline-1 outline-accent/40"}`;

  const move = (direction: "up" | "down") => moveSection(view, breakpoint, id, direction);
  const toggleCollapsed = () => setCollapsed(view, breakpoint, id, !collapsed);

  const onHandleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
      event.preventDefault();
      move("up");
    } else if (event.key === "ArrowDown" || event.key === "ArrowRight") {
      event.preventDefault();
      move("down");
    }
  };

  const onPointerDown = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    const handle = event.currentTarget;
    // Capture only helps until the first move remounts this handle; the drag listens on window.
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // a pointer the browser already released is not worth failing over
    }
    beginSectionDrag({ view, breakpoint, id, pointerId: event.pointerId, scope: order.length > 0 ? order : null, from: handle });
  };

  return (
    <div
      className={`min-w-0 ${className} ${outline}`}
      data-layout-section={id}
      data-layout-dragging={dragging ? "true" : "false"}
      data-layout-span={String(span)}
      data-layout-collapsed={collapsed ? "true" : "false"}
      data-layout-first-in-column={firstInColumn ? "true" : "false"}
    >
      {editing && (
        <div className="mb-1 flex flex-wrap items-center gap-1 rounded-[var(--radius-md)] border border-accent/40 bg-accent/5 px-1.5 py-1 text-xs" data-layout-chrome={id}>
          <button
            type="button"
            className={`${dragging ? "cursor-grabbing border-accent text-accent" : "cursor-grab border-border text-text-secondary"} rounded border px-1 py-0.5 hover:border-accent hover:text-accent`}
            data-layout-handle={id}
            aria-label={tr("layout.moveSection", { label: tr(labelKey) })}
            title={tr("layout.reorderHint")}
            onPointerDown={onPointerDown}
            onKeyDown={onHandleKeyDown}
          >
            <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" aria-hidden="true">
              <circle cx="6" cy="4" r="1.3" />
              <circle cx="10" cy="4" r="1.3" />
              <circle cx="6" cy="8" r="1.3" />
              <circle cx="10" cy="8" r="1.3" />
              <circle cx="6" cy="12" r="1.3" />
              <circle cx="10" cy="12" r="1.3" />
            </svg>
          </button>
          <span className="mr-1 font-semibold text-text-primary">{tr(labelKey)}</span>
          <button type="button" className={CHIP} data-layout-move-up={id} title={tr("layout.moveUp")} aria-label={tr("layout.moveUp")} onClick={() => move("up")}>
            &uarr;
          </button>
          <button type="button" className={CHIP} data-layout-move-down={id} title={tr("layout.moveDown")} aria-label={tr("layout.moveDown")} onClick={() => move("down")}>
            &darr;
          </button>
          <button
            type="button"
            className={`rounded border border-border px-1.5 py-0.5 ${spanLocked ? "cursor-default text-text-muted opacity-60" : "cursor-pointer text-text-secondary hover:border-accent hover:text-accent"}`}
            data-layout-span-cycle={id}
            disabled={spanLocked}
            aria-disabled={spanLocked}
            title={tr(spanLocked ? "layout.widthLocked" : "layout.changeWidth")}
            aria-label={tr(spanLocked ? "layout.widthLocked" : "layout.changeWidth")}
            onClick={() => setSpan(view, breakpoint, id, nextSpan(span, descriptor?.minSpan))}
          >
            {span === "full" ? tr("layout.spanFull") : String(span)}
          </button>
          {canCollapse && (
            <button
              type="button"
              className={CHIP}
              data-layout-collapse={id}
              aria-pressed={collapsed}
              title={tr(collapsed ? "layout.expandSection" : "layout.collapseSection")}
              aria-label={tr(collapsed ? "layout.expandSection" : "layout.collapseSection")}
              onClick={toggleCollapsed}
            >
              &minus;
            </button>
          )}
          {canPopout && <OpenInWindowButton target={{ kind: "section", sectionId: id }} tr={tr} iconSize={12} data-layout-popout={id} className={CHIP} />}
          {canHide && (
            <button
              type="button"
              className="ml-auto cursor-pointer rounded border border-border px-1.5 py-0.5 text-text-secondary hover:border-danger hover:text-danger"
              data-layout-hide={id}
              title={tr("layout.hideSection")}
              onClick={() => setHidden(view, breakpoint, id, true)}
            >
              {tr("layout.hideSection")}
            </button>
          )}
        </div>
      )}

      {showHeader && (
        <button
          type="button"
          className="flex w-full cursor-pointer items-center gap-1 border-0 bg-transparent py-1 text-left text-inherit"
          data-layout-collapse-header={id}
          aria-expanded={!collapsed}
          onClick={toggleCollapsed}
        >
          <ToggleIcon collapsed={collapsed} />
          <span className="text-sm font-semibold text-text-primary">{tr(labelKey)}</span>
        </button>
      )}

      {!(canCollapse && collapsed) && children}
    </div>
  );
}
