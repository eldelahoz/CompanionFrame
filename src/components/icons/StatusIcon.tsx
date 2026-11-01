import type { ComponentState } from "../../lib/componentStatus";
import { NavIcon } from "./NavIcon";

// Part-status glyphs in the NavIcon style: angular 1.5 strokes on a 24px grid, Tenno diamonds and hexagons.
const PATHS = {
  // Filled diamond with a chevron cut out of it.
  ready: (
    <>
      <path d="M12 2.8 21.2 12 12 21.2 2.8 12Z" fill="currentColor" />
      <path d="M8 12.2l3 3 5-5.6" stroke="var(--bg-surface)" />
    </>
  ),
  // Diamond with only its lower half filled: some of the parts.
  partial: (
    <>
      <path d="M12 2.8 21.2 12 12 21.2 2.8 12Z" />
      <path d="M2.8 12h18.4L12 21.2Z" fill="currentColor" />
    </>
  ),
  // Empty hexagon split by a broken diagonal: nothing yet.
  missing: (
    <>
      <path d="M12 2.8 19.8 7.3v9.4L12 21.2 4.2 16.7V7.3Z" />
      <path d="M8.5 8.5l2.7 2.7M12.8 12.8l2.7 2.7" />
    </>
  ),
};

/** Glyph for a part state; "build" reuses the foundry glyph. */
export default function StatusIcon({ state }: { state: ComponentState }) {
  if (state === "build") return <NavIcon view="foundry" className="h-4! w-4! [&_.nav-core]:fill-current" />;
  return (
    <svg className="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {PATHS[state]}
    </svg>
  );
}
