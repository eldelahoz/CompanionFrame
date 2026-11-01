import { useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { Translator } from "../../lib/i18n";
import { planSections } from "../../lib/layout/plan";
import { LAYOUT_NARROW_MAX_PX, type LayoutBreakpoint, type LayoutView } from "../../lib/layout/types";
import { layoutBreakpoint, useSections } from "../../stores/layout";
import { PopoutSoloSection } from "../../stores/popout";
import LayoutSection from "./LayoutSection";

interface Props {
  view: LayoutView;
  tr: Translator;
  /** Renders one section's content by id. */
  children: (sectionId: string) => ReactNode;
  /** Restricts this grid to a subset of the view's sections; a view can host more than one grid. */
  only?: readonly string[] | null;
  /** Sections whose content exists right now; an absent one keeps no empty slot. */
  available?: readonly string[] | null;
  /** Wide-breakpoint track sizes. */
  wideTemplate?: string;
  gapClass?: string;
  /** Gap between sections stacked inside one column. */
  columnGapClass?: string;
  className?: string;
  /** Sections pinned to the top while the rest of the grid scrolls. */
  sticky?: readonly string[];
}

export default function LayoutGrid({
  view,
  tr,
  children,
  only = null,
  available = null,
  wideTemplate = "minmax(0, 1fr) minmax(0, 1fr)",
  gapClass = "gap-x-6",
  columnGapClass = "",
  className = "",
  sticky = [],
}: Props) {
  const soloSectionId = useContext(PopoutSoloSection);
  const ref = useRef<HTMLDivElement>(null);
  // Hidden grids report zero width; retain the last breakpoint until visible.
  const [breakpoint, setBreakpoint] = useState<LayoutBreakpoint>("wide");

  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const measure = () => {
      const width = el.clientWidth;
      if (width <= 0) return;
      const next = width <= LAYOUT_NARROW_MAX_PX ? "narrow" : "wide";
      setBreakpoint(next);
      layoutBreakpoint.set(next);
    };
    measure();
    // Deferred to a frame: resizing from inside the observer callback trips "loop completed with undelivered notifications".
    let frame = 0;
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    });
    observer.observe(el);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, []);

  const sections = useSections(view, breakpoint);
  const scoped = useMemo(() => (only ? sections.filter((s) => only.includes(s.id)) : sections), [sections, only]);
  const order = useMemo(() => scoped.map((s) => s.id), [scoped]);
  // Hidden and unavailable are ignored in solo mode: the user asked for this one section by name.
  const solo = soloSectionId && scoped.some((s) => s.id === soloSectionId) ? soloSectionId : null;
  const rows = useMemo(
    () => (soloSectionId ? [] : planSections(scoped, breakpoint, available ? new Set(available) : undefined)),
    [soloSectionId, scoped, breakpoint, available],
  );
  const stickyClass = (id: string) => (sticky.includes(id) ? "view-sticky-filters" : "");

  return (
    <div
      ref={ref}
      className={`grid ${gapClass} ${className}`}
      style={{ gridTemplateColumns: breakpoint === "wide" && !solo ? wideTemplate : "minmax(0, 1fr)" }}
      data-layout-grid={view}
      data-layout-breakpoint={breakpoint}
      data-layout-solo={solo ?? undefined}
    >
      {solo && (
        <div className="min-w-0" style={{ gridColumn: "1 / -1" }}>
          <LayoutSection view={view} breakpoint={breakpoint} order={[]} id={solo} span="full" collapsed={false} tr={tr}>
            {children(solo)}
          </LayoutSection>
        </div>
      )}

      {rows.map((row, rowIndex) =>
        row.kind === "full" ? (
          <div key={rowIndex} className={`min-w-0 ${stickyClass(row.slot.id)}`} style={{ gridColumn: "1 / -1" }}>
            <LayoutSection view={view} breakpoint={breakpoint} order={order} id={row.slot.id} span={row.slot.span} collapsed={row.slot.collapsed} firstInColumn={row.slot.firstInColumn} tr={tr}>
              {children(row.slot.id)}
            </LayoutSection>
          </div>
        ) : (
          // An empty wide column still renders so a section dragged out of it can be dropped back.
          row.columns.map((column, columnIndex) =>
            column.length > 0 || (breakpoint === "wide" && !solo) ? (
              <div
                key={`${rowIndex}-${columnIndex}`}
                className={`flex min-w-0 flex-col ${columnGapClass}`}
                style={{ gridColumn: columnIndex + 1 }}
                data-layout-column={breakpoint === "wide" ? columnIndex : undefined}
              >
                {column.map((placement) => (
                  <LayoutSection
                    key={placement.id}
                    view={view}
                    breakpoint={breakpoint}
                    order={order}
                    id={placement.id}
                    span={placement.span}
                    collapsed={placement.collapsed}
                    firstInColumn={placement.firstInColumn}
                    className={stickyClass(placement.id)}
                    tr={tr}
                  >
                    {children(placement.id)}
                  </LayoutSection>
                ))}
              </div>
            ) : null,
          )
        ),
      )}
    </div>
  );
}
