import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { InvRow } from "../../lib/inventory/inventoryRows";
import { useVirtualRows } from "./useVirtualRows";

/**
 * The card grid, a window of rows at a time. The columns and the gap come from the CSS itself (a hidden empty `.inv-grid`
 * resolves `auto-fill` for the current width), so the breakpoints live in one place.
 */
export default function VirtualCards({ rows, render }: { rows: readonly InvRow[]; render: (row: InvRow) => ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const probe = useRef<HTMLDivElement>(null);
  const [grid, setGrid] = useState({ cols: 4, gap: 12 });
  useLayoutEffect(() => {
    const el = probe.current;
    if (!el) return;
    const read = () => {
      const cs = getComputedStyle(el);
      const cols = cs.gridTemplateColumns.split(" ").filter((t) => t.endsWith("px")).length || 4; // jsdom resolves no tracks
      const gap = parseFloat(cs.rowGap) || 0;
      setGrid((g) => (g.cols === cols && g.gap === gap ? g : { cols, gap }));
    };
    read();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(read);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const v = useVirtualRows(Math.ceil(rows.length / grid.cols), "cards", box, grid.gap);
  return (
    <div ref={box} className="inv-virtual" style={{ height: v.getTotalSize() }}>
      <div ref={probe} className="inv-grid inv-probe" aria-hidden="true" />
      {v.getVirtualItems().map((it) => (
        <div
          key={it.key}
          ref={v.measureElement}
          data-index={it.index}
          className="inv-grid inv-virtual-row"
          style={{ gridTemplateColumns: `repeat(${grid.cols}, minmax(0, 1fr))`, transform: `translateY(${it.start - v.options.scrollMargin}px)` }}
        >
          {rows.slice(it.index * grid.cols, (it.index + 1) * grid.cols).map(render)}
        </div>
      ))}
    </div>
  );
}
