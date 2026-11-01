import type { ReactNode } from "react";

// Our own sidebar glyphs: angular 1.5 strokes with Tenno-style diamonds, chevrons and hexagons (24px grid).
// Each has a small "core" that fills with currentColor when its view is active (see .nav-btn in styles/shell.css).
const core = { className: "nav-core" };

const PATHS: Record<string, ReactNode> = {
  dashboard: (
    <>
      <path d="M12 3 15 9l5 3-5 2-1.5 6h-3L9 14l-5-2 5-3Z" />
      <path {...core} d="M12 10l1.5 2-1.5 2-1.5-2Z" />
    </>
  ),
  inventory: (
    <>
      <path d="M12 2.8 16 6l1 7-3 5.5-2 2.7-2-2.7L7 13l1-7Z" />
      <path d="M12 2.8v5.4" />
      <path {...core} d="M9 11.2l3 2 3-2-3 1Z" />
    </>
  ),
  foundry: (
    <>
      <path d="M4.5 7h15l-2.3 8.5H6.8Z" />
      <path d="M9 15.5 8 20.5h8l-1-5" />
      <path d="M10 3.5l1 2M14 3.5l-1 2" />
      <path {...core} d="M9.3 10.3h5.4l-.6 2.3H9.9Z" />
    </>
  ),
  mastery: (
    <>
      <path d="M12 2.8 19.8 7.3v9.4L12 21.2 4.2 16.7V7.3Z" />
      <path d="M8 13.4 12 10.2l4 3.2M8 17.2l4-3.2 4 3.2" />
      <path {...core} d="M12 5.6l1.4 1.4L12 8.4 10.6 7Z" />
    </>
  ),
  stats: (
    <>
      <path d="M3.5 20.5h17" />
      <path d="M5.5 20.5v-6l2.5-2v8M10.75 20.5V9l2.5-2v13.5" />
      <path {...core} d="M16 20.5v-9.5l2.5-2v11.5" />
    </>
  ),
  world: (
    <>
      <circle cx="12" cy="12" r="5" />
      <ellipse cx="12" cy="12" rx="10" ry="3.4" transform="rotate(-22 12 12)" />
      <path {...core} d="M18.4 15.6l1.3 1.3-1.3 1.3-1.3-1.3Z" />
    </>
  ),
  syndicates: (
    <>
      <path d="M4.5 3.5h15" />
      <path d="M6.5 3.5h11V20L12 16.5 6.5 20Z" />
      <path {...core} d="M12 7.2l2.2 2.6L12 12.4 9.8 9.8Z" />
    </>
  ),
  missions: (
    <>
      <path d="M12 2.8v4.4M12 16.8v4.4M2.8 12h4.4M16.8 12h4.4" />
      <path d="M12 7.5 16.5 12 12 16.5 7.5 12Z" />
      <path {...core} d="M12 10.5 13.5 12 12 13.5 10.5 12Z" />
    </>
  ),
  market: (
    <>
      <path d="M3.5 8.5H17l-3-3M20.5 15.5H7l3 3" />
      <path {...core} d="M12 9.8 14.2 12 12 14.2 9.8 12Z" />
    </>
  ),
  analytics: (
    <>
      <path d="M3.5 20.5h17" />
      <path d="M4.5 16l4.5-5 4 3 5-5.8" />
      <path {...core} d="M19.2 4.3l1.5 1.5-1.5 1.5-1.5-1.5Z" />
    </>
  ),
  rivens: (
    <>
      <path d="M12.5 2.8 18.5 10.5 12 21.2 5.5 12.8Z" />
      <path d="M10.6 7.4 13.2 10.3 10.8 12.9 13.6 16.2" />
      <path {...core} d="M15.2 13.2l1 .6-.4 1.3-1-.6Z" />
    </>
  ),
  relics: (
    <>
      <path d="M12 2.8 17.5 7l1 6.5-6.5 7.7-6.5-7.7 1-6.5Z" />
      <path d="M6.5 7 12 10.5 17.5 7M5.5 13.5 12 10.5l6.5 3" />
      <path {...core} d="M12 14.2l1.5 2-1.5 2-1.5-2Z" />
    </>
  ),
  wiki: (
    <>
      <path d="M3.5 5.5 12 7.5l8.5-2v13l-8.5 2-8.5-2Z" />
      <path d="M12 7.5v13" />
      <path d="M6 10.6l3.5.8M6 13.8l3.5.8" />
      <path {...core} d="M16.3 10.2l1.4 1.4-1.4 1.4-1.4-1.4Z" />
    </>
  ),
  arbi: (
    <>
      <path d="M9.2 4.8h5.6l4.4 4.4v5.6l-4.4 4.4H9.2l-4.4-4.4V9.2Z" />
      <path d="M10 2.6h4M12 12V8.2" />
      <path {...core} d="M12 11l1 1-1 1-1-1Z" />
    </>
  ),
  settings: (
    <>
      <path d="M10.5 3h3l.6 2.4 2 .9 2.2-1.3 2.1 2.1-1.3 2.2.9 2 2.4.6v3l-2.4.6-.9 2 1.3 2.2-2.1 2.1-2.2-1.3-2 .9-.6 2.4h-3l-.6-2.4-2-.9-2.2 1.3-2.1-2.1 1.3-2.2-.9-2L3 13.5v-3l2.4-.6.9-2-1.3-2.2 2.1-2.1 2.2 1.3 2-.9Z" />
      <path {...core} d="M12 9.4 14.6 12 12 14.6 9.4 12Z" />
    </>
  ),
};

/** Sidebar glyph for a view; falls back to the dashboard glyph for an unknown id. */
export function NavIcon({ view, className = "" }: { view: string; className?: string }) {
  return (
    <svg className={`nav-icon ${className}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {PATHS[view] ?? PATHS.dashboard}
    </svg>
  );
}

/** CompanionFrame mark: a frame around the companion's point. */
export function BrandMark({ className = "" }: { className?: string }) {
  return (
    // Small app icon (docs/design/icon/icon.svg): ring notched at the four points, teal core.
    <svg className={className} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="7.5" stroke="currentColor" strokeWidth={3} strokeDasharray="9.28 2.5" strokeDashoffset={10.53} />
      <circle cx="12" cy="12" r="3.2" fill="var(--accent)" />
    </svg>
  );
}
