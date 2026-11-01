import type { ReactNode } from "react";

/**
 * Responsive table shell. Write ordinary thead/tbody rows inside.
 * - mode="scroll" (default): scrolls sideways with the first column pinned.
 * - mode="stack": in a container under ~750px, rows become cards. Mark cells with
 *   data-primary (the item cell), data-actions (row buttons) and data-label="…" (the rest, labelled).
 * Number cells take className="ui-num" (right-aligned, tabular).
 */
export function Table({ mode = "scroll", label, children, className = "" }: { mode?: "scroll" | "stack"; label?: string; children: ReactNode; className?: string }) {
  return (
    <div className={`ui-table-wrap ${className}`} data-mode={mode}>
      <table className="ui-table" aria-label={label}>
        {children}
      </table>
    </div>
  );
}

/** Item cell: thumbnail (image or initials), name and a muted second line. */
export function ItemCell({ name, sub, image }: { name: ReactNode; sub?: ReactNode; image?: string | null }) {
  const initials = typeof name === "string" ? name.split(/\s+/).slice(0, 2).map((w) => w[0]).join("") : "";
  return (
    <div className="ui-item">
      <span className="ui-thumb">{image ? <img src={image} alt="" loading="lazy" /> : initials}</span>
      <div className="min-w-0">
        <div className="ui-item-name">{name}</div>
        {sub && <div className="ui-item-sub">{sub}</div>}
      </div>
    </div>
  );
}
