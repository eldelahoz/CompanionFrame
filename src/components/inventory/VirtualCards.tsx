import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useVirtualRows, type RowKind } from "./useVirtualRows";

/**
 * The card grid, a window of rows at a time. The columns and the gap come from the CSS itself (a hidden empty grid of
 * `gridClass` resolves `auto-fill` for the current width), so the breakpoints live in one place. Every card of a grid has
 * the same height in CSS; `kind` names it so the last measured row size is remembered per grid.
 */
export default function VirtualCards<T>({ rows, render, gridClass = "inv-grid", kind = "cards" }: { rows: readonly T[]; render: (row: T, index: number) => ReactNode; gridClass?: string; kind?: RowKind }) {
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
  const v = useVirtualRows(Math.ceil(rows.length / grid.cols), kind, box, grid.gap);
  return (
    <div ref={box} className="inv-virtual" style={{ height: v.getTotalSize() }}>
      <div ref={probe} className={`${gridClass} inv-probe`} aria-hidden="true" />
      {v.getVirtualItems().map((it) => (
        <div
          key={it.key}
          ref={v.measureElement}
          data-index={it.index}
          className={`${gridClass} inv-virtual-row`}
          style={{ gridTemplateColumns: `repeat(${grid.cols}, minmax(0, 1fr))`, transform: `translateY(${it.start - v.options.scrollMargin}px)` }}
        >
          {rows.slice(it.index * grid.cols, (it.index + 1) * grid.cols).map((row, n) => render(row, it.index * grid.cols + n))}
        </div>
      ))}
    </div>
  );
}
